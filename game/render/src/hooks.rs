//! Game-owned passes in an engine-owned frame.
//! @ref llp/1046.006.000-render-hooks.rfc.md#d1-the-shape-a-type-parameter-like-presentation
use crate::{FrameInput, RenderError};
use exact_game::{Component, Entity, Mat4, Ref, Resource, Target, Transform, World};
use exact_gpu::wgpu;

pub(crate) mod budget;
pub(crate) mod gpu_timing;
mod materials;
pub(crate) mod metrics;
mod pipelines;
mod targets;
pub(crate) use materials::MaterialBindings;
pub use materials::{CustomMaterial, MaterialGpu, MATERIAL_SHADOWS_WGSL, MATERIAL_WGSL};
pub use pipelines::Pipelines;
pub(crate) use targets::{FrameBinding, HookTargets};

/// The engine's shared frame uniform and lighting helpers. Append game entry points.
pub const FRAME_WGSL: &str = include_str!("shaders/frame.wgsl");

/// Reflected frame-uniform interface, generated from [`FRAME_WGSL`].
pub mod frame_interface {
    include!(concat!(env!("OUT_DIR"), "/frame_interface.rs"));
}

/// Restricted borrowing of simulation values. No mutable query or raw World escape.
/// `&World` itself permits mutation, so it must not cross this boundary.
pub struct RenderWorld<'a>(pub(crate) &'a World);
impl<'a> RenderWorld<'a> {
    /// Read one component by entity or name.
    pub fn get<C: Component>(&self, target: impl Target) -> Option<Ref<'a, C>> {
        self.0.get(target)
    }
    /// Read an optional resource.
    pub fn resource<R: Resource>(&self) -> Option<Ref<'a, R>> {
        self.0.try_resource::<R>()
    }
    /// Visit one component column in stable entity order, with immutable values.
    pub fn for_each<C: Component>(&self, mut visit: impl FnMut(Entity, &C)) {
        for (entity, value) in self.0.query::<&C>().iter() {
            visit(entity, value);
        }
    }
    /// Whether the stock renderer draws `entity`: `World::is_visible`, false
    /// when it or any Parent ancestor is `Visible(false)`. Hooks that draw on
    /// behalf of entities should skip hidden ones the same way.
    pub fn is_visible(&self, entity: Entity) -> bool {
        self.0.is_visible(entity)
    }
    /// Resolve a named entity, including its generation.
    pub fn named(&self, name: &str) -> Option<Entity> {
        self.0.named(name)
    }
    /// Completed simulation tick.
    pub fn tick(&self) -> u64 {
        self.0.tick()
    }
    /// Simulation ticks per second.
    pub fn hz(&self) -> u32 {
        self.0.hz()
    }
    /// Current global transform; displayed poses are supplied by FrameView.
    pub fn global(&self, entity: Entity) -> Option<Mat4> {
        self.0.global(entity).map(Mat4::from)
    }
}

/// Optional attachments and presentation scheduling for the current frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Needs(u8);
impl Needs {
    /// No optional work.
    pub const NONE: Self = Self(0);
    /// Opaque HDR/depth snapshot and a multisampled refracting-surface stage.
    pub const SCENE_COPY: Self = Self(1);
    /// HDR input/output before bloom and tone.
    pub const HDR_POST: Self = Self(2);
    /// Sampleable final scene depth for post, independently of SCENE_COPY.
    pub const FINAL_DEPTH: Self = Self(4);
    /// Request another visible frame even when the simulation is paused.
    pub const ANIMATE: Self = Self(8);
    /// Presentation resources are still preparing. Does not stop simulation ticks.
    pub const PENDING: Self = Self(16);
    /// Pending only for want of allowance. A redraw at the same time cannot help.
    /// Has effect only with `PENDING`.
    pub const STARVED: Self = Self(32);
    /// Whether all the requested bits are present.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub(crate) const fn attachments(self) -> Self {
        Self(self.0 & 7)
    }
}
impl std::ops::BitOr for Needs {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A live frame's time limit for hook preparation (LLP 1182.000 D6).
#[derive(Clone, Copy, Debug)]
pub struct Deadline {
    start: crate::perf::Stamp,
    offset_ms: f64,
}
impl Deadline {
    pub(crate) fn new(start: crate::perf::Stamp, offset_ms: f64) -> Self {
        Self { start, offset_ms }
    }

    pub(crate) fn remaining_at(&self, now: crate::perf::Stamp) -> f64 {
        self.offset_ms - self.start.elapsed_at(now)
    }

    /// Whether the frame has reached its preparation deadline.
    pub fn passed(&self) -> bool {
        self.remaining_at(crate::perf::Stamp::now()) <= 0.
    }

    /// Milliseconds left for preparation, or zero after the deadline.
    pub fn remaining_ms(&self) -> f64 {
        self.remaining_at(crate::perf::Stamp::now()).max(0.)
    }
}

/// Host clock and lifecycle facts, separate from saved game time.
///
/// ```compile_fail,E0616
/// use exact_game_render::HookTime;
/// let _ = HookTime::default().serial;
/// ```
#[derive(Clone, Copy, Debug)]
pub struct HookTime {
    /// Identifies this prepare call within the canvas, including held frames.
    pub(crate) serial: u64,
    /// Bounded units this call to prepare may complete.
    pub allowance: u32,
    /// The preparation deadline for a live canvas with a known frame period.
    pub deadline: Option<Deadline>,
    /// Seconds since the last draw; zero on resets, clamped to 100 ms on live gaps.
    pub dt: f32,
    /// Histories must be primed before this draw.
    pub reset: bool,
    /// The agent owns time, rather than the display.
    pub seekable: bool,
    /// Simulation is paused; presentation may independently animate.
    pub paused: bool,
    /// Logical dimensions in CSS points.
    pub points: (f32, f32),
    /// Device pixels per logical point.
    pub scale: f32,
    /// Shader asset generation; candidate pipelines replace valid ones atomically.
    pub shader_generation: u32,
    /// Milliseconds since this canvas's previous drawn frame; 0 for its first
    /// and for the first after a rest or a look. With `display_period_ms`, what
    /// an adaptive render-scale controller or a frame budget steers by.
    pub interval_ms: f64,
    /// The display's period as the host knows it; 0 while unknown or headless.
    pub display_period_ms: f64,
    /// The host's display callback for this frame.
    pub host: exact_gpu::HostTiming,
    /// The device's thermal state (`Unknown` where the host has no cheap source).
    pub thermal_state: exact_gpu::ThermalState,
}
impl Default for HookTime {
    fn default() -> Self {
        Self {
            serial: 0,
            allowance: 0,
            deadline: None,
            dt: 0.,
            reset: true,
            seekable: true,
            paused: false,
            points: (1., 1.),
            scale: 1.,
            shader_generation: 0,
            interval_ms: 0.,
            display_period_ms: 0.,
            host: Default::default(),
            thermal_state: Default::default(),
        }
    }
}

/// The camera, lights, interpolation and attachment dimensions of this exact draw.
pub struct FrameView<'a> {
    /// Engine inputs used by its own pipelines, including view/projection and time.
    pub frame: &'a FrameInput<'a>,
    /// Host clock/lifecycle facts.
    pub time: HookTime,
    /// Active pixel rectangle, starting at (0, 0).
    pub pixels: (u32, u32),
    /// Allocation extent; sampling must respect the active rectangle.
    pub capacity: (u32, u32),
    /// Forward colour format; post outputs use the same HDR format.
    pub color_format: wgpu::TextureFormat,
    /// Forward depth format.
    pub depth_format: wgpu::TextureFormat,
    /// Geometry pipeline sample count. Fullscreen post uses one sample.
    pub sample_count: u32,
    pub(crate) poses: &'a Poses,
}
impl FrameView<'_> {
    /// Displayed transform of an entity registered through Hooks::entities.
    pub fn pose(&self, entity: Entity) -> Option<Mat4> {
        if let Some(a) = self.frame.attachments.iter().find(|a| a.entity == entity) {
            return Some(a.matrix);
        }
        self.poses.at(entity, self.frame.alpha)
    }
    /// Projection inverse, valid for both perspective and orthographic cameras.
    pub fn inverse_projection(&self) -> Mat4 {
        self.frame.proj.inverse()
    }
}

/// Borrowed GPU access for preparing resources; the engine owns command submission.
///
/// ```compile_fail,E0451
/// use exact_game_render::HookGpu;
/// fn rebuild<'a>(gpu: &HookGpu<'a>) -> HookGpu<'a> {
///     HookGpu {
///         materials: None,
///         device: gpu.device,
///         queue: gpu.queue,
///         frame_binding: gpu.frame_binding,
///         frame_layout: gpu.frame_layout,
///     }
/// }
/// ```
pub struct HookGpu<'a> {
    /// Optional model material support, present for an assets-enabled game.
    pub materials: Option<MaterialGpu<'a>>,
    /// Current device; handles must be discarded on device loss.
    pub device: &'a wgpu::Device,
    /// Uniform/buffer uploads for this frame.
    pub queue: &'a wgpu::Queue,
    /// Engine frame uniform at group 0, binding 0, matching FRAME_WGSL.
    pub frame_binding: &'a wgpu::BindGroup,
    /// Reflected layout of frame_binding.
    pub frame_layout: &'a wgpu::BindGroupLayout,
    _prepare: PrepareGpu,
}

struct PrepareGpu;

impl<'a> HookGpu<'a> {
    pub(crate) fn new(
        materials: Option<MaterialGpu<'a>>,
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
        frame_binding: &'a wgpu::BindGroup,
        frame_layout: &'a wgpu::BindGroupLayout,
    ) -> Self {
        Self {
            materials,
            device,
            queue,
            frame_binding,
            frame_layout,
            _prepare: PrepareGpu,
        }
    }
}

/// Immutable pre-surface inputs. Linear depth is positive view-space Z; zero is sky.
pub struct SceneCopy<'a> {
    /// Resolved linear HDR colour, distinct from the continuation's resolve target.
    pub color: &'a wgpu::TextureView,
    /// R32Float nearest-sample linear view depth. Sample with textureLoad.
    pub depth: &'a wgpu::TextureView,
}
/// Fullscreen post inputs. Output must be completely written over the active rectangle.
pub struct PostInputs<'a> {
    /// Final resolved linear HDR scene.
    pub input: &'a wgpu::TextureView,
    /// Separate linear HDR output consumed by both bloom and tone.
    pub output: &'a wgpu::TextureView,
    /// Final linear depth, only when FINAL_DEPTH was requested.
    pub depth: Option<&'a wgpu::TextureView>,
    /// Earlier pre-surface depth, only when SCENE_COPY was requested.
    pub opaque_depth: Option<&'a wgpu::TextureView>,
}

/// Cumulative game-reported GPU creation work; separate from engine-observed work.
#[derive(Clone, Copy, Debug, Default)]
pub struct HookWork {
    /// Pipeline creations.
    pub pipelines: u64,
    /// Buffer creations or replacements.
    pub buffers: u64,
    /// Texture creations or replacements.
    pub textures: u64,
}
impl HookWork {
    pub(crate) fn json(self) -> String {
        format!(
            "{{\"pipelines\":{},\"buffers\":{},\"textures\":{}}}",
            self.pipelines, self.buffers, self.textures
        )
    }
    pub(crate) fn since(self, old: Self) -> Self {
        Self {
            pipelines: self.pipelines.saturating_sub(old.pipelines),
            buffers: self.buffers.saturating_sub(old.buffers),
            textures: self.textures.saturating_sub(old.textures),
        }
    }
}

/// Game-reported counters cumulative since this hook value was made (LLP 1182.000 D1).
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameWork {
    /// Draw calls issued by the hook in opaque, background, surface, and post.
    pub draws: u64,
    /// Submitted triangles, including instances, before culling or clipping.
    pub triangles: u64,
    /// Bytes passed to `write_buffer`, `write_texture`, and `create_buffer_init`.
    pub upload_bytes: u64,
    /// Game-defined items that issued a draw, counted once per frame across layers.
    pub items: u64,
}

/// Optional game rendering. Every callback borrows resources for this frame only.
/// @ref llp/1046.006.000-render-hooks.rfc.md#d1-the-shape-a-type-parameter-like-presentation
pub trait Hooks: Default {
    /// Compile away all hook preparation for the default implementation.
    const ENABLED: bool = true;
    /// Maximum bounded units per advancing frame, or zero to opt out.
    const ALLOWANCE: u32 = 0;
    /// Job types this GPU module can dispatch in a worker.
    const REMOTE: &'static [crate::RemoteJob] = &[];
    /// Allocate/resize resources, extract immutable world data, and upload uniforms.
    fn prepare(
        &mut self,
        _gpu: &HookGpu<'_>,
        _view: &FrameView<'_>,
        _world: &RenderWorld<'_>,
    ) -> Result<(), RenderError> {
        Ok(())
    }
    /// Optional resources and independent demand for another frame, read after prepare.
    fn needs(&self) -> Needs {
        Needs::NONE
    }
    /// Whether callbacks can draw while a replacement is pending.
    fn drawable(&self) -> bool {
        true
    }
    /// Entity histories retained at tick boundaries. New registrations prime current/current.
    fn entities(&self) -> &[Entity] {
        &[]
    }
    /// A named readiness reason, while PENDING is set.
    fn pending_reason(&self) -> &str {
        "render hook resources pending"
    }
    /// A JSON object of the game's own presentation counters, read with `state`. Diagnostics only.
    fn report(&self) -> Option<&str> {
        None
    }
    /// Presentation job gauges and cumulative counts, read with `state`.
    fn jobs(&self) -> Option<crate::JobsReport> {
        None
    }
    /// Last rejected shader/pipeline update, even if a previous set still draws.
    fn error(&self) -> Option<&str> {
        None
    }
    /// Cumulative work performed through the raw GPU handles.
    fn work(&self) -> Option<HookWork> {
        None
    }
    /// Cumulative draw and upload counts, read after the last hook stage of a frame.
    fn frame_work(&self) -> Option<FrameWork> {
        None
    }
    /// Name of the game-defined unit counted by `FrameWork::items`.
    fn item_name(&self) -> &'static str {
        "items"
    }
    /// Custom shading for loaded unskinned opaque model materials.
    fn materials(&self) -> &[CustomMaterial] {
        &[]
    }
    /// Extra per-instance word; presentation-only and never saved.
    fn instance_data(&self, _transform: u32) -> u32 {
        0
    }
    /// Compute/offscreen passes, after skinning and before shadows.
    fn compute(
        &mut self,
        _encoder: &mut wgpu::CommandEncoder,
        _view: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        Ok(())
    }
    /// Additional opaque geometry. Depth writes are required for occluding geometry.
    fn opaque(
        &mut self,
        _pass: &mut wgpu::RenderPass<'_>,
        _view: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        Ok(())
    }
    /// Background content after sky, before the snapshot. The game owns classification
    /// for transparent content that belongs behind its refractor.
    fn background(
        &mut self,
        _pass: &mut wgpu::RenderPass<'_>,
        _view: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        Ok(())
    }
    /// Refracting geometry in the 4× continuation, with writable depth.
    fn surface(
        &mut self,
        _pass: &mut wgpu::RenderPass<'_>,
        _scene: &SceneCopy<'_>,
        _view: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        Ok(())
    }
    /// Linear HDR effects, before bloom and the engine's tone curve.
    fn post(
        &mut self,
        _encoder: &mut wgpu::CommandEncoder,
        _inputs: &PostInputs<'_>,
        _view: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        Ok(())
    }
    /// Drawn frames per second at most, 0 for none: a budget or a thermal cap
    /// (`FrameView.time.thermal_state`). The lower of this and the game's
    /// `Game::max_fps` stands, either 0 for none.
    fn max_fps(&self) -> u32 {
        0
    }
    /// Drop device-owned state immediately; the next prepare receives reset=true.
    fn device_lost(&mut self) {
        *self = Self::default();
    }
}
impl Hooks for () {
    const ENABLED: bool = false;
}

#[derive(Default)]
pub(crate) struct Poses {
    entries: Vec<(Entity, [Transform; 2])>,
    tick: Option<u64>,
    generation: u64,
    parent_revision: u64,
}
impl Poses {
    pub fn track(&mut self, entities: &[Entity], world: &World) {
        let poses = std::cell::LazyCell::new(|| crate::world::scene::PoseRows::new(world));
        self.entries.retain(|(e, _)| entities.contains(e));
        for &e in entities {
            if self.entries.iter().any(|(old, _)| *old == e) {
                continue;
            }
            if let Some(p) = poses.pose(e) {
                self.entries.push((e, [p; 2]));
            }
        }
    }
    pub fn sync(&mut self, world: &World) {
        let poses = std::cell::LazyCell::new(|| crate::world::scene::PoseRows::new(world));
        let reset = self.generation != world.presentation_generation();
        self.generation = world.presentation_generation();
        let parent_revision = world.revision::<exact_game::Parent>();
        let parent_changed = self.parent_revision != parent_revision;
        self.parent_revision = parent_revision;
        let next = self.tick != Some(world.tick());
        self.tick = Some(world.tick());
        self.entries.retain_mut(|(e, pair)| {
            let Some(pose) = poses.pose(*e) else {
                return false;
            };
            if next {
                pair[0] = pair[1];
            }
            pair[1] = pose;
            if reset || poses.snap(*e, parent_changed) {
                pair[0] = pose;
            }
            true
        });
    }
    fn at(&self, e: Entity, alpha: f32) -> Option<Mat4> {
        let (_, pair) = self.entries.iter().find(|(id, _)| *id == e)?;
        let pose = crate::world::scene::interpolate(*pair, alpha);
        Some(Mat4::from_scale_rotation_translation(
            pose.scale,
            pose.rotation,
            pose.position,
        ))
    }
}

#[cfg(test)]
mod pose_tests {
    use super::*;
    use crate::world::scene::resolution_count;

    #[test]
    fn hook_feeds_resolve_pose_columns_once() {
        let mut world = World::new(60, 0);
        let mut poses = Poses::default();
        let before = resolution_count();
        poses.track(&[], &world);
        poses.sync(&world);
        assert_eq!(resolution_count() - before, 0);
        let entities: Vec<_> = (0..16)
            .map(|i| world.spawn(Transform::at(i as f32, 0., 0.)))
            .collect();
        world.propagate();
        let before = resolution_count();
        poses.track(&entities, &world);
        assert_eq!(resolution_count() - before, 1);
        for _ in 0..2 {
            let before = resolution_count();
            poses.sync(&world);
            assert_eq!(resolution_count() - before, 1);
            assert_eq!(poses.entries.len(), entities.len());
        }
        for (i, &e) in entities.iter().enumerate() {
            assert_eq!(poses.at(e, 0.5).unwrap().w_axis.x, i as f32);
        }
    }
}

#[derive(Default)]
pub(crate) struct HookClock {
    serial: u64,
    last: Option<(f64, bool)>,
    generation: Option<u64>,
    pub budget: Option<Box<budget::Budget>>,
}
impl HookClock {
    pub fn reset(&mut self) {
        self.last = None;
    }
    pub fn frame(
        &mut self,
        frame: &exact_gpu::Frame,
        timing: &exact_gpu::FrameTiming,
        generation: u64,
        paused: bool,
    ) -> HookTime {
        let reset = self.generation != Some(generation)
            || self.last.is_none_or(|(t, seekable)| {
                frame.now_ms < t || frame.seekable != seekable || frame.now_ms - t > 1000.
            });
        let dt = if reset {
            0.
        } else {
            self.last.map_or(0., |(t, _)| {
                ((frame.now_ms - t) / 1000.).clamp(0., 0.1) as f32
            })
        };
        self.last = Some((frame.now_ms, frame.seekable));
        self.generation = Some(generation);
        self.serial = self.serial.wrapping_add(1);
        HookTime {
            serial: self.serial,
            allowance: 0,
            deadline: None,
            dt,
            reset,
            paused,
            seekable: frame.seekable,
            points: (frame.width, frame.height),
            scale: frame.scale,
            shader_generation: frame.shader_generation,
            interval_ms: timing.interval_ms,
            display_period_ms: timing.display_period_ms,
            host: timing.host,
            thermal_state: timing.thermal_state,
        }
    }
    /// A look frame's time (LLP 1127.000): `frame` held, no time passing, and
    /// the same timing history for the next live frame.
    pub fn held(&mut self, frame: &exact_gpu::Frame, generation: u64, paused: bool) -> HookTime {
        self.serial = self.serial.wrapping_add(1);
        HookTime {
            serial: self.serial,
            dt: 0.,
            reset: self.last.is_none() || self.generation != Some(generation),
            paused,
            seekable: frame.seekable,
            points: (frame.width, frame.height),
            scale: frame.scale,
            shader_generation: frame.shader_generation,
            ..HookTime::default()
        }
    }
}
