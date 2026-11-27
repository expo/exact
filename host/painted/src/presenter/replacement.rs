//! A replacement commits between interactions, and the presenter keeps the
//! candidate that has to wait (LLP 1029.000.002 D7, D9).
use super::*;
use crate::host::LET_GO_AFTER;
use std::time::Instant;

/// What a replacement answers at once. A refusal is the `Err` beside it.
#[derive(Debug, PartialEq)]
pub enum ReloadResult {
    /// Committed, with the presentation's error, if any.
    Applied(Option<String>),
    /// Kept by the presenter, unprepared, until this clears: `composition`,
    /// `pointer`, `gesture`, `module` or `requests`.
    Held(&'static str),
}

/// How a held candidate ended. Each is told once.
#[derive(Debug, PartialEq)]
pub enum ReplacementOutcome {
    /// It committed.
    Applied,
    /// It failed when it was prepared, and is not tried again.
    Refused(String),
    /// A newer candidate took its place.
    Superseded,
    /// Its presenter went first.
    Dropped,
}

/// A held candidate: bytes and the data source, never a prepared runner, so
/// the state it carries is the state at its commit.
pub(super) struct Candidate<D> {
    plan: Vec<u8>,
    data: Option<D>,
    module: Option<crate::delivery::Module>,
    requests_since: Option<Instant>,
    hold: &'static str,
    finished: Option<Box<dyn FnOnce(ReplacementOutcome)>>,
}

impl<D> Candidate<D> {
    pub(super) fn finish(&mut self, outcome: ReplacementOutcome) {
        if let Some(finished) = self.finished.take() {
            finished(outcome);
        }
    }
}

impl<D> Drop for Candidate<D> {
    fn drop(&mut self) {
        self.finish(ReplacementOutcome::Dropped);
    }
}

/// The newest complete candidate that has to wait (D9). The presenter keeps
/// one, whichever producer brought it: a newer one takes its place.
pub(super) enum Waiting<D> {
    /// A local pair, as bytes.
    Reload(Candidate<D>),
    /// A generation the development URL's poll downloaded, and what its
    /// last attempt waited for.
    Generation(crate::fetch::Generation, Option<&'static str>),
    /// The bundle the update store has staged: whether `deliveryActivate`
    /// asked for it, and what its last attempt waited for.
    Update {
        explicit: bool,
        hold: Option<&'static str>,
    },
}

impl<D: DataSource> Presenter<D> {
    /// A newer candidate arrived, to wait or to apply: the one that waited
    /// does not apply. A staged update stays staged.
    pub(super) fn supersede(&mut self) {
        match self.waiting.take() {
            Some(Waiting::Reload(mut held)) => held.finish(ReplacementOutcome::Superseded),
            Some(Waiting::Generation(..)) => {
                if let Some(dev) = self.dev.as_mut() {
                    dev.held_since = None;
                }
            }
            Some(Waiting::Update { .. }) | None => {}
        }
    }

    /// A generation the poll downloaded is the newest candidate. The one
    /// already waiting, brought again, keeps what it waited for.
    pub(super) fn keep_generation(&mut self, generation: crate::fetch::Generation) {
        let hold = match &self.waiting {
            Some(Waiting::Generation(waits, hold)) if waits.identity == generation.identity => {
                *hold
            }
            _ => {
                self.supersede();
                None
            }
        };
        self.waiting = Some(Waiting::Generation(generation, hold));
    }

    /// The staged update is to apply (`deliveryActivate`, or the cadence
    /// under `immediate`): the newest candidate, unless it waits already.
    /// With nothing staged there is no candidate, and one that waits stays.
    pub(super) fn owe_update(&mut self, explicit: bool) {
        let explicit = match self.waiting {
            Some(Waiting::Update {
                explicit: asked, ..
            }) => explicit || asked,
            Some(_) if !self.host.runner().delivery().staged => return,
            _ => {
                self.supersede();
                explicit
            }
        };
        self.waiting = Some(Waiting::Update {
            explicit,
            hold: None,
        });
    }

    #[cfg(test)]
    pub(super) fn update_owed(&self) -> bool {
        matches!(self.waiting, Some(Waiting::Update { .. }))
    }

    #[cfg(test)]
    pub(super) fn update_hold(&self) -> Option<&'static str> {
        match self.waiting {
            Some(Waiting::Update { hold, .. }) => hold,
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn generation_waits(&self) -> bool {
        matches!(self.waiting, Some(Waiting::Generation(..)))
    }

    /// The dev loop's restart, state carried; pictures and offsets go, focus
    /// stays (LLP 1007 §6).
    pub fn reload(&mut self, plan: &[u8], data: D) -> Result<ReloadResult, HostError> {
        self.reload_module(plan, data, self.module.clone(), false)
    }

    /// Replace a local development pair, carrying compatible state. It
    /// commits now, or the presenter keeps it and commits it at the first
    /// [`Presenter::run_commands`] with no hold.
    pub fn reload_module(
        &mut self,
        plan: &[u8],
        data: D,
        module: Option<crate::delivery::Module>,
        let_go: bool,
    ) -> Result<ReloadResult, HostError> {
        self.submit_reload(plan, data, module, let_go, None)
    }

    /// [`Presenter::reload_module`], told how a held candidate ends.
    pub fn reload_with_outcome(
        &mut self,
        plan: &[u8],
        data: D,
        module: Option<crate::delivery::Module>,
        let_go: bool,
        finished: impl FnOnce(ReplacementOutcome) + 'static,
    ) -> Result<ReloadResult, HostError> {
        self.submit_reload(plan, data, module, let_go, Some(Box::new(finished)))
    }

    fn submit_reload(
        &mut self,
        plan: &[u8],
        data: D,
        module: Option<crate::delivery::Module>,
        let_go: bool,
        finished: Option<Box<dyn FnOnce(ReplacementOutcome)>>,
    ) -> Result<ReloadResult, HostError> {
        self.content_region_refuses()?;
        self.supersede();
        let module = module.or_else(|| self.module.clone());
        let mut candidate = Candidate {
            plan: plan.to_vec(),
            data: Some(data),
            module,
            requests_since: None,
            hold: "",
            // Its producer hears an outcome only once it is held: before
            // that an error is its whole answer.
            finished: None,
        };
        if let Some(hold) = self.reload_hold(&mut candidate, let_go)? {
            candidate.hold = hold;
            candidate.finished = finished;
            self.waiting = Some(Waiting::Reload(candidate));
            return Ok(ReloadResult::Held(hold));
        }
        let data = candidate.data.take().unwrap();
        self.reload_now(&candidate.plan, data, candidate.module.take(), let_go)
            .map(ReloadResult::Applied)
    }

    /// What the person holds a replacement for, if anything (D7, D9; LLP
    /// 1030.003 D5): a composition; a button or a touch down as the shell
    /// delivered it, before any routing; the system's predictive Back; or a
    /// row the keys lifted, which holds no button (LLP 1094 D9).
    pub(super) fn interaction_hold(&self) -> Option<&'static str> {
        if self.composing_field().is_some() || self.hosted_composing() {
            Some("composition")
        } else if self.pressed != 0 || self.touching || self.surface_pressed() {
            Some("pointer")
        } else if self.back_previewing() || self.group_keyed() {
            Some("gesture")
        } else {
            None
        }
    }

    /// The shell's word that the press the presenter heard ended with no
    /// `pointer_up` here: its routing took the touch (Android's reader, a
    /// second finger on Windows) or the system cancelled it.
    pub fn pointer_released(&mut self) {
        self.buttons_released(1);
    }

    /// The shell's word that buttons `bits` (as [`Presenter::pointer_aux`]
    /// numbers them, 1 the primary) are up with no release here: their
    /// releases went to another window, and the hardware says so.
    pub fn buttons_released(&mut self, bits: u32) {
        self.pressed &= !bits;
    }

    /// A touch as Android's reader says it: 0 down, 1 up, 2 move, 3 its
    /// cancel, 4 held for the long-press time. The cancel ends the press
    /// the presenter heard and says nothing of the finger: the reader's
    /// scroll, a field or a hosted view takes a touch that is still down,
    /// and no up follows it here ([`Presenter::set_touching`] says when the
    /// finger lifts).
    pub fn reader_touch(
        &mut self,
        action: i32,
        x: f32,
        y: f32,
        now: f64,
        at: f64,
    ) -> Result<(), String> {
        if action != 2 {
            self.top.lift = None;
        }
        match action {
            0 => {
                if let Some(key) = self.reader.pending_height_end.take() {
                    if let Some(port) = self.host.kernel().node_by_key(key).map(|n| n.id) {
                        if let Some(error) = self.list_drag_ended(port, (0., 0.), None, false) {
                            self.host.log(error);
                        }
                    }
                }
                self.pointer_down_at(x, y, now, at).map(|_| ())
            }
            1 => {
                self.reader.lifting_touch = true;
                self.reader.height_scroll_release = None;
                let result = self.pointer_up_at(x, y, now, at);
                self.reader.lifting_touch = false;
                result.map(|_| ())
            }
            // Drag-out hands the contact to the shell and closes its menu at presentation time.
            2 if self.lift_moved(x, y, now) => Ok(()),
            2 => self.pointer_move_at(x, y, now, at).map(|_| ()),
            4 => {
                self.long_press(x, y, now);
                Ok(())
            }
            _ => {
                self.pointer_released();
                self.pointer_cancel(now)
            }
        }
    }

    /// Whether a touch is down on the shell's view, for a shell that routes
    /// touches before the presenter hears them (Android's reader, Windows'
    /// fingers): from the first finger's down to the last one's lift or the
    /// system's cancel, whoever the shell gave the touch to. Whether a
    /// replacement was waiting, and so wants a turn.
    pub fn set_touching(&mut self, on: bool) -> bool {
        self.touching = on;
        !on && self.replacement_pending().is_some()
    }

    /// The interaction hold, then today's rules for a module pair: its
    /// module still preparing, or requests in flight until the dev loop lets
    /// them go.
    fn reload_hold(
        &self,
        candidate: &mut Candidate<D>,
        let_go: bool,
    ) -> Result<Option<&'static str>, HostError> {
        if let Some(hold) = self.interaction_hold() {
            return Ok(Some(hold));
        }
        let Some(module) = &candidate.module else {
            return Ok(None);
        };
        if self.painted {
            let data = module
                .replacement(&candidate.plan, candidate.data.as_ref().unwrap())
                .map_err(HostError::Asset)?;
            if !data
                .preload()
                .map_err(|e| HostError::Asset(format!("candidate module: {e:?}")))?
            {
                let wake = self.executor.waker();
                data.when_preloaded(Box::new(move || wake()));
                return Ok(Some("module"));
            }
        }
        if !let_go && self.host.runner().has_pending() {
            candidate.requests_since.get_or_insert_with(Instant::now);
            return Ok(Some("requests"));
        }
        Ok(None)
    }

    /// Commit a held reload whose hold has cleared.
    pub(super) fn resume_reload(&mut self) {
        let reload = |waiting: &mut Waiting<D>| matches!(waiting, Waiting::Reload(_));
        let Some(Waiting::Reload(mut candidate)) = self.waiting.take_if(reload) else {
            return;
        };
        let let_go = candidate
            .requests_since
            .is_some_and(|since| since.elapsed() >= LET_GO_AFTER);
        let result = match self.reload_hold(&mut candidate, let_go) {
            Ok(Some(hold)) => {
                candidate.hold = hold;
                self.waiting = Some(Waiting::Reload(candidate));
                return;
            }
            Ok(None) => {
                let data = candidate.data.take().unwrap();
                self.reload_now(&candidate.plan, data, candidate.module.take(), let_go)
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(_) => candidate.finish(ReplacementOutcome::Applied),
            Err(error) => {
                self.host.log(format!("replacement refused: {error}"));
                candidate.finish(ReplacementOutcome::Refused(error.to_string()));
            }
        }
    }

    /// Milliseconds until a candidate held for requests lets them go.
    pub(super) fn reload_due_in_ms(&self) -> Option<f64> {
        let Some(Waiting::Reload(held)) = &self.waiting else {
            return None;
        };
        if held.hold != "requests" {
            return None;
        }
        Some(
            LET_GO_AFTER
                .saturating_sub(held.requests_since?.elapsed())
                .as_secs_f64()
                * 1000.0,
        )
    }

    /// What a replacement waits for now (`state.replacement.pending`), or
    /// nothing when none waits.
    pub fn replacement_pending(&self) -> Option<&'static str> {
        let own = match self.waiting.as_ref()? {
            Waiting::Reload(held) => Some(held.hold),
            Waiting::Generation(_, hold) | Waiting::Update { hold, .. } => *hold,
        };
        Some(self.interaction_hold().or(own).unwrap_or("preparing"))
    }

    /// The host's own section of the agent's `state`, the painter's.
    pub(crate) fn state_sections(&self) -> String {
        format!(",\"painter\":{}", self.painter_state())
    }

    /// Tells the runner what a replacement waits for, which its `state`
    /// reports with the last replacement's report (`state.replacement`).
    pub(crate) fn tell_replacement_pending(&mut self) {
        let pending = self.replacement_pending();
        self.host.runner_mut().set_replacement_pending(pending);
    }

    fn content_region_refuses(&self) -> Result<(), HostError> {
        match self.content_registration {
            Some(_) => Err(HostError::Layout(
                "content-region trial requires session retirement before reload".into(),
            )),
            None => Ok(()),
        }
    }

    /// Restart on a new plan with carry, now.
    pub(super) fn reload_now(
        &mut self,
        plan: &[u8],
        data: D,
        module: Option<crate::delivery::Module>,
        let_go: bool,
    ) -> Result<Option<String>, HostError> {
        self.content_region_refuses()?;
        let module = module.or_else(|| self.module.clone());
        let decoded = Plan::decode(plan).map_err(HostError::Plan)?;
        // Preserve live font state until the candidate runner boots successfully.
        let candidate_text = (self.make_text)(&decoded, &self.assets);
        if let Some(reason) = self.assets.take_refusal() {
            return Err(HostError::Asset(reason));
        }
        let kept = self.focus_place();
        let mut carried = self.host.carry();
        let data = self.prepare_logic(
            plan,
            data,
            module.as_ref(),
            &candidate_text,
            &mut carried,
            self.host.runner().delivery(),
            let_go,
        )?;
        let (mut host, error) = exact_runner::time::launching_with(
            exact_runner::time::Launch {
                preferences: Some(self.host.runner().viewport().preferences),
                time: Some(self.host.runner().wall_time()),
                place: Some(self.host.runner().place().clone()),
                page: Some(self.candidate_page()),
                ..exact_runner::time::Launch::NONE
            },
            || {
                Host::boot_candidate(
                    plan,
                    data,
                    Box::new(Measurer(candidate_text.clone())),
                    self.viewport.0,
                    self.viewport.1,
                    Some(&carried),
                    Some(self.host.runner().delivery().clone()),
                )
            },
        )?;
        if let Some(error) = error {
            return Err(HostError::Layout(error));
        }
        self.restore_time(&mut host)?;
        let shaders = self
            .surfaces
            .prepare_shaders(&self.compat, &self.assets)
            .map_err(HostError::Asset)?;
        self.surfaces
            .commit_shaders(shaders)
            .map_err(HostError::Asset)?;
        let faults = self.host.runner().faults().clone();
        host.install_presence();
        self.host = host;
        self.host.runner_mut().share_faults(&faults);
        // The old executor lets go of everything, then says which uploads had
        // begun to go out; the rest never left (LLP 1016.000 D10).
        self.executor.forget(|_| false);
        let started = self.executor.let_go_started(|_| false);
        self.executor = self.host.executor_with(self.start_executor);
        self.host.runner_mut().uploads_started(&started);
        self.replaced();
        if self.display.new_session() {
            self.painted = false;
        }
        self.activation_failed = None;
        self.module = module;
        self.text = candidate_text.clone();
        self.brush.text = candidate_text;
        self.images.reset();
        self.packages_publish_to_images();
        self.restore_focus(kept);
        let e = self.after_commit();
        Ok(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_runner::{Answer, DataError, Store, Value};
    use std::{cell::Cell, rc::Rc};
    /// The host work these tests hand out (LLP 1047.001 D12).
    struct TestWork;
    impl exact_runner::HostWork for TestWork {}

    #[derive(Clone)]
    struct Loading(Rc<Cell<bool>>);
    impl DataSource for Loading {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            Ok(Answer::Later(
                exact_runner::Request::continuation::<TestWork>(1),
            ))
        }
        fn preload(&self) -> Result<bool, DataError> {
            Ok(self.0.get())
        }
        fn replacement(&self, _: &[u8], _: &str, _: Vec<u8>) -> Result<Self, DataError> {
            Ok(self.clone())
        }
    }

    /// The reload that waits.
    fn held<D: DataSource>(p: &mut Presenter<D>) -> &mut Candidate<D> {
        match p.waiting.as_mut() {
            Some(Waiting::Reload(held)) => held,
            _ => panic!("no reload waits"),
        }
    }

    #[test]
    fn requests_get_their_full_window_after_an_interaction_or_module_hold() {
        let plan = crate::test_plan(
            "component App\n  resource w = wait(1) as shape number\n  view\n    text w.toString()\n",
        );
        for interaction in [true, false] {
            let ready = Rc::new(Cell::new(true));
            let (mut p, error) = Presenter::boot_with(
                &plan,
                Loading(ready.clone()),
                (200., 200.),
                1.,
                PathBuf::new(),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.painted = true;
            assert!(p.host.runner().has_pending());
            if interaction {
                p.pointer_aux(2, true, 100., 150., 0.);
            } else {
                ready.set(false);
            }
            let module = Some(crate::delivery::Module {
                receipt: String::new(),
                bytes: Vec::new(),
            });
            assert_eq!(
                p.reload_module(&plan, Loading(ready.clone()), module, false)
                    .unwrap(),
                ReloadResult::Held(if interaction { "pointer" } else { "module" }),
            );
            let before_release = Instant::now();
            assert!(held(&mut p).requests_since.is_none());
            assert_eq!(p.reload_due_in_ms(), None);
            if interaction {
                p.pointer_aux(2, false, 100., 150., 1.);
            } else {
                ready.set(true);
            }
            p.resume_reload();
            assert_eq!(p.replacement_pending(), Some("requests"));
            assert!(held(&mut p).requests_since.unwrap() >= before_release);
            assert!(p.reload_due_in_ms().unwrap() > 0.);
            held(&mut p).requests_since = Some(Instant::now() - LET_GO_AFTER);
            p.resume_reload();
            assert_eq!(p.replacement_pending(), None);
        }
    }
    /// A candidate whose module is refused before anything holds it was
    /// never the presenter's: the error is its whole answer.
    #[test]
    fn a_candidate_refused_before_it_is_held_tells_no_outcome() {
        #[derive(Clone)]
        struct Unpaired;
        impl DataSource for Unpaired {
            fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
                Err(DataError::UnknownSource(name.into()))
            }
            fn replacement(&self, _: &[u8], _: &str, _: Vec<u8>) -> Result<Self, DataError> {
                Err(DataError::UnknownSource("module".into()))
            }
        }
        let plan = crate::test_plan("component App\n  view\n    text \"x\"\n");
        let (mut p, error) = Presenter::boot_with(
            &plan,
            Unpaired,
            (200., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.painted = true;
        let module = Some(crate::delivery::Module {
            receipt: String::new(),
            bytes: Vec::new(),
        });
        let told = Rc::new(Cell::new(0));
        let tell = told.clone();
        let result = p.reload_with_outcome(&plan, Unpaired, module, false, move |_| {
            tell.set(tell.get() + 1)
        });
        assert!(result.is_err());
        assert_eq!(told.get(), 0);
    }
    /// A URL generation and a staged update are tried again at the shell's
    /// next turn: the loader wakes its loop when the image they wait for
    /// has loaded, so that turn comes.
    #[test]
    fn a_candidate_whose_module_is_preparing_wakes_the_loop_when_it_has_loaded() {
        type Wakes = Rc<std::cell::RefCell<Vec<Box<dyn FnOnce() + Send>>>>;
        #[derive(Clone, Default)]
        struct Preparing(Wakes);
        impl DataSource for Preparing {
            fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
                Err(DataError::UnknownSource(name.into()))
            }
            fn preload(&self) -> Result<bool, DataError> {
                Ok(false)
            }
            fn when_preloaded(&self, wake: Box<dyn FnOnce() + Send>) {
                self.0.borrow_mut().push(wake);
            }
            fn replacement(&self, _: &[u8], _: &str, _: Vec<u8>) -> Result<Self, DataError> {
                Ok(self.clone())
            }
        }
        let plan = crate::test_plan("component App\n  view\n    text \"x\"\n");
        let source = Preparing::default();
        let (mut p, error) = Presenter::boot_with(
            &plan,
            source.clone(),
            (200., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.painted = true;
        let module = Some(crate::delivery::Module {
            receipt: String::new(),
            bytes: Vec::new(),
        });
        let refused = p.reload_now(&plan, source.clone(), module, false);
        assert!(matches!(refused, Err(HostError::PreparingModule)));
        assert_eq!(source.0.borrow().len(), 1);
    }
}
