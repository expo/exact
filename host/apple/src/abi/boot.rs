//! One session's boots from the input buffer: prepare, then commit.

use super::*;
use exact_runner::DataSource;

pub(super) struct PreparedHost<D: DataSource> {
    pub(super) host: crate::host::Host<D>,
    pub(super) batch: String,
    pub(super) bindings: Option<ibex2::host::Bindings>,
    pub(super) hooks: Hooks,
    pub(super) module: bool,
    pub(super) let_go: bool,
    pub(super) carried_from: Option<exact_runner::Carried>,
}

impl<D: DataSource> Bridge<D> {
    /// Prepare and commit a single session's plan replacement.
    pub fn boot_plan(&mut self, len: usize, data: D, hooks: Hooks, width: f32, height: f32) -> u32 {
        let len = self.prepare_plan(len, data, hooks, width, height);
        if self.prepared.is_some() {
            self.commit_plan()
        } else {
            len
        }
    }
    /// Gallery refresh uses a fresh data seam and an empty, disposable store.
    /// Host activation would reset the running app's picked-file roots.
    pub fn refresh_entry(&mut self, mut data: D, now: f64) -> u32 {
        use exact_runner::{Answer, Store, Value};
        if !now.is_finite() {
            return self.refuse("entry refresh date must be finite");
        }
        let Some(host) = &self.host else {
            return self.emit(super::not_booted());
        };
        let plan = host.runner().plan().clone();
        let Some(source) = plan.snapshot_refresh else {
            return self.refuse("not a snapshot entry");
        };
        let signature = plan.source(source);
        data.bind(&plan);
        let mut store = Store::new(data.grants(), []);
        let answer = exact_runner::time::launching_with(self.launch_facts.clone(), || {
            data.activate()?;
            data.answer(&mut store, plan.str(signature.name), &[Value::Number(now)])
        });
        let result = match answer {
            Ok(Answer::Now(value)) if value.conforms(&plan, signature.ty) => {
                let mut json = String::new();
                exact_runner::agent::typed_json(&plan, signature.ty, &value, &mut json);
                serde_json::json!({"entryRefresh":"timeline", "timeline":serde_json::from_str::<serde_json::Value>(&json).unwrap()})
            }
            Ok(Answer::Now(_)) => {
                return self.refuse("entry refresh does not conform to its timeline type")
            }
            Ok(Answer::Later(request)) => {
                if let Some(token) = request.continuation {
                    data.discard(token);
                }
                serde_json::json!({"entryRefresh":"pending"})
            }
            Err(error) => {
                serde_json::json!({"entryRefresh":"unavailable", "reason":error.to_string()})
            }
        };
        self.emit(result.to_string())
    }
}

impl<D: DataSource> Bridge<D> {
    pub(super) fn boot_fresh(
        &mut self,
        plan: PlanBytes<'_>,
        mut data: D,
        hooks: Hooks,
        width: f32,
        height: f32,
    ) -> Result<String, String> {
        data.set_document_owner(self.document_owner);
        if let Some(compat) = self.compat {
            exact_runner::delivery::refuse_analysis(compat).map_err(str::to_string)?;
        }
        let measurer: Box<dyn TextMeasurer> = match hooks.measure {
            Some(f) => Box::new(
                CallbackMeasurer::of(f, &hooks)
                    .with_field_chrome(self.field_chrome)
                    .with_button_measure(self.button_measure)
                    .with_measure_revision(
                        self.measure_revision
                            .get_or_insert_with(|| Rc::new(std::cell::Cell::new(0)))
                            .clone(),
                    ),
            ),
            None => Box::new(MonospaceMeasurer::default()),
        };
        // The app's bindings, once (LLP 1016 D6; LLP 1018 D6): the secrets it
        // kept are read into a snapshot before the runner boots, so the first
        // frame is a returning user's; the executor thread takes the same
        // bindings for its requests. Build beside any running host: the dev
        // menu may use this fresh-state path to reload the baked plan.
        // A fresh named drive empties leftover secrets before that read.
        // An archive that links no I/O binds nothing (LLP 1047.001).
        let launch_store = self
            .links
            .io
            .and_then(|io| (io.preloads)())
            .map(|launch| launch.watch_store(|| false));
        let crate::store::Endowed {
            bindings,
            snapshot,
            unread_kept,
            secrets,
            unbound,
            presence,
        } = match self.links.io {
            Some(io) => (io.endow)(data.grants(), data.app_id(), true),
            None => crate::store::Endowed::none(),
        };
        let fonts = self.fonts;
        let fonts_ctx = self.fonts_ctx;
        let control_text = self.control_text;
        let native_timers = self.native_timers;
        let wall = self.live_clock;
        let ctx = hooks.ctx;
        let launch = self.launch();
        match exact_runner::time::launching_with(launch, || {
            Host::boot_stored_after_decode(
                plan,
                data,
                measurer,
                self.launch_layout,
                self.boot_viewport(width, height),
                None,
                snapshot,
                secrets,
                self.compat,
                self.delivery,
                None,
                self.launch.as_deref().unwrap_or("/"),
                self.start_on_frame,
                self.region,
                self.links.clone(),
                fonts.map(|callback| (callback, fonts_ctx)),
                move |runner| {
                    if native_timers {
                        runner.use_native_timers(true);
                        runner.set_live_clock(wall.map(preferences::sampler));
                    }
                    if let Some(callback) = fonts {
                        let stacks = runner.runtime_font_stacks();
                        install_fonts(runner.plan(), &stacks, callback, fonts_ctx);
                    }
                    control_text::prepare(runner, control_text, ctx)
                },
            )
        }) {
            Ok((mut host, batch)) => {
                host.launch_store = launch_store;
                if let Some(why) = unbound {
                    host.log(&format!("{why}; every request is refused"));
                }
                host.runner_mut().forget_unread_kept(&unread_kept);
                if let Some((backend, keystore)) = presence {
                    host.runner_mut().install_presence(backend, keystore);
                }
                host.commit_boot();
                let executor = self.links.io.map(|io| {
                    (io.start)(
                        bindings,
                        &host.grants(),
                        hooks.wake.map(|w| (w, hooks.wake_ctx)),
                    )
                });
                if let Some(executor) = &executor {
                    if let Some(preloads) = &host.preloads {
                        preloads.adopt(&**executor);
                    }
                    if let Some(roots) = host.app_roots() {
                        executor.set_app_roots(roots);
                    }
                    host.listen_io(&**executor, self.document_owner);
                }
                self.presenter_hooks(&mut host, &hooks);
                self.executor = executor;
                let batch = Host::ending(self.host.replace(host), batch);
                self.parked.clear();
                Ok(batch)
            }
            Err(e) => Err(format!("{e:?}")),
        }
    }
}
