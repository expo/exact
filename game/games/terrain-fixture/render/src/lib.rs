use exact_game_render::hooks::{HookTime, Pipelines, SceneCopy};
use exact_game_render::{FrameView, FrameWork, HookGpu, Hooks, Needs, RenderError, RenderWorld};
use exact_gpu::wgpu::{self, util::DeviceExt};
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use terrain_fixture_logic::{height, Blend, WorkloadSettings, SIDE};

mod mesh;
mod workload;

use workload::{FrameBudget, Job, Origin, SectionWork, Unit, Workload};

pub mod shaders {
    include!(concat!(env!("OUT_DIR"), "/shaders.rs"));
}

#[derive(Default)]
pub struct Passes {
    settings: WorkloadSettings,
    workload: Workload<Candidate>,
    pending_reason: OnceCell<String>,
    report: OnceCell<String>,
    observed: (u64, bool),
    sections: BTreeMap<Origin, Section>,
    section_buffers: SectionBuffers<wgpu::Buffer>,
    water: Water,
    pipeline: Pipelines<wgpu::RenderPipeline>,
    frame: Option<wgpu::BindGroup>,
    work: FrameWork,
}

#[derive(Default)]
struct Water {
    tiles: BTreeMap<Origin, Vec<mesh::WaterQuad>>,
    uploaded: BTreeSet<Origin>,
    camera: Option<[f32; 3]>,
    dirty: bool,
    buffer: Option<Buffer>,
    pipeline: Pipelines<wgpu::RenderPipeline>,
}

impl Water {
    fn needs_upload(&self, camera: [f32; 3]) -> bool {
        self.dirty || (!self.tiles.is_empty() && self.camera != Some(camera))
    }
}

struct Section {
    buffer: Option<Buffer>,
    input: Option<Box<[u8]>>,
}

#[derive(Default)]
struct Candidate {
    mesh: mesh::Mesh,
    input: Option<Box<[u8]>>,
}

impl Candidate {
    fn cpu_bytes(&self) -> u64 {
        self.input.as_ref().map_or(0, |input| input.len() as u64)
            + self.mesh.vertices.capacity() as u64
            + (self.mesh.indices.capacity() * size_of::<u32>()) as u64
    }
}

struct Buffer {
    buffer: wgpu::Buffer,
    index_offset: u64,
    indices: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BufferStats {
    created: u64,
    reused: u64,
    destroyed: u64,
    bytes_written: u64,
}

struct SectionBuffers<B> {
    stats: BufferStats,
    free: BTreeMap<u64, Vec<B>>,
}

impl<B> Default for SectionBuffers<B> {
    fn default() -> Self {
        Self {
            stats: BufferStats::default(),
            free: BTreeMap::new(),
        }
    }
}

impl<B> SectionBuffers<B> {
    fn upload(
        &mut self,
        pool: bool,
        bytes: &[u8],
        work: &mut FrameWork,
        create: impl FnOnce(Option<u64>, &[u8]) -> B,
        write: impl FnOnce(&B, &[u8]),
    ) -> (B, u64) {
        let size = bytes.len() as u64;
        self.stats.bytes_written += size;
        work.upload_bytes += size;
        if !pool {
            self.stats.created += 1;
            return (create(None, bytes), size);
        }

        let class = size.next_power_of_two();
        let (buffer, allocated) = if let Some(buffer) = self.free.get_mut(&class).and_then(Vec::pop)
        {
            self.stats.reused += 1;
            (buffer, 0)
        } else {
            self.stats.created += 1;
            (create(Some(class), bytes), class)
        };
        write(&buffer, bytes);
        (buffer, allocated)
    }

    fn release(&mut self, pool: bool, capacity: u64, buffer: B) -> u64 {
        if pool {
            self.free.entry(capacity).or_default().push(buffer);
            return 0;
        }
        self.stats.destroyed += 1;
        capacity
    }

    fn clear(&mut self) {
        self.free.clear();
        self.stats.destroyed = self.stats.created;
    }
}

impl Buffer {
    fn upload(
        gpu: &HookGpu<'_>,
        mut mesh: mesh::Mesh,
        work: &mut FrameWork,
        previous: Option<Self>,
        usage: wgpu::BufferUsages,
    ) -> Option<Self> {
        let (bytes, index_offset, indices) = Self::pack(&mut mesh)?;
        let size = bytes.len() as u64;
        let buffer =
            if let Some(previous) = previous.filter(|previous| previous.buffer.size() == size) {
                gpu.queue.write_buffer(&previous.buffer, 0, bytes);
                previous.buffer
            } else {
                Self::create_initialized(gpu, bytes, usage)
            };
        work.upload_bytes += size;
        Some(Self {
            buffer,
            index_offset,
            indices,
        })
    }

    fn pack(mesh: &mut mesh::Mesh) -> Option<(&[u8], u64, u32)> {
        if mesh.indices.is_empty() {
            return None;
        }
        let index_offset = mesh.vertices.len() as u64;
        let indices = mesh.indices.len() as u32;
        mesh.vertices.extend(
            std::mem::take(&mut mesh.indices)
                .into_iter()
                .flat_map(u32::to_le_bytes),
        );
        Some((&mesh.vertices, index_offset, indices))
    }

    fn create_initialized(
        gpu: &HookGpu<'_>,
        bytes: &[u8],
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        gpu.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain geometry"),
                contents: bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::INDEX | usage,
            })
    }

    fn create_section(gpu: &HookGpu<'_>, capacity: Option<u64>, bytes: &[u8]) -> wgpu::Buffer {
        match capacity {
            Some(size) => gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("terrain section"),
                size,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::INDEX
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            None => Self::create_initialized(gpu, bytes, wgpu::BufferUsages::empty()),
        }
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, work: &mut FrameWork) {
        pass.set_vertex_buffer(0, self.buffer.slice(..self.index_offset));
        pass.set_index_buffer(
            self.buffer
                .slice(self.index_offset..self.index_offset + u64::from(self.indices) * 4),
            wgpu::IndexFormat::Uint32,
        );
        pass.draw_indexed(0..self.indices, 0, 0..1);
        work.draws += 1;
        work.triangles += u64::from(self.indices / 3);
    }

    fn bytes(buffer: Option<&Self>) -> u64 {
        buffer.map_or(0, |b| b.buffer.size())
    }
}

impl Passes {
    fn begin_work(
        &mut self,
        settings: WorkloadSettings,
        tick: u64,
        hz: u32,
        time: &HookTime,
    ) -> bool {
        // A presentation rebuild also resets the clock's histories.
        if tick < self.observed.0 || settings != self.settings {
            self.reset(settings);
        }
        let tick_changed = tick != self.observed.0;
        self.observed.0 = tick;
        let advanced = time.allowance > 0
            && self
                .workload
                .advance(settings, tick, hz, !time.seekable, time.interval_ms);
        tick_changed || advanced
    }

    fn pipeline_pending(&self) -> bool {
        self.frame.is_none() || self.pipeline.pending() || self.water.pipeline.pending()
    }

    fn reset(&mut self, settings: WorkloadSettings) {
        self.workload.reset();
        self.sections.clear();
        self.section_buffers.clear();
        if self.settings == settings {
            self.water = Water {
                pipeline: std::mem::take(&mut self.water.pipeline),
                ..Water::default()
            };
        } else {
            self.pipeline = Pipelines::default();
            self.water = Water::default();
        }
        self.settings = settings;
        self.work = FrameWork::default();
        self.pending_reason.take();
        self.report.take();
    }

    fn retire(&mut self, origin: Origin) {
        if self.workload.contains(origin) {
            return;
        }
        if let Some(section) = self.sections.remove(&origin) {
            let removed = self.release_section_buffer(section.buffer);
            self.workload.gpu_bytes(removed, 0);
            self.workload
                .cpu_bytes(section.input.map_or(0, |input| input.len() as u64), 0);
        }
        if let Some(quads) = self.water.tiles.remove(&origin) {
            self.workload
                .cpu_bytes((quads.capacity() * size_of::<mesh::WaterQuad>()) as u64, 0);
            self.water.dirty = true;
        }
    }

    fn release_section_buffer(&mut self, buffer: Option<Buffer>) -> u64 {
        buffer.map_or(0, |b| {
            self.section_buffers
                .release(self.settings.pool, b.buffer.size(), b.buffer)
        })
    }

    fn section(&mut self, gpu: &HookGpu<'_>, job: Job) {
        let input = self
            .settings
            .compare
            .then(|| mesh::padded_input(job.origin, height, job.changed));
        if job.kind == SectionWork::Compare {
            let equal = self
                .sections
                .get(&job.origin)
                .is_some_and(|section| section.input == input);
            self.workload.compared(job, equal, self.settings.blend);
            return;
        }

        let mesh = mesh::section_with(
            job.origin,
            height,
            job.changed,
            self.settings.vertex,
            self.settings.light,
        );
        let candidate = Candidate { mesh, input };
        let cpu_bytes = candidate.cpu_bytes();
        match self.workload.stage_if_repair(job, candidate) {
            Ok(()) => self.workload.cpu_bytes(0, cpu_bytes),
            Err(candidate) => self.install(gpu, job, candidate),
        }
    }

    fn install(&mut self, gpu: &HookGpu<'_>, job: Job, candidate: Candidate) {
        if !self.workload.install(job, self.settings) {
            return;
        }
        let Candidate { mut mesh, input } = candidate;
        let origin @ [x, y, z] = job.origin;
        let (old_cpu, removed) = self.sections.remove(&origin).map_or((0, 0), |old| {
            let cpu = old.input.map_or(0, |input| input.len() as u64);
            (cpu, self.release_section_buffer(old.buffer))
        });
        let mut added = 0;
        let buffer = Buffer::pack(&mut mesh).map(|(bytes, index_offset, indices)| {
            let (buffer, allocated) = self.section_buffers.upload(
                self.settings.pool,
                bytes,
                &mut self.work,
                |capacity, bytes| Buffer::create_section(gpu, capacity, bytes),
                |buffer, bytes| gpu.queue.write_buffer(buffer, 0, bytes),
            );
            added = allocated;
            Buffer {
                buffer,
                index_offset,
                indices,
            }
        });
        let cpu = input.as_ref().map_or(0, |input| input.len() as u64);
        if buffer.is_some() || input.is_some() {
            self.sections.insert(origin, Section { buffer, input });
        }
        self.workload.gpu_bytes(removed, added);
        self.workload.cpu_bytes(old_cpu, cpu);
        if self.settings.blend != Blend::Off
            && y == mesh::WATER_Y
            && !self.water.tiles.contains_key(&origin)
        {
            let quads = mesh::water_quads(x, z, height);
            if !quads.is_empty() {
                self.workload
                    .cpu_bytes(0, (quads.capacity() * size_of::<mesh::WaterQuad>()) as u64);
                self.water.tiles.insert(origin, quads);
                self.water.dirty = true;
            }
        }
    }

    fn prepare_work(
        &mut self,
        gpu: &HookGpu<'_>,
        view: &FrameView<'_>,
        tick: u64,
        hz: u32,
    ) -> bool {
        let mut budget = FrameBudget::new(self.settings.budget, view.time.allowance);
        let camera = view.frame.camera_position.to_array();
        self.water.dirty = self.water.needs_upload(camera);
        let mut worked = false;
        while let Some(unit) =
            self.workload
                .next(&mut budget, self.settings, tick, hz, self.water.dirty)
        {
            worked = true;
            match unit {
                Unit::Retire(origin) => {
                    self.retire(origin);
                    self.workload.retired();
                }
                Unit::Edit { tick, index } => self.workload.edit(self.settings, tick, index, hz),
                Unit::Section(job) => self.section(gpu, job),
                Unit::Install(job, candidate) => {
                    self.workload.cpu_bytes(candidate.cpu_bytes(), 0);
                    self.install(gpu, job, candidate);
                }
                Unit::DropStaged(candidate) => self.workload.cpu_bytes(candidate.cpu_bytes(), 0),
                Unit::Blend => {
                    self.prepare_water(gpu, camera);
                    self.workload.blended();
                }
            }
            budget.spent(|| view.time.deadline.is_some_and(|deadline| deadline.passed()));
        }
        if !self.water.dirty && !self.workload.retiring() {
            worked |= self.workload.blend_current();
        }
        worked
    }

    fn prepare_water(&mut self, gpu: &HookGpu<'_>, camera: [f32; 3]) {
        let mut quads: Vec<_> = self.water.tiles.values().flatten().copied().collect();
        mesh::sort_water(&mut quads, camera);
        let removed = Buffer::bytes(self.water.buffer.as_ref());
        self.water.buffer = Buffer::upload(
            gpu,
            mesh::water(&quads, self.settings.vertex),
            &mut self.work,
            self.water.buffer.take(),
            wgpu::BufferUsages::COPY_DST,
        );
        self.workload
            .gpu_bytes(removed, Buffer::bytes(self.water.buffer.as_ref()));
        self.water.uploaded = self.water.tiles.keys().copied().collect();
        self.water.dirty = false;
        self.water.camera = Some(camera);
    }

    fn water_drawable(&self) -> bool {
        self.water.pipeline.get().is_some() && self.water.buffer.is_some()
    }

    fn draw_water(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        if self.settings.draw {
            if let (Some(pipeline), Some(buffer), Some(frame)) =
                (self.water.pipeline.get(), &self.water.buffer, &self.frame)
            {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, frame, &[]);
                let first = self.work.draws == 0;
                buffer.draw(pass, &mut self.work);
                self.work.items += self.water.uploaded.len() as u64;
                if first {
                    self.report.take();
                }
            }
        }
    }
}

impl Hooks for Passes {
    const ALLOWANCE: u32 = 64;

    fn prepare(
        &mut self,
        gpu: &HookGpu<'_>,
        view: &FrameView<'_>,
        world: &RenderWorld<'_>,
    ) -> Result<(), RenderError> {
        let settings = world
            .get::<WorkloadSettings>("camera")
            .map_or_else(WorkloadSettings::default, |settings| *settings);
        let pending = (
            self.workload.pending(self.settings),
            self.pipeline_pending(),
        );
        let tick = world.tick();
        let hz = world.hz();
        let changed = self.begin_work(settings, tick, hz, &view.time);
        self.pipeline
            .update(gpu.device, view.time.shader_generation, || {
                pipeline(gpu, view, settings, false)
            });
        if settings.blend != Blend::Off {
            self.water
                .pipeline
                .update(gpu.device, view.time.shader_generation, || {
                    pipeline(gpu, view, settings, true)
                });
        }
        self.frame = Some(gpu.frame_binding.clone());

        let timestamp_queries = gpu
            .device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY);
        let thermal_changed = self.workload.thermal(view.time.thermal_state.keyword());
        let worked = view.time.allowance > 0 && self.prepare_work(gpu, view, tick, hz);
        if self.observed.1 != timestamp_queries || thermal_changed || changed || worked {
            self.observed.1 = timestamp_queries;
            self.report.take();
        }
        if pending != (self.workload.pending(settings), self.pipeline_pending()) {
            self.pending_reason.take();
        }
        Ok(())
    }

    fn needs(&self) -> Needs {
        let mut needs = if self.settings.blend == Blend::Surface {
            Needs::SCENE_COPY
        } else {
            Needs::NONE
        };
        if self.pipeline_pending() {
            needs = needs | Needs::PENDING;
        } else if self.workload.pending(self.settings) > 0
            || self.workload.retiring()
            || self.water.dirty
        {
            needs = needs | Needs::PENDING | Needs::STARVED;
        }
        needs
    }

    fn pending_reason(&self) -> &str {
        self.pending_reason.get_or_init(|| {
            let pending = self.workload.pending(self.settings);
            if self.pipeline_pending() {
                format!("{pending} sections pending, terrain pipeline validating")
            } else {
                format!("{pending} sections pending")
            }
        })
    }

    fn report(&self) -> Option<&str> {
        let (tick, timestamp_queries) = self.observed;
        self.frame.as_ref()?;
        Some(self.report.get_or_init(|| {
            self.workload.report(
                self.settings,
                tick,
                self.work.draws > 0,
                self.work.upload_bytes,
                timestamp_queries,
                self.section_buffers.stats,
            )
        }))
    }

    fn error(&self) -> Option<&str> {
        self.pipeline
            .error()
            .or_else(|| self.water.pipeline.error())
    }

    fn frame_work(&self) -> Option<FrameWork> {
        Some(self.work)
    }

    fn item_name(&self) -> &'static str {
        "sections"
    }

    fn opaque(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        view: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        let (Some(pipeline), Some(frame)) = (self.pipeline.get(), &self.frame) else {
            return Ok(());
        };
        let rows = (view.frame.proj * view.frame.view).transpose();
        let (x, y, z, w) = (rows.x_axis, rows.y_axis, rows.z_axis, rows.w_axis);
        // WebGPU clips depth to 0 <= z <= w.
        let planes = [w + x, w - x, w + y, w - y, z, w - z].map(|p| p.to_array());
        let blended = self.water_drawable();

        if self.settings.draw {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, frame, &[]);
        }
        let before = self.work.draws;
        for (&origin, section) in &self.sections {
            if !self.workload.draw_eligible(origin) {
                continue;
            }
            let Some(buffer) = &section.buffer else {
                continue;
            };
            if in_frustum(origin, &planes) {
                if self.settings.draw {
                    buffer.draw(pass, &mut self.work);
                    self.work.items +=
                        u64::from(!blended || !self.water.uploaded.contains(&origin));
                } else {
                    std::hint::black_box(buffer);
                }
            }
        }
        if before == 0 && self.work.draws > 0 {
            self.report.take();
        }
        Ok(())
    }

    fn background(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        _: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        if self.settings.blend == Blend::Background {
            self.draw_water(pass);
        }
        Ok(())
    }

    fn surface(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        _: &SceneCopy<'_>,
        _: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        if self.settings.blend == Blend::Surface {
            self.draw_water(pass);
        }
        Ok(())
    }
}
fn in_frustum(origin: [i32; 3], planes: &[[f32; 4]; 6]) -> bool {
    planes.iter().all(|&[x, y, z, w]| {
        // The box corner furthest inside this plane suffices to reject the box.
        let distance = |start, normal: f32| {
            let far_corner = if normal >= 0. { start + SIDE } else { start };
            far_corner as f32 * normal
        };
        distance(origin[0], x) + distance(origin[1], y) + distance(origin[2], z) + w >= 0.
    })
}

fn pipeline(
    gpu: &HookGpu<'_>,
    view: &FrameView<'_>,
    settings: WorkloadSettings,
    water: bool,
) -> wgpu::RenderPipeline {
    use shaders::terrain;
    let device = gpu.device;
    let module = device.create_shader_module(terrain::module());
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain"),
        bind_group_layouts: &[Some(gpu.frame_layout)],
        immediate_size: 0,
    });
    let mut buffer = wgpu::VertexBufferLayout {
        array_stride: u64::from(settings.vertex),
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3],
    };
    let lit = !water && settings.light > 0;
    if lit && settings.vertex == 64 {
        buffer.attributes = &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 24,
                shader_location: 1,
            },
        ];
    }
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(if water { "terrain water" } else { "terrain" }),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some(match (lit, settings.vertex) {
                (true, 64) => terrain::entry::LIT_VS,
                (true, _) => terrain::entry::PACKED_VS,
                (false, _) => terrain::entry::TERRAIN_VS,
            }),
            buffers: &[Some(buffer)],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(if water {
                terrain::entry::WATER_FS
            } else if lit {
                terrain::entry::LIT_FS
            } else {
                terrain::entry::TERRAIN_FS
            }),
            targets: &[Some(wgpu::ColorTargetState {
                format: view.color_format,
                blend: water.then_some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: (!water).then_some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: view.depth_format,
            depth_write_enabled: Some(!water),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: view.sample_count,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_section_buffers_reuse_capacity_classes_across_twenty_shifts() {
        use terrain_fixture_logic::Window;

        let run = |pool| {
            let mut buffers = SectionBuffers::default();
            let mut work = FrameWork::default();
            let mut sections = BTreeMap::new();
            let mut writes = 0;
            let mut resident = 0;
            let mut previous = None;
            for shift in 0..=20 {
                let window = Window {
                    center: [shift, 0],
                    radius: 1,
                };
                sections.retain(|&origin, capacity| {
                    if window.contains(origin) {
                        true
                    } else {
                        resident -= buffers.release(pool, *capacity, *capacity);
                        false
                    }
                });
                for origin in window.entering(previous) {
                    let size = if shift > 0 && shift % 3 == 1 && origin[1..] == [0, -SIDE] {
                        68
                    } else {
                        60
                    };
                    let (capacity, allocated) = buffers.upload(
                        pool,
                        &[0; 68][..size],
                        &mut work,
                        |class, bytes| class.unwrap_or(bytes.len() as u64),
                        |capacity, bytes| {
                            assert_eq!(*capacity, (bytes.len() as u64).next_power_of_two());
                            writes += 1;
                        },
                    );
                    resident += allocated;
                    sections.insert(origin, capacity);
                }
                assert_eq!(sections.len(), 72);
                if pool {
                    assert_eq!(buffers.stats.created, 72 + u64::from(shift > 0));
                }
                previous = Some(window);
            }
            assert_eq!(work.upload_bytes, 33176);
            let stats = buffers.stats;
            buffers.clear();
            assert!(buffers.free.is_empty());
            assert_eq!(buffers.stats.destroyed, stats.created);
            (stats, writes, resident)
        };
        assert_eq!(
            run(false),
            (
                BufferStats {
                    created: 552,
                    reused: 0,
                    destroyed: 480,
                    bytes_written: 33176,
                },
                0,
                4328
            )
        );
        assert_eq!(
            run(true),
            (
                BufferStats {
                    created: 73,
                    reused: 479,
                    destroyed: 0,
                    bytes_written: 33176,
                },
                552,
                4736
            )
        );
    }

    #[test]
    fn decision_world_rewind_rebuilds_and_presentation_reset_keeps_sections() {
        let settings = WorkloadSettings::default();
        let mut passes = Passes::default();
        let frame = |passes: &mut Passes, settings: WorkloadSettings, tick, reset| {
            let time = HookTime {
                allowance: 64,
                reset,
                ..HookTime::default()
            };
            passes.begin_work(settings, tick, 60, &time);
            let mut budget = FrameBudget::new(settings.budget, time.allowance);
            while let Some(Unit::Section(job)) =
                passes.workload.next(&mut budget, settings, tick, 60, false)
            {
                assert!(passes.workload.install(job, settings));
                passes.sections.insert(
                    job.origin,
                    Section {
                        buffer: None,
                        input: None,
                    },
                );
                budget.spent(|| false);
            }
            (passes.sections.len(), passes.workload.pending(settings))
        };
        assert_eq!(frame(&mut passes, settings, 1, true), (64, 2248));
        assert_eq!(frame(&mut passes, settings, 2, true), (128, 2184));
        assert_eq!(frame(&mut passes, settings, 1, false), (64, 2248));
        let pooled = WorkloadSettings {
            pool: true,
            ..settings
        };
        assert_eq!(frame(&mut passes, pooled, 2, false), (64, 2248));
        passes.device_lost();
        assert!(passes.sections.is_empty());
        assert_eq!(frame(&mut passes, pooled, 3, false), (64, 2248));
    }

    #[test]
    fn decision_water_sorts_back_to_front_for_each_camera() {
        let mut quads = [[0, 0, 1, 1], [16, 0, 1, 1]];
        mesh::sort_water(&mut quads, [0., 0., 0.]);
        assert_eq!(quads, [[16, 0, 1, 1], [0, 0, 1, 1]]);
        mesh::sort_water(&mut quads, [17., 0., 0.]);
        assert_eq!(quads, [[0, 0, 1, 1], [16, 0, 1, 1]]);
    }

    #[test]
    fn a_fresh_hook_needs_a_redraw_to_start_validation() {
        assert_eq!(Passes::default().needs(), Needs::PENDING);
    }
}
