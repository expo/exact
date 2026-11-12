//! Rust argument defaults and atomic timed binding for world surfaces, and
//! the quality ladder's rung the first binding resolves (LLP 1139.003 D3).
use super::{observer, Clock, Executor, Game, Sim, SurfaceError, Value, WorldSurface};
use exact_game::Args;

impl<G: Game, P: Executor, const ASSETS: bool, H: crate::Hooks> WorldSurface<G, P, ASSETS, H> {
    /// The world raster's scale: the rung's choice, or the game's own.
    pub(super) fn raster_scale(&self) -> f32 {
        let game = self
            .sim
            .as_ref()
            .map_or(exact_game::AUTO_RENDER_SCALE, |sim| {
                G::render_scale(sim.args())
            });
        self.rung
            .map_or(crate::quality::unresolved_scale(game), |rung| {
                rung.scale(game)
            })
    }
    /// The device's tier changed (`prefer gpu-tier`): the rung is resolved
    /// again, and a renderer made at another rung is released. True when a
    /// live renderer was released.
    pub(super) fn retier_rung(&mut self) -> bool {
        let Some(sim) = &self.sim else { return false };
        let next = crate::quality::Rung::resolve(G::quality(sim.args()), exact_gpu::tier());
        if self.rung == Some(next) {
            return false;
        }
        self.rung = Some(next);
        self.release_renderer()
    }
    /// Release the renderer, as on a lost device, to be made again under the
    /// same world; a look frame's own renderer and a readback in flight go
    /// with it. The placed children's textures are the host's on the same
    /// device and stay. True when a live renderer was released.
    fn release_renderer(&mut self) -> bool {
        self.look.retiered();
        if self.render.is_none() {
            return false;
        }
        let children: Vec<_> = self
            .placed
            .children
            .iter()
            .map(|c| c.texture.clone())
            .collect();
        // The device stays: its assets are prepared again for the new
        // renderer, as often as it is released, and a later loss of the
        // device invalidates them again.
        self.device_assets_invalidated = false;
        exact_gpu::Surface::device_lost(self);
        self.device_assets_invalidated = false;
        for (child, texture) in self.placed.children.iter_mut().zip(children) {
            child.texture = texture;
        }
        true
    }
    pub(super) fn surface_arguments(&self) -> Vec<(&'static str, Value)> {
        G::Args::FIELDS
            .iter()
            .map(|(name, _)| *name)
            .zip(G::Args::default().values())
            .collect()
    }
    pub(super) fn bind_arguments(
        &mut self,
        values: &[Value],
        at_ms: Option<f64>,
    ) -> Result<(), SurfaceError> {
        if at_ms.is_some_and(|at| !at.is_finite()) {
            return Err(SurfaceError("bind clock must be finite".into()));
        }
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        let mut resample = false;
        if let Some(sim) = &mut self.sim {
            let generation = sim.generation();
            let models = sim.world().model_revision();
            // A live bind while at rest: the idle time runs no ticks.
            let woke = at_ms.filter(|_| self.resting && !self.seekable);
            if let Some(at) = woke {
                sim.wake(at);
            }
            sim.bind_with(
                values,
                at_ms,
                if self.seekable {
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
                    false,
                    0,
                ),
            )
            .map_err(SurfaceError)?;
            // An accepted bind: the next frame advances from it, not from the rest.
            if woke.is_some() {
                self.idle_tick = None;
                self.resting = false;
            }
            self.assets_dirty |= models != sim.world().model_revision();
            // New scene samples (`Game::samples`) make the renderer again.
            let samples = crate::quality::samples(self.rung, G::samples(sim.args()));
            resample = self.samples.is_some_and(|made| made != samples);
            if generation != sim.generation() {
                self.hook_clock.reset();
                if let Some((_, feed)) = &mut self.render {
                    feed.reset();
                }
                self.perf.restart();
                self.gpu_timing = None;
            }
        } else {
            let mut sim = Sim::from_values(values).map_err(SurfaceError)?;
            sim.development_edits(self.development_edits);
            sim.defer_assets(ASSETS && self.device);
            if let Some(at) = at_ms {
                sim.advance(at, Clock::Seekable);
            }
            // The quality ladder's rung (LLP 1139.003 D3): read once, from the
            // first arguments and the device's tier, before any renderer exists.
            self.rung.get_or_insert_with(|| {
                crate::quality::Rung::resolve(G::quality(sim.args()), exact_gpu::tier())
            });
            self.sim = Some(sim);
        }
        if resample {
            self.release_renderer();
        }
        if let Err(error) = self.check_primitive_assets() {
            self.sim = None;
            self.asset_check = None;
            return Err(error);
        }
        self.dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_gpu::Surface;

    #[derive(Default, exact_game::Args)]
    struct Options {
        #[live]
        paused: bool,
    }
    struct Counter;
    impl Game for Counter {
        const ID: &'static str = "bind-clock";
        type Args = Options;
        fn setup(_: &mut exact_game::World, _: &Options) {}
        fn tick(_: &mut exact_game::World, _: &exact_game::Input, _: &Options) {}
        fn paused(args: &Options) -> bool {
            args.paused
        }
    }

    #[test]
    fn stamped_bind_uses_the_surface_clock_before_the_next_frame() {
        for seekable in [true, false] {
            for paused in [true, false] {
                let mut surface = WorldSurface::<Counter>::default();
                surface.clock(false);
                surface.bind(&[Value::Bool(false)], None).unwrap();
                let sim = surface.sim.as_mut().unwrap();
                sim.frame_period(1000. / 60.);
                sim.advance(0., Clock::Live);
                sim.advance(100., Clock::Live);
                assert_eq!(sim.world().tick(), 6);

                surface.clock(seekable);
                surface.bind(&[Value::Bool(paused)], Some(1100.)).unwrap();
                let sim = surface.sim.as_mut().unwrap();
                let expected = if seekable { 66 } else { 21 };
                assert_eq!(
                    sim.world().tick(),
                    expected,
                    "seekable={seekable}, paused={paused}"
                );
                let clock = if seekable {
                    Clock::Seekable
                } else {
                    Clock::Live
                };
                assert_eq!(sim.advance(1100., clock), 0);
                assert_eq!(sim.world().tick(), expected);
                if paused {
                    assert_eq!(sim.advance(2100., clock), 0);
                    assert_eq!(sim.world().tick(), expected);
                }
            }
        }
    }

    #[test]
    fn stamped_binds_before_the_first_live_frame_keep_seekable_time() {
        let mut surface = WorldSurface::<Counter>::default();
        surface.clock(false);
        surface.bind(&[Value::Bool(false)], Some(0.)).unwrap();
        surface.bind(&[Value::Bool(false)], Some(1000.)).unwrap();
        let sim = surface.sim.as_mut().unwrap();
        assert_eq!(sim.world().tick(), 60);
        sim.frame_period(1000. / 60.);
        sim.advance(1000., Clock::Live);
        assert_eq!(sim.world().tick(), 60);
        assert_eq!(sim.alpha(), 1.);
    }
}
