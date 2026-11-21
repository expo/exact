use exact_game_render::exact_game::Mat4;
use exact_game_render::hooks::{HookTime, Pipelines, SceneCopy};
use exact_game_render::{FrameView, FrameWork, HookGpu, Hooks, Needs, RenderError, RenderWorld};
use exact_gpu::wgpu::{self, util::DeviceExt};
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use terrain_fixture_logic::{height, Blend, Draw, WorkloadSettings, SIDE};

mod arena;
mod jobs;
mod mesh;
mod workload;

use arena::{Allocation, Arenas, Indexed, Indirect, INDIRECT_BYTES};
use workload::{FrameBudget, Job, Origin, SectionWork, Unit, Workload};

pub mod shaders {
    include!(concat!(env!("OUT_DIR"), "/shaders.rs"));
}

#[derive(Default)]
pub struct Passes {
    settings: WorkloadSettings,
    workload: Workload<Candidate>,
    jobs: Option<jobs::TerrainJobs>,
    job_error: Option<String>,
    pending_reason: OnceCell<String>,
    report: OnceCell<String>,
    observed: (u64, bool),
    sections: BTreeMap<Origin, Section>,
    section_buffers: SectionBuffers<wgpu::Buffer>,
    arenas: Arenas,
    indirect: Option<Indirect>,
    binds: u64,
    total_binds: u64,
    dedicated_draws: DedicatedDraws,
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
    buffer: Option<SectionBuffer>,
    input: Option<PaddedInput>,
}

struct SectionBuffer {
    geometry: Geometry,
    slot: Option<u32>,
}

enum Geometry {
    Dedicated(Buffer),
    Shared(Allocation),
}

enum PaddedInput {
    Owned(Box<[u8]>),
    Shared(Arc<Vec<u8>>),
}

impl PaddedInput {
    fn cells(&self) -> &[u8] {
        match self {
            Self::Owned(cells) => cells,
            Self::Shared(cells) => cells,
        }
    }

    fn shared(&self) -> Option<Arc<Vec<u8>>> {
        match self {
            Self::Owned(_) => None,
            Self::Shared(cells) => Some(cells.clone()),
        }
    }
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

#[derive(Default)]
struct OpaqueCalls {
    bound_arena: Option<usize>,
    binds: u64,
    draws: u64,
}

impl OpaqueCalls {
    fn section(&mut self, arena: Option<usize>, indirect: bool) -> bool {
        let bind_geometry = arena.is_none() || self.bound_arena != arena;
        self.binds += 2 * u64::from(bind_geometry) + u64::from(indirect);
        self.draws += 1;
        self.bound_arena = arena;
        bind_geometry
    }
}

#[derive(Default)]
struct DedicatedDraws {
    last: Option<u64>,
    total: u64,
    frames: u64,
}

#[derive(Default)]
struct OpaqueStats {
    binds: u64,
    total_binds: u64,
    arenas: usize,
    arena_free_bytes: u64,
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

    fn bind(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.buffer.slice(..self.index_offset));
        pass.set_index_buffer(
            self.buffer
                .slice(self.index_offset..self.index_offset + u64::from(self.indices) * 4),
            wgpu::IndexFormat::Uint32,
        );
    }

    fn indexed(&self) -> Indexed {
        Indexed {
            count: self.indices,
            first: 0,
            base: 0,
        }
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, work: &mut FrameWork) {
        self.bind(pass);
        self.indexed().issue(pass);
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
        let rebuilt = tick < self.observed.0 || settings != self.settings;
        if rebuilt {
            self.reset(settings);
        }
        let tick_changed = tick != self.observed.0;
        self.observed.0 = tick;
        let advanced = time.allowance > 0
            && self
                .workload
                .advance(settings, tick, hz, !time.seekable, time.interval_ms);
        rebuilt || tick_changed || advanced
    }

    fn pipeline_pending(&self) -> bool {
        self.frame.is_none() || self.pipeline.pending() || self.water.pipeline.pending()
    }

    fn reset(&mut self, settings: WorkloadSettings) {
        self.workload.reset();
        if settings.jobs {
            self.jobs.get_or_insert_with(jobs::TerrainJobs::new).clear();
        } else {
            self.jobs = None;
        }
        self.job_error = None;
        self.sections.clear();
        self.section_buffers.clear();
        self.arenas = Arenas::default();
        self.indirect = None;
        if self.settings == settings {
            if settings.draw == Draw::Indirect {
                self.pipeline = Pipelines::default();
            }
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
        self.dedicated_draws.last = None;
        self.pending_reason.take();
        self.report.take();
    }

    fn retire(&mut self, origin: Origin) {
        if self.workload.contains(origin) {
            return;
        }
        if let Some(section) = self.sections.remove(&origin) {
            let removed = self.release_section_buffer(section.buffer, None);
            self.workload.gpu_bytes(removed, 0);
            self.workload.cpu_bytes(
                section.input.map_or(0, |input| input.cells().len() as u64),
                0,
            );
        }
        if let Some(quads) = self.water.tiles.remove(&origin) {
            self.workload
                .cpu_bytes((quads.capacity() * size_of::<mesh::WaterQuad>()) as u64, 0);
            self.water.dirty = true;
        }
    }

    fn release_section_buffer(
        &mut self,
        buffer: Option<SectionBuffer>,
        kept_slot: Option<u32>,
    ) -> u64 {
        let Some(buffer) = buffer else { return 0 };
        if let Some(slot) = buffer.slot.filter(|slot| Some(*slot) != kept_slot) {
            self.indirect.as_mut().unwrap().free(slot);
        }
        match buffer.geometry {
            Geometry::Dedicated(b) => self.section_buffers.release(
                !self.settings.draw.shared() && self.settings.pool,
                b.buffer.size(),
                b.buffer,
            ),
            Geometry::Shared(allocation) => {
                self.arenas.free(allocation);
                0
            }
        }
    }

    fn upload_section(
        &mut self,
        gpu: &HookGpu<'_>,
        mesh: &mut mesh::Mesh,
        previous_slot: Option<u32>,
    ) -> Option<(SectionBuffer, u64)> {
        let (bytes, index_offset, indices) = Buffer::pack(mesh)?;
        let shared = self.settings.draw.shared();
        let allocation = if shared {
            self.arenas.upload(
                gpu,
                bytes,
                u64::from(self.settings.vertex),
                index_offset,
                indices,
            )
        } else {
            None
        };
        let (geometry, mut added, draw) = match allocation {
            Some((allocation, added)) => {
                self.work.upload_bytes += bytes.len() as u64;
                let draw = allocation.draw;
                (Geometry::Shared(allocation), added, draw)
            }
            None => {
                let (buffer, allocated) = self.section_buffers.upload(
                    !shared && self.settings.pool,
                    bytes,
                    &mut self.work,
                    |capacity, bytes| Buffer::create_section(gpu, capacity, bytes),
                    |buffer, bytes| gpu.queue.write_buffer(buffer, 0, bytes),
                );
                let buffer = Buffer {
                    buffer,
                    index_offset,
                    indices,
                };
                let draw = buffer.indexed();
                (Geometry::Dedicated(buffer), allocated, draw)
            }
        };
        let slot = self.indirect.as_mut().map(|indirect| {
            let (slot, allocated) = indirect.upload(gpu, draw, previous_slot);
            added += allocated;
            self.work.upload_bytes += INDIRECT_BYTES;
            slot
        });
        Some((SectionBuffer { geometry, slot }, added))
    }

    fn section(&mut self, gpu: Option<&HookGpu<'_>>, job: Job) {
        let input = self
            .settings
            .compare
            .then(|| mesh::padded_input(job.origin, height, job.changed));
        if job.kind == SectionWork::Compare {
            let equal = self.sections.get(&job.origin).is_some_and(|section| {
                section.input.as_ref().map(PaddedInput::cells) == input.as_deref()
            });
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

    fn install(&mut self, gpu: Option<&HookGpu<'_>>, job: Job, candidate: Candidate) {
        if !self.workload.install(job, self.settings) {
            if self.settings.jobs {
                self.workload.retry_job(job);
            }
            return;
        }
        if self.settings.jobs {
            self.workload.refresh_after_kept_mesh(job);
        }
        self.install_buffers(gpu, job.origin, candidate);
    }

    fn install_buffers(&mut self, gpu: Option<&HookGpu<'_>>, origin: Origin, candidate: Candidate) {
        let Candidate { mut mesh, input } = candidate;
        let [x, y, z] = origin;
        let (old_cpu, old_buffer) = self.sections.remove(&origin).map_or((0, None), |old| {
            (
                old.input.map_or(0, |input| input.cells().len() as u64),
                old.buffer,
            )
        });
        let has_mesh = !mesh.indices.is_empty();
        let buffer = if self.settings.draw.shared() {
            let previous_slot = old_buffer.as_ref().and_then(|buffer| buffer.slot);
            let (buffer, added) = gpu
                .and_then(|gpu| self.upload_section(gpu, &mut mesh, previous_slot))
                .map_or((None, 0), |(buffer, added)| (Some(buffer), added));
            self.workload.gpu_bytes(0, added);
            let kept_slot = buffer.as_ref().and_then(|buffer| buffer.slot);
            let removed = self.release_section_buffer(old_buffer, kept_slot);
            self.workload.gpu_bytes(removed, 0);
            buffer
        } else {
            let removed = self.release_section_buffer(old_buffer, None);
            let (buffer, added) = gpu
                .and_then(|gpu| self.upload_section(gpu, &mut mesh, None))
                .map_or((None, 0), |(buffer, added)| (Some(buffer), added));
            self.workload.gpu_bytes(removed, added);
            buffer
        };
        let cpu = input.as_ref().map_or(0, |input| input.len() as u64);
        if has_mesh || input.is_some() {
            let input = input.map(|cells| {
                if self.settings.jobs {
                    PaddedInput::Shared(Arc::new(cells.into_vec()))
                } else {
                    PaddedInput::Owned(cells)
                }
            });
            self.sections.insert(origin, Section { buffer, input });
        }
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
        gpu: Option<&HookGpu<'_>>,
        time: &HookTime,
        tick: u64,
        hz: u32,
        camera: [f32; 3],
    ) -> bool {
        let mut budget = FrameBudget::new(self.settings.budget, time.allowance);
        self.water.dirty = self.water.needs_upload(camera);
        let mut worked = false;
        while let Some(unit) =
            self.workload
                .next(&mut budget, self.settings, tick, hz, self.water.dirty)
        {
            let small = unit.small();
            self.run_unit(gpu, unit, hz, camera);
            worked = true;
            budget.spent(small, || {
                time.deadline.is_some_and(|deadline| deadline.passed())
            });
        }
        worked | self.finish_blended_refills()
    }

    fn run_unit(
        &mut self,
        gpu: Option<&HookGpu<'_>>,
        unit: Unit<Candidate>,
        hz: u32,
        camera: [f32; 3],
    ) {
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
    }

    fn finish_blended_refills(&mut self) -> bool {
        !self.water.dirty && !self.workload.retiring() && self.workload.blend_current()
    }

    fn prepare_water(&mut self, gpu: Option<&HookGpu<'_>>, camera: [f32; 3]) {
        if let Some(gpu) = gpu {
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
        }
        self.water.uploaded = self.water.tiles.keys().copied().collect();
        self.water.dirty = false;
        self.water.camera = Some(camera);
    }

    fn water_drawable(&self) -> bool {
        self.water.pipeline.get().is_some() && self.water.buffer.is_some()
    }

    fn draw_water(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        if self.settings.draw != Draw::Off {
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

    fn record_dedicated_draws(&mut self, before: u64) {
        let draws = self.work.draws - before;
        self.dedicated_draws.last = Some(draws);
        self.dedicated_draws.total += draws;
        self.dedicated_draws.frames += 1;
        if before == 0 && draws > 0 {
            self.report.take();
        }
    }

    fn record_shared_binds(&mut self, binds: u64) {
        self.total_binds += binds;
        if self.binds != binds || binds != 0 {
            self.binds = binds;
            self.report.take();
        }
    }

    fn opaque_stats(&self) -> OpaqueStats {
        let binds = match self.settings.draw {
            Draw::On => self.dedicated_draws.last.map_or(0, |draws| 2 * draws + 1),
            Draw::Off => 0,
            Draw::Arena | Draw::Indirect => self.binds,
        };
        OpaqueStats {
            binds,
            total_binds: self.total_binds
                + 2 * self.dedicated_draws.total
                + self.dedicated_draws.frames,
            arenas: self.arenas.0.len(),
            arena_free_bytes: self.arenas.free_bytes(),
        }
    }
}

impl Hooks for Passes {
    const ALLOWANCE: u32 = 64;
    const REMOTE: &'static [exact_game_render::RemoteJob] = jobs::REMOTE;

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
        if settings.draw == Draw::Indirect && self.indirect.is_none() {
            self.indirect = Some(Indirect::new(gpu.device));
        }
        let slot_layout = self.indirect.as_ref().map(|indirect| &indirect.layout);
        self.pipeline
            .update(gpu.device, view.time.shader_generation, || {
                pipeline(gpu, view, settings, false, slot_layout)
            });
        if settings.blend != Blend::Off {
            self.water
                .pipeline
                .update(gpu.device, view.time.shader_generation, || {
                    pipeline(gpu, view, settings, true, None)
                });
        }
        self.frame = Some(gpu.frame_binding.clone());

        let timestamp_queries = gpu
            .device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY);
        let thermal_changed = self.workload.thermal(view.time.thermal_state.keyword());
        let worked = if settings.jobs {
            self.prepare_jobs(gpu, view, tick, hz)
        } else {
            view.time.allowance > 0
                && self.prepare_work(
                    Some(gpu),
                    &view.time,
                    tick,
                    hz,
                    view.frame.camera_position.to_array(),
                )
        };
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
        if self.job_error.is_some() {
            return needs;
        }
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
                self.work,
                timestamp_queries,
                self.section_buffers.stats,
                self.opaque_stats(),
            )
        }))
    }

    fn jobs(&self) -> Option<exact_game_render::JobsReport> {
        self.jobs.as_ref().map(jobs::TerrainJobs::report)
    }

    fn error(&self) -> Option<&str> {
        self.job_error
            .as_deref()
            .or_else(|| self.pipeline.error())
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
            if self.settings.draw.shared() {
                self.record_shared_binds(0);
            }
            return Ok(());
        };
        let planes = frustum_planes(view.frame.proj * view.frame.view);
        let blended = self.water_drawable();

        let drawing = self.settings.draw != Draw::Off;
        if drawing {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, frame, &[]);
        }
        let visible = self.sections.iter().filter_map(|(&origin, section)| {
            if !self.workload.draw_eligible(origin) {
                return None;
            }
            let buffer = section.buffer.as_ref()?;
            in_frustum(origin, &planes).then_some((origin, buffer))
        });
        if !self.settings.draw.shared() {
            let before = self.work.draws;
            for (origin, section) in visible {
                let Geometry::Dedicated(buffer) = &section.geometry else {
                    unreachable!("dedicated draw mode has shared geometry");
                };
                if drawing {
                    buffer.draw(pass, &mut self.work);
                    self.work.items +=
                        u64::from(!blended || !self.water.uploaded.contains(&origin));
                } else {
                    std::hint::black_box(buffer);
                }
            }
            if drawing {
                self.record_dedicated_draws(before);
            }
            return Ok(());
        }

        let mut calls = OpaqueCalls::default();
        for (origin, section) in visible {
            let slot = section.slot;
            match &section.geometry {
                Geometry::Dedicated(buffer) => {
                    calls.section(None, slot.is_some());
                    buffer.bind(pass);
                    issue_section(pass, self.indirect.as_ref(), slot, buffer.indexed());
                    self.work.triangles += u64::from(buffer.indices / 3);
                }
                Geometry::Shared(allocation) => self.arenas.0[allocation.arena]
                    .visible
                    .push((allocation.draw, slot)),
            }
            self.work.items += u64::from(!blended || !self.water.uploaded.contains(&origin));
        }
        // The opaque section faces do not overlap in area, so their depth test
        // allows one binding per arena while preserving the order inside it.
        for (index, arena) in self.arenas.0.iter_mut().enumerate() {
            for (draw, slot) in arena.visible.drain(..) {
                if calls.section(Some(index), slot.is_some()) {
                    pass.set_vertex_buffer(0, arena.buffer.slice(..));
                    pass.set_index_buffer(arena.buffer.slice(..), wgpu::IndexFormat::Uint32);
                }
                issue_section(pass, self.indirect.as_ref(), slot, draw);
                self.work.triangles += u64::from(draw.count / 3);
            }
        }
        self.work.draws += calls.draws;
        self.record_shared_binds(calls.binds + 1);
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

fn issue_section(
    pass: &mut wgpu::RenderPass<'_>,
    indirect: Option<&Indirect>,
    slot: Option<u32>,
    draw: Indexed,
) {
    if let Some(slot) = slot {
        indirect.unwrap().issue(pass, slot);
    } else {
        draw.issue(pass);
    }
}

fn frustum_planes(view_proj: Mat4) -> [[f32; 4]; 6] {
    let rows = view_proj.transpose();
    let (x, y, z, w) = (rows.x_axis, rows.y_axis, rows.z_axis, rows.w_axis);
    // WebGPU clips depth to 0 <= z <= w.
    [w + x, w - x, w + y, w - y, z, w - z].map(|p| p.to_array())
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
    slot_layout: Option<&wgpu::BindGroupLayout>,
) -> wgpu::RenderPipeline {
    use shaders::terrain;
    let device = gpu.device;
    let module = device.create_shader_module(terrain::module());
    let layouts = [Some(gpu.frame_layout), slot_layout];
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain"),
        bind_group_layouts: &layouts[..1 + usize::from(slot_layout.is_some())],
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
    fn opaque_calls_share_an_arena_binding() {
        const SECTIONS: u64 = 5;
        let plan = |arena, indirect| {
            let mut calls = OpaqueCalls::default();
            for _ in 0..SECTIONS {
                calls.section(arena, indirect);
            }
            (calls.binds, calls.draws)
        };
        assert_eq!(plan(None, false), (2 * SECTIONS, SECTIONS));
        assert_eq!(plan(Some(0), false), (2, SECTIONS));
        assert_eq!(plan(Some(0), true), (2 + SECTIONS, SECTIONS));
        let mut calls = OpaqueCalls::default();
        assert!(calls.section(None, false)); // Oversized section.
        assert!(calls.section(Some(0), false));
        assert!(!calls.section(Some(0), false));
        assert!(calls.section(Some(1), false));
        assert_eq!((calls.binds, calls.draws), (6, 4));
    }

    #[test]
    fn dedicated_draws_keep_the_report_cached_after_the_first_draw() {
        let mut hook = Passes::default();
        hook.report.set(String::new()).unwrap();
        hook.work.draws = 3;
        hook.record_dedicated_draws(0);
        assert!(hook.report.get().is_none());

        hook.report.set(String::new()).unwrap();
        hook.work.draws = 7;
        hook.record_dedicated_draws(4);
        assert!(
            hook.report.get().is_some(),
            "held frame dropped the cached report"
        );
        let stats = hook.opaque_stats();
        assert_eq!((stats.binds, stats.total_binds), (7, 14));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn test_frame() -> exact_gpu::Frame {
        exact_gpu::Frame {
            width: 128.,
            height: 72.,
            scale: 1.,
            now_ms: 0.,
            seekable: true,
            period_ms: 0.,
            children_generation: 0,
            shader_generation: 0,
            headroom: 1.,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn arena_and_indirect_match_dedicated_pixels_and_arena_rebuilds_after_loss() {
        use exact_game_render::exact_game::{Transform, Vec2};
        use exact_game_render::WorldSurface;
        use exact_gpu::{fixture, Surface, Value};
        use std::path::Path;
        use terrain_fixture_logic::{TerrainFixture, CAMERA, SECTIONS_DRAWN, SECTION_COUNT};

        type TerrainSurface = WorldSurface<TerrainFixture, (), false, Passes>;
        #[derive(Default)]
        struct EmptyTerrain;
        impl Hooks for EmptyTerrain {}

        fn fill(gpu: &exact_gpu::Gpu, surface: &mut TerrainSurface, frame: &mut exact_gpu::Frame) {
            let advances = SECTION_COUNT.div_ceil(Passes::ALLOWANCE as usize);
            for _ in 0..advances + 2 {
                frame.now_ms += 17.;
                fixture::render(gpu, surface, frame).unwrap();
                if surface.ready_reasons().is_empty() {
                    return;
                }
            }
            panic!("terrain did not fill: {:?}", surface.ready_reasons());
        }

        fn held(
            gpu: &exact_gpu::Gpu,
            surface: &mut TerrainSurface,
            frame: &exact_gpu::Frame,
        ) -> fixture::Pixels {
            let sim = surface.sim().unwrap();
            let pose = *sim.world().get::<Transform>("camera").unwrap();
            let view =
                Mat4::from_scale_rotation_translation(pose.scale, pose.rotation, pose.position)
                    .inverse();
            let planes = frustum_planes(CAMERA.matrix(Vec2::new(frame.width, frame.height)) * view);
            let hook = surface.render_hooks();
            assert!(hook.water.buffer.is_none());
            let mut arenas = BTreeSet::new();
            let mut dedicated = 0;
            for (&origin, section) in &hook.sections {
                if !hook.workload.draw_eligible(origin) || !in_frustum(origin, &planes) {
                    continue;
                }
                match &section.buffer.as_ref().unwrap().geometry {
                    Geometry::Shared(allocation) => {
                        arenas.insert(allocation.arena);
                    }
                    Geometry::Dedicated(_) => dedicated += 1,
                }
            }
            let draws = SECTIONS_DRAWN as u64;
            let mode = hook.settings.draw;
            if mode.shared() {
                assert!(!arenas.is_empty());
            } else {
                assert_eq!(dedicated, draws);
            }
            let binds = 1
                + 2 * (dedicated + arenas.len() as u64)
                + if mode == Draw::Indirect { draws } else { 0 };
            let before = hook.work;
            let binds_before = hook.opaque_stats().total_binds;
            hook.report().unwrap();

            let (image, _) = fixture::render(gpu, surface, frame).unwrap();
            assert!(
                surface.ready_reasons().is_empty(),
                "{:?}",
                surface.ready_reasons()
            );
            let hook = surface.render_hooks();
            assert_eq!(hook.work.upload_bytes, before.upload_bytes);
            assert_eq!(hook.work.draws, before.draws + draws);
            assert_eq!(hook.work.items, before.items + draws);
            let stats = hook.opaque_stats();
            assert_eq!(stats.binds, binds);
            assert_eq!(stats.total_binds, binds_before + binds);
            if mode == Draw::On {
                assert!(hook.report.get().is_some());
            }
            image
        }

        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        exact_gpu::shaders::load_dir(
            Path::new(env!("EXACT_GAME_SHADERS")),
            &exact_gpu::Registry {
                surfaces: &[],
                shaders: shaders::SHADERS,
            },
        )
        .unwrap();
        for stride in ["12", "64"] {
            let mut baseline = None;
            for mode in ["on", "arena", "indirect"] {
                let args = [
                    "smoke", "stand", "5", "0", "discard", "off", stride, "off", "count:64", mode,
                ];
                let args = args.map(Value::str);
                let mut surface = TerrainSurface::default();
                surface.bind(&args, None).unwrap();
                let mut frame = test_frame();
                fill(&gpu, &mut surface, &mut frame);
                let image = held(&gpu, &mut surface, &frame);
                if let Some(control) = &baseline {
                    assert!(&image == control, "{mode} pixels differ at stride {stride}");
                } else {
                    let mut empty =
                        WorldSurface::<TerrainFixture, (), false, EmptyTerrain>::default();
                    empty.bind(&args, None).unwrap();
                    let blank = empty.settle(&gpu, &frame, 4).unwrap();
                    assert!(
                        image != blank,
                        "dedicated terrain is blank at stride {stride}"
                    );
                    baseline = Some(image);
                }
                if mode == "arena" {
                    surface.device_lost();
                    assert!(surface.render_hooks().arenas.0.is_empty());
                    assert!(surface.render_hooks().sections.is_empty());
                    surface.device_ready(gpu.device.features(), gpu.device.limits());
                    fill(&gpu, &mut surface, &mut frame);
                    assert!(!surface.render_hooks().arenas.0.is_empty());
                    let rebuilt = held(&gpu, &mut surface, &frame);
                    assert!(
                        baseline.as_ref() == Some(&rebuilt),
                        "rebuilt arena pixels differ at stride {stride}"
                    );
                }
            }
            println!("vertex={stride}: {SECTIONS_DRAWN} opaque draws, bind counts match, arena and indirect pixels match dedicated terrain, and arena pixels match after loss");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn arena_replacement_retirement_and_oversized_fallback_account_for_resident_bytes() {
        use arena::ARENA_BYTES;
        use exact_game_render::WorldSurface;
        use exact_gpu::{fixture, Surface};
        use terrain_fixture_logic::TerrainFixture;

        #[derive(Default)]
        struct Churn(Passes);

        impl Hooks for Churn {
            fn prepare(
                &mut self,
                gpu: &HookGpu<'_>,
                _: &FrameView<'_>,
                _: &RenderWorld<'_>,
            ) -> Result<(), RenderError> {
                let hook = &mut self.0;
                hook.settings.draw = Draw::Arena;
                hook.frame = Some(gpu.frame_binding.clone());
                let candidate = |vertex_bytes| Candidate {
                    mesh: mesh::Mesh {
                        vertices: vec![0; vertex_bytes],
                        indices: vec![0, 1, 2],
                    },
                    input: None,
                };
                let place = |hook: &Passes, origin| {
                    let section = &hook.sections[&origin].buffer.as_ref().unwrap().geometry;
                    match section {
                        Geometry::Shared(allocation) => allocation.draw.base,
                        Geometry::Dedicated(_) => panic!("expected a shared range"),
                    }
                };
                let check = |hook: &mut Passes, free, resident, peak| {
                    hook.report.take();
                    let report = hook.report().unwrap();
                    assert!(
                        report.contains(&format!("\"arenaFreeBytes\":{free},")),
                        "{report}"
                    );
                    assert!(
                        report.contains(&format!("\"gpuBufferBytes\":{resident},")),
                        "{report}"
                    );
                    assert!(
                        report.contains(&format!("\"peakGpuBufferBytes\":{peak},")),
                        "{report}"
                    );
                };
                let origin = [0, 0, 0];
                let neighbour = [16, 0, 0];
                hook.install_buffers(Some(gpu), origin, candidate(36));
                hook.install_buffers(Some(gpu), neighbour, candidate(36));
                let old = place(hook, origin);
                check(hook, ARENA_BYTES - 96, ARENA_BYTES, ARENA_BYTES);
                hook.install_buffers(Some(gpu), origin, candidate(36));
                assert_ne!(place(hook, origin), old);
                check(hook, ARENA_BYTES - 96, ARENA_BYTES, ARENA_BYTES);
                let oversized = ARENA_BYTES.div_ceil(12) * 12;
                hook.install_buffers(Some(gpu), origin, candidate(oversized as usize));
                assert!(matches!(
                    hook.sections[&origin].buffer.as_ref().unwrap().geometry,
                    Geometry::Dedicated(_)
                ));
                let resident = ARENA_BYTES + oversized + 12;
                check(hook, ARENA_BYTES - 48, resident, resident);
                hook.install_buffers(Some(gpu), origin, candidate(oversized as usize));
                let peak = resident + oversized + 12;
                check(hook, ARENA_BYTES - 48, resident, peak);
                hook.retire(origin);
                hook.retire(neighbour);
                assert!(hook.sections.is_empty());
                check(hook, ARENA_BYTES, ARENA_BYTES, peak);
                println!("oversized replacement: {resident} resident bytes, {peak} peak bytes");
                Ok(())
            }
        }

        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        let mut surface = WorldSurface::<TerrainFixture, (), false, Churn>::default();
        surface.bind(&[], None).unwrap();
        fixture::render(&gpu, &mut surface, &test_frame()).unwrap();
        assert!(surface.error().is_none(), "{:?}", surface.error());
        assert_eq!(surface.render_hooks().0.arenas.0.len(), 1);
    }

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
        let settings = WorkloadSettings {
            compare: true,
            ..WorkloadSettings::default()
        };
        let mut passes = Passes::default();
        let frame = |passes: &mut Passes, settings: WorkloadSettings, tick, reset| {
            let mut time = HookTime::default();
            time.allowance = 64;
            time.reset = reset;
            passes.begin_work(settings, tick, 60, &time);
            assert!(passes.prepare_work(None, &time, tick, 60, [0.; 3]));
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
        let arena = WorkloadSettings {
            draw: Draw::Arena,
            ..pooled
        };
        assert!(passes
            .sections
            .values()
            .all(|section| section.input.is_some()));
        assert!(passes.begin_work(arena, 2, 60, &HookTime::default()));
        assert!(passes.sections.is_empty());
        assert_eq!(passes.workload.pending(arena), 2312);
        assert_eq!(frame(&mut passes, arena, 3, false), (64, 2248));
        assert_eq!(frame(&mut passes, arena, 4, false), (128, 2184));
        passes.device_lost();
        assert!(passes.sections.is_empty());
        assert_eq!(frame(&mut passes, arena, 5, true), (64, 2248));
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
