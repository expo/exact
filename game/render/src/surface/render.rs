//! World advancement and GPU frame ownership.
use super::*;

pub(super) fn retain_primes<const ASSETS: bool>(
    render: &mut Option<(crate::renderer::RendererWithAssets<ASSETS>, Feed)>,
    placed: &mut crate::placed::Placements,
    retain: bool,
) {
    placed.attachments.retain_primes(retain);
    if let Some((renderer, feed)) = render {
        feed.retain_primes(retain);
        if let Some(skin) = &mut renderer.models.skinning {
            skin.retain_primes(retain);
        }
    }
}

impl<G: Game, P: Executor, const ASSETS: bool, H: crate::Hooks> Default
    for WorldSurface<G, P, ASSETS, H>
{
    fn default() -> Self {
        Self {
            sim: None,
            development_edits: true,
            pending_restore: None,
            placed: Default::default(),
            refusal: None,
            presentation: P::default(),
            hooks: H::default(),
            hook_clock: Default::default(),
            hook_poses: Default::default(),
            gpu_timing: None,
            encoded: None,
            render: None,
            format: None,
            payloads: Default::default(),
            device: false,
            device_assets_invalidated: false,
            storage_limit: 0,
            perf: Perf::default(),
            ready_work: None,
            trace: None,
            error: None,
            dirty: true,
            output_opaque: false,
            frame_ready: false,
            assets_dirty: false,
            models_seen: 0,
            asset_check: None,
            reported: false,
            generation: 0,
            seekable: true,
            audio_requested: false,
            hidden: false,
            interrupted: false,
            look: Default::default(),
            rung: None,
            resting: false,
            still: false,
            idle_tick: None,
            samples: None,
            timing: Default::default(),
            waiting: Default::default(),
            held_back: false,
            #[cfg(not(target_arch = "wasm32"))]
            uploads: None,
        }
    }
}

/// Milliseconds a live frame spends preparing what the world does not
/// present, once its first frame is drawn.
const LATER_MS: f64 = 4.0;

/// Whether a live preparation may do work for what the world does not present
/// yet: none before the first frame, then [`LATER_MS`] of it.
struct Later {
    live: bool,
    first: bool,
    started: Option<Stamp>,
}
impl Later {
    fn new(live: bool, first: bool) -> Self {
        Self {
            live,
            first,
            started: None,
        }
    }
    fn allows(&mut self) -> bool {
        !self.first && self.started.get_or_insert_with(Stamp::now).elapsed() < LATER_MS
    }
}

impl<G: Game, P: Executor, const ASSETS: bool, H: crate::Hooks> WorldSurface<G, P, ASSETS, H> {
    /// Read the simulation after a successful bind.
    pub fn sim(&self) -> Option<&Sim<G>> {
        self.sim.as_ref()
    }
    /// Read presentation diagnostics owned by this canvas's render hooks.
    pub fn render_hooks(&self) -> &H {
        &self.hooks
    }
    /// `Surface::prepare_assets` under whatever pipeline timing is on.
    pub(super) fn prepare_now(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        if !self.storage_fits(device) {
            return;
        }
        if !ASSETS {
            return;
        }
        let models = self.sim.as_ref().map(|s| s.world().model_revision());
        if models.is_some_and(|m| m != self.models_seen) {
            self.models_seen = models.unwrap();
            self.assets_dirty = true;
        }
        if !self.assets_dirty && self.format == Some(format) && !self.asset_work() {
            return;
        }
        let Some(sim) = &mut self.sim else { return };
        sim.refresh_presented();
        let mut textures = sim.take_textures();
        if self.render.is_none()
            && sim.presentation_models().next().is_none()
            && textures.is_empty()
        {
            self.assets_dirty = false;
            return;
        }
        if self.format != Some(format) {
            self.gpu_timing = None;
            self.hook_clock.reset();
            self.hooks.device_lost();
            self.ready_work = None;
            self.perf.pipeline_ready = crate::perf::timing().then_some(0.);
            self.render = Some((
                crate::renderer::RendererWithAssets::<ASSETS>::configured(
                    device,
                    queue,
                    format,
                    *self
                        .samples
                        .insert(crate::quality::samples(self.rung, G::samples(sim.args()))),
                    G::DEPTH_PREPASS,
                )
                .with_matte_models(G::MATTE_MODELS),
                {
                    let mut feed = Feed::default();
                    feed.share_attachment_diagnostics(self.placed.attachments.diagnostics.clone());
                    feed
                },
            ));
            self.format = Some(format);
        }
        let (renderer, _feed) = self.render.as_mut().unwrap();
        let live = sim.presentation_assets().map(str::to_owned).collect();
        // Under the live clock what the world does not present (`Sim::presents`:
        // looks registered for later) waits for its first frame, then prepares
        // a few milliseconds a frame, and textures upload off this thread. Under
        // the agent's clock everything prepares here, now.
        let mut later = Later::new(!self.seekable, self.ready_work.is_none());
        self.held_back = false;
        // Accept deliveries before the sole compaction: live identical residents
        // may still be marked retired until preparation reactivates them.
        let mut prepared = Vec::new();
        for (name, model) in sim.presentation_models() {
            if later.live && !sim.is_prepared(name) && !sim.presents(name) && !later.allows() {
                self.held_back = true;
                continue;
            }
            let digest = *self
                .placed
                .attachments
                .model_digests
                .entry(name.to_owned())
                .or_insert_with(|| {
                    sim.model_identity(name)
                        .unwrap_or_else(|| crate::models::model_digest(model))
                });
            let result = renderer.prepare_model_digest(name, model, digest);
            prepared.push((name.to_owned(), result.map_err(|e| e.to_string())));
        }
        // Resident geometry is independent of draw readiness: a texture still in
        // flight hides the model but must not evict this pass's accepted bytes.
        let touched = prepared
            .iter()
            .filter(|(_, result)| result.is_ok())
            .map(|(name, _)| name.clone())
            .collect();
        for (name, result) in prepared {
            sim.asset_prepared(&name, result);
        }
        // Uploads that finished install first: under the agent's clock every one
        // sent, so an inline upload of a newer delivery below replaces them. A
        // name delivered again since its upload was sent (now in `textures` or
        // waiting) has that upload superseded first: its result, a failure
        // included, never reaches the world.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(uploads) = &mut self.uploads {
            for name in textures.keys().chain(self.waiting.keys()) {
                uploads.supersede(name);
            }
            for (name, result) in uploads.finished(!later.live) {
                // A name retired since it was sent has nothing to install; a
                // replacement of one already prepared does (`Uploads` keeps
                // only each name's latest delivery).
                if !sim.is_loaded(&name) {
                    continue;
                }
                let result = result.map(|written| {
                    if !renderer.reactivate_texture(&name, written.digest()) {
                        renderer.install_texture(&name, written);
                    }
                });
                sim.asset_prepared(&name, result);
            }
        }
        // What waits is reconsidered every time, under the presentation and
        // the clock as they are now; a newer delivery of a waiting name replaces it.
        let mut waited = std::mem::take(&mut self.waiting);
        waited.append(&mut textures);
        textures = waited;
        #[cfg(not(target_arch = "wasm32"))]
        if later.live && self.uploads.is_none() {
            self.uploads = crate::models::Uploads::new(device, queue);
        }
        for (name, texture) in textures {
            let presented = sim.presents(&name);
            if later.live && later.first && !presented {
                self.waiting.insert(name, texture);
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            let (name, texture) = match self.uploads.as_mut().filter(|_| later.live) {
                Some(uploads) => match uploads.send(name, texture) {
                    Ok(()) => continue,
                    Err(back) => back,
                },
                None => (name, texture),
            };
            if later.live && !presented && !later.allows() {
                self.waiting.insert(name, texture);
                continue;
            }
            let result = renderer
                .add_texture(&name, &texture)
                .map_err(|e| e.to_string());
            sim.asset_prepared(&name, result);
        }
        // Closure acceptance can reactivate an identical resident model without
        // uploading it. Publish that activity before compaction considers it.
        for (name, model) in &mut renderer.models.loaded {
            let active = sim.model_prepared(name);
            if model.active != active {
                model.active = active;
                renderer.models.revision += 1;
            }
        }
        if renderer.retired_bytes(&live) > RETIRED_BUDGET {
            renderer.compact_assets(&live, &touched);
        }
        self.finish_restore();
        self.assets_dirty = false;
        self.dirty = true;
    }
    pub(super) fn render_frame(
        &mut self,
        frame: &Frame,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> bool {
        self.frame_ready = false;
        if !self.storage_fits(device) {
            return false;
        }
        self.reported = false;
        if self.error.is_some() {
            return false;
        }
        self.device = true;
        self.prepare_assets(device, queue, format);
        let asset_work = self.asset_work();
        let Some(sim) = &mut self.sim else {
            return false;
        };
        if self.generation != sim.world().presentation_generation() {
            self.generation = sim.world().presentation_generation();
            self.dirty = true;
        }
        let drawable = frame.width.is_finite()
            && frame.height.is_finite()
            && frame.width > 0.0
            && frame.height > 0.0
            && frame.scale.is_finite()
            && frame.scale > 0.0;
        if !frame.now_ms.is_finite() {
            return true;
        }
        // The host's display period: the live scheduler looks ahead by it.
        sim.frame_period(frame.period_ms);
        // The first frame after a rest: the idle time runs no ticks.
        if std::mem::take(&mut self.resting) && !frame.seekable {
            sim.wake(frame.now_ms);
            // What woke it may change the pose: rest again only after it draws.
            self.idle_tick = None;
        }
        if !drawable {
            if frame.seekable {
                sim.advance_with(
                    frame.now_ms,
                    Clock::Seekable,
                    observer(
                        &mut self.render,
                        &mut self.placed,
                        &mut self.perf,
                        &mut self.trace,
                        &mut self.error,
                        &mut self.hook_poses,
                        false,
                        0,
                    ),
                );
            }
            self.presentation.sync(
                sim.world(),
                sim.generation(),
                !G::paused(sim.args()),
                frame.seekable,
            );
            self.dirty = true;
            return self.error.is_none();
        }
        if sim.is_loading() {
            sim.advance(
                frame.now_ms,
                if frame.seekable {
                    Clock::Seekable
                } else {
                    Clock::Live
                },
            );
            return sim.assets_pending();
        }
        // After a live first frame no frame waits: an entity whose model is not
        // ready keeps its previous look (#90), and the world keeps ticking.
        let ready = if self.seekable {
            sim.device_assets_ready()
        } else if self.ready_work.is_some() {
            true
        } else {
            sim.refresh_presented();
            sim.presented_assets_ready()
        };
        if ASSETS && !ready {
            return sim.assets_pending() || asset_work;
        }
        // A new world's first size runs `Game::first_frame`: this frame shows it.
        if sim.viewport(frame.width, frame.height) {
            self.dirty = true;
        }
        self.look.points = (frame.width, frame.height);
        self.look.held = Some(*frame);
        if self.format != Some(format) {
            self.gpu_timing = None;
            self.hook_clock.reset();
            self.hooks.device_lost();
            self.ready_work = None;
            self.render = Some((
                crate::renderer::RendererWithAssets::<ASSETS>::configured(
                    device,
                    queue,
                    format,
                    *self
                        .samples
                        .insert(crate::quality::samples(self.rung, G::samples(sim.args()))),
                    G::DEPTH_PREPASS,
                )
                .with_matte_models(G::MATTE_MODELS),
                {
                    let mut feed = Feed::default();
                    feed.share_attachment_diagnostics(self.placed.attachments.diagnostics.clone());
                    feed
                },
            ));
            self.format = Some(format);
            self.dirty = true;
            self.perf.pipeline_ready = crate::perf::timing().then_some(0.);
        }
        if ASSETS {
            if let Some((renderer, _)) = &mut self.render {
                for (name, model) in &mut renderer.models.loaded {
                    let active = sim.model_prepared(name);
                    if model.active != active {
                        model.active = active;
                        renderer.models.revision += 1;
                        self.dirty = true;
                    }
                }
            }
        }
        // Buffers sized for `Game::PREPARE` before the instances that need them.
        if ASSETS && !G::PREPARE.is_empty() {
            if let Some((renderer, _)) = &mut self.render {
                if renderer.reserve_prepared(G::PREPARE) {
                    renderer.models.revision += 1;
                    self.dirty = true;
                }
            }
        }
        // Seed setup/current state before running ticks; no origin streak on frame one.
        if self.dirty {
            if let Some(trace) = &mut self.trace {
                trace.feed(sim.world());
            }
            if let Err(e) = if ASSETS {
                self.placed.feed(sim.world())
            } else {
                self.placed.feed_primitive(sim.world())
            } {
                self.error = Some(SurfaceError(e.to_string()));
                return false;
            }
            let (renderer, feed) = self.render.as_mut().unwrap();
            if let Err(e) = feed.feed(sim.world(), renderer) {
                self.error = Some(SurfaceError(e.to_string()));
                return false;
            }
        }
        // The dirty feed seeds histories; the final observer feed may replace it.
        retain_primes(&mut self.render, &mut self.placed, true);
        self.perf.pixels = frame.pixels();
        self.perf.frame(frame.now_ms, frame.seekable);
        let due = sim.ticks_due(frame.now_ms, Clock::Live);
        let ticks = sim.advance_with(
            frame.now_ms,
            if frame.seekable {
                Clock::Seekable
            } else {
                Clock::Live
            },
            observer(
                &mut self.render,
                &mut self.placed,
                &mut self.perf,
                &mut self.trace,
                &mut self.error,
                &mut self.hook_poses,
                !frame.seekable,
                due,
            ),
        );
        self.presentation.sync(
            sim.world(),
            sim.generation(),
            !G::paused(sim.args()),
            frame.seekable,
        );
        self.perf.ticks.push(ticks as f64);
        if self.error.is_some() {
            return false;
        }
        let (renderer, feed) = self.render.as_mut().unwrap();
        let start = (!frame.seekable).then(Stamp::now);
        self.hook_poses.track(self.hooks.entities(), sim.world());
        self.hook_poses.sync(sim.world());
        let hook_time = if H::ENABLED {
            self.hook_clock.frame(
                frame,
                &self.timing,
                sim.world().presentation_generation(),
                G::paused(sim.args()),
            )
        } else {
            crate::HookTime::default()
        };
        // The agent's armed timing keeps samples a snapshot's does not: arming
        // replaces the snapshot's, and with neither there is none.
        let armed = self.perf.armed();
        if !(armed || self.perf.reporting.is_some())
            || (armed && self.gpu_timing.as_ref().is_some_and(|t| !t.armed()))
        {
            self.gpu_timing = None;
        }
        if feed.mouse_look() {
            feed.unshown_motion(sim.unshown_motion());
        }
        let pointer = feed.pointer_pan().and_then(|id| sim.frame_pointer(id));
        feed.pointer_sample(pointer);
        if let Some(trace) = &mut self.trace {
            trace.pointer = pointer;
        }
        let mut input = feed.frame_pixels(sim.world(), sim.alpha(), (frame.width, frame.height));
        if let Some(rung) = self.rung {
            let authored = sim.world().try_resource::<exact_game::Shadows>().is_some();
            rung.clamp(&mut input, authored);
            renderer.set_water_effects(rung.water_effects());
        }
        // Every pixel the renderer stores has alpha one: each clear of the
        // output stores one, opaque draws write one, and every blend composes
        // alpha OVER, which keeps a destination alpha of one at one (tests
        // `every_output_pixel_stores_alpha_one`). Hooks can write any alpha.
        self.output_opaque = !H::ENABLED;
        self.perf.lights(input.lights.len(), input.lights_dropped);
        input.headroom = frame.headroom;
        // Armed perf adds asynchronous pass timings and per-view culling counts.
        // A snapshot's reader adds the pass timings alone (`gpuMs`).
        let armed = self.perf.armed();
        if armed || self.perf.reporting.is_some() {
            if armed {
                renderer.count_culled(true);
            }
            if self.gpu_timing.is_none() {
                self.gpu_timing = crate::hooks::gpu_timing::GpuTiming::new(device, queue, armed);
                if H::ENABLED && armed && self.gpu_timing.is_some() {
                    renderer
                        .hook_metrics
                        .get_or_insert_with(Default::default)
                        .engine
                        .buffers += crate::hooks::gpu_timing::GpuTiming::BUFFERS;
                }
            }
            if H::ENABLED && armed {
                renderer
                    .hook_metrics
                    .get_or_insert_with(Default::default)
                    .arm(false);
            }
            if let Some(timing) = &mut self.gpu_timing {
                timing.poll(device, queue);
                input.timestamps = timing.query();
            }
        }
        self.placed
            .frame(&input, exact_game::Vec2::new(frame.width, frame.height));
        #[cfg(not(target_arch = "wasm32"))]
        renderer
            .quads
            .children(&renderer.device, &renderer.queue, &self.placed);
        let direct_frames = renderer.direct_frames;
        self.perf.stats = match renderer.encode_hooked(
            encoder,
            target,
            frame.pixels(),
            &input,
            &mut self.hooks,
            Some(sim.world()),
            hook_time,
            &self.hook_poses,
        ) {
            Ok(stats) => stats,
            Err(error) => {
                self.error = Some(SurfaceError(error.to_string()));
                return false;
            }
        };
        self.hook_poses.track(self.hooks.entities(), sim.world());
        // Pipeline time, timed on live frames only, since the renderer was made.
        // A seekable frame times nothing and leaves earlier live time pending.
        let mut pipeline = crate::perf::pipeline_ms();
        if start.is_some() {
            pipeline += std::mem::take(&mut self.perf.pipeline_pending);
            self.perf.pipeline.push(pipeline);
        }
        if let Some(ms) = &mut self.perf.pipeline_ready {
            *ms += pipeline;
        }
        if frame.seekable {
            self.perf.pipeline_ready = None;
        }
        if !self.hooks.needs().contains(crate::Needs::PENDING)
            && self.hooks.error().is_none()
            && self.ready_work.is_none()
        {
            let pipeline_ms = self
                .perf
                .pipeline_ready
                .map_or("untimed".into(), |ms| format!("{ms:.1}"));
            eprintln!(
                "game renderer first frame encoded: {}x{} pipelines={} pipeline_ms={pipeline_ms} direct={} f16={}",
                frame.pixels().0,
                frame.pixels().1,
                renderer.residency_work().pipeline_creations,
                self.perf.stats.direct_frames > direct_frames,
                renderer.pipelines.half
            );
            self.ready_work = Some(renderer.residency_work());
        }
        self.frame_ready =
            !self.hooks.needs().contains(crate::Needs::PENDING) && self.hooks.error().is_none();
        if let Some(start) = start {
            let ms = start.elapsed();
            self.perf.encode.push(ms);
            if let Some(report) = &mut self.perf.reporting {
                report.phases[2].push(ms);
            }
            if let Some(trace) = &mut self.trace {
                trace.times[2] = ms;
            }
        }
        if let Some(trace) = &mut self.trace {
            trace.projection[..16]
                .copy_from_slice(&(input.proj * input.view).to_cols_array().map(f64::from));
            let (width, height) = frame.pixels();
            trace.projection[16] = f64::from(width);
            trace.projection[17] = f64::from(height);
            trace.frame(
                frame.now_ms,
                sim.alpha(),
                ticks,
                input.camera_position.to_array().map(f64::from),
                trace.times,
            );
            trace.times = [0.; 3];
        }
        // Timed once submitted (`submitted`): the module submits the frame.
        self.encoded = Some(renderer.timed.get());
        if self.perf.armed() {
            self.perf.culled = renderer.culled();
            self.perf.culled_triangles = renderer.culled_triangles();
        }
        let wants = !G::paused(sim.args()) || ticks != 0;
        retain_primes(&mut self.render, &mut self.placed, false);
        self.dirty = false;
        // At rest once idle at two ticks: this frame drew between idle poses.
        let rests = if !frame.seekable && sim.render_idle() {
            let tick = sim.world().tick();
            tick > *self.idle_tick.get_or_insert(tick)
        } else {
            self.idle_tick = None;
            false
        };
        let hooks = self.hooks.needs().contains(crate::Needs::ANIMATE)
            || self.hooks.needs().contains(crate::Needs::PENDING);
        self.resting = rests && !hooks && !asset_work;
        (wants && !rests) || hooks || asset_work
    }

    /// Live asset work left for later frames: textures waiting for the first
    /// frame or the frame's budget, uploads on the worker, models held back.
    pub(super) fn asset_work(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if self
            .uploads
            .as_ref()
            .is_some_and(crate::models::Uploads::busy)
        {
            return true;
        }
        self.held_back || !self.waiting.is_empty()
    }

    /// A display frame the module does not render: run the ticks due by
    /// `frame` and feed them, as the next `render_frame` would. At 256,000
    /// moving cubes on a Pixel 5 a tick and its upload take 8 ms, so a frame
    /// that waited for the GPU and then ran two missed its display frame too.
    /// Only for a plain live frame with nothing pending first: a frame to
    /// seed, assets, hooks.
    pub(super) fn advance_frame(&mut self, frame: &Frame) {
        if H::ENABLED
            || self.dirty
            || self.error.is_some()
            || frame.seekable
            || self.render.is_none()
        {
            return;
        }
        let Some(sim) = &mut self.sim else {
            return;
        };
        if !frame.now_ms.is_finite()
            || sim.is_loading()
            || (ASSETS && self.ready_work.is_none())
            || self.generation != sim.world().presentation_generation()
        {
            return;
        }
        let due = sim.ticks_due(frame.now_ms, Clock::Live);
        if due == 0 {
            return;
        }
        sim.frame_period(frame.period_ms);
        sim.advance_with(
            frame.now_ms,
            Clock::Live,
            observer(
                &mut self.render,
                &mut self.placed,
                &mut self.perf,
                &mut self.trace,
                &mut self.error,
                &mut self.hook_poses,
                true,
                due,
            ),
        );
        self.presentation
            .sync(sim.world(), sim.generation(), !G::paused(sim.args()), false);
    }

    /// The module submitted the frame `render_frame` encoded: map its
    /// readbacks and start its pass timings.
    pub(super) fn frame_submitted(&mut self) {
        let Some(timed) = self.encoded.take() else {
            return;
        };
        let Some((renderer, _)) = &mut self.render else {
            return;
        };
        renderer.submitted();
        if let Some(report) = &mut self.perf.reporting {
            report.submitted();
        }
        // `Game::PREPARE`'s pipelines, after the frame that sets their keys is
        // on its way: their time counts in the next frame's `pipelineMs`.
        if ASSETS && !G::PREPARE.is_empty() && self.ready_work.is_some() {
            let _timing = crate::perf::PipelineTiming::start(self.perf.live);
            renderer.warm_prepared(G::PREPARE);
            self.perf.pipeline_pending += crate::perf::pipeline_ms();
        }
        let frame = self.sim.as_mut().map_or(0, Sim::trace_frame);
        if let Some(timing) = &mut self.gpu_timing {
            timing.submitted(&renderer.queue, timed, frame);
        }
    }
}

pub(super) fn append_trace(
    out: &mut String,
    mut events: String,
    timing: Option<&crate::hooks::gpu_timing::GpuTiming>,
) {
    events.pop();
    if let Some(timing) = timing {
        let gpu = timing.trace_json();
        if gpu.len() > 2 {
            if events.len() > 1 {
                events.push(',');
            }
            events.push_str(&gpu[1..gpu.len() - 1]);
        }
    }
    events.push(']');
    out.push_str(",\"traceEvents\":");
    out.push_str(&events);
}
