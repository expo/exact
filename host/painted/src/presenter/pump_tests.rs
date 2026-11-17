use super::*;
#[path = "../../../apple/src/pump_fixture.rs"]
mod fixture;
use fixture::{plan, Replies};

#[test]
fn completion_during_an_activation_probe_keeps_its_next_turn() {
    use exact_runner::{DataError, Value};
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Loading {
        ready: Cell<bool>,
        finish_on_probe: Cell<bool>,
        wake: RefCell<Option<Box<dyn FnOnce() + Send>>>,
    }
    impl DataSource for Loading {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::Number(99.))
        }
        fn preload(&self) -> Result<bool, DataError> {
            let ready = self.ready.get();
            if self.finish_on_probe.replace(false) {
                // Completion races with a probe that already observed loading.
                self.ready.set(true);
                self.wake.borrow_mut().take().unwrap()();
            }
            Ok(ready)
        }
        fn when_preloaded(&self, wake: Box<dyn FnOnce() + Send>) {
            *self.wake.borrow_mut() = Some(wake);
        }
    }
    let plan = crate::test_plan(include_str!(
        "../../../../logic/tests/fixtures/activation.contract"
    ));
    let (mut p, error) = Presenter::boot_with(
        &plan,
        Loading::default(),
        (40., 40.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    assert!(p.module_pending());
    p.host.runner().data_ref().finish_on_probe.set(true);
    p.executor.notify();
    assert!(p.pump(0.).is_none());
    assert!(p.module_pending());
    assert!(
        p.executor.take_wake(),
        "completion during the readiness probe must survive wake acknowledgement"
    );
    assert!(p.pump(0.).is_none());
    assert!(!p.module_pending());
    let frame = p.display_frame().unwrap();
    assert_eq!(frame.pixels.pixel(39, 39).unwrap().blue(), 255);
}

#[test]
fn ready_replies_share_one_presenter_layout_with_count_and_time_bounds() {
    for (count, advance_after, first) in [(20, None, 20), (40, None, 32), (10, Some(3), 3)] {
        let mut source = Replies::new();
        source.advance_after = advance_after;
        let clock = source.clock.clone();
        let (mut p, error) = Presenter::boot_with(
            &plan(count),
            source,
            (390., 844.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let (seq, layouts) = (p.host.runner().seq(), p.host.layout_calls);
        assert_eq!(p.pump_with_clock(0., || clock.get()), None);
        assert_eq!(
            p.host.runner().data_ref().parsed,
            (0..first).collect::<Vec<_>>()
        );
        assert_eq!(p.host.runner().seq() - seq, first as u64);
        assert_eq!(p.host.layout_calls - layouts, 1);
        if first < count {
            assert!(
                p.executor.take_wake(),
                "remaining replies rearm the presenter"
            );
            assert_eq!(p.pump_with_clock(0., || clock.get()), None);
            assert_eq!(
                p.host.runner().data_ref().parsed,
                (0..count).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn a_presenter_error_preserves_prior_transactions_and_leaves_later_replies() {
    let source = Replies::new();
    let clock = source.clock.clone();
    let (mut p, error) = Presenter::boot_with(
        &fixture::fault_plan(),
        source,
        (390., 844.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let seq = p.host.runner().seq();
    assert!(p.pump_with_clock(0., || clock.get()).is_some());
    assert_eq!(p.host.runner().data_ref().parsed, [0, 1, 2]);
    assert_eq!(p.host.runner().seq() - seq, 2);
    assert_eq!(p.pump_with_clock(0., || clock.get()), None);
    assert_eq!(p.host.runner().data_ref().parsed, [0, 1, 2, 3]);
}

#[test]
fn a_presenter_refusal_keeps_its_ordered_turn() {
    let mut source = Replies::new();
    source.refuse = Some(1);
    let clock = source.clock.clone();
    let (mut p, error) = Presenter::boot_with(
        &plan(4),
        source,
        (390., 844.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    assert_eq!(p.pump_with_clock(0., || clock.get()), None);
    assert_eq!(p.host.runner().data_ref().parsed, [0, 1]);
    assert_eq!(p.pump_with_clock(0., || clock.get()), None);
    assert_eq!(p.host.runner().data_ref().parsed, [0, 1, 2, 3]);
}

/// A session booted with no view is told hidden before its first turn and
/// its data module's activation, in the order Android's host makes those
/// calls on its thread (`Headless`, `Owner.boot`). A resource that asks only
/// of a visible page then asks nothing of the module, though the module's
/// image was loaded at the boot.
#[test]
fn a_session_nothing_shows_is_hidden_before_its_data_activates() {
    use exact_runner::{DataError, Value};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A module whose image is loaded at the boot and which answers once it
    /// is activated.
    #[derive(Default)]
    struct Module {
        asked: Rc<RefCell<Vec<String>>>,
        active: bool,
    }
    impl DataSource for Module {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            self.asked.borrow_mut().push(name.to_owned());
            Ok(Value::str("fed"))
        }
        fn activate(&mut self) -> Result<(), DataError> {
            self.active = true;
            Ok(())
        }
        fn ready(&self) -> bool {
            self.active
        }
    }
    let plan = crate::test_plan(
        "shape Page\n  visibilityState: string\ncomponent App\n  resource page = exactPage() as shape Page\n  resource feed = feed() when page.visibilityState == \"visible\" as shape string else empty()\n  view\n    text `${page.visibilityState} ${feed}`\n",
    );
    let module = Module::default();
    let asked = module.asked.clone();
    let (mut p, error) = Presenter::boot_with(
        &plan,
        module,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.agent = false;

    p.page_execution(true, false);
    assert!(p.pump(0.).is_none());
    p.activate_unshown();
    assert!(p.pump(1.).is_none());
    assert_eq!(p.data_state(), 1, "{:?}", p.activation_error());
    assert!(asked.borrow().is_empty(), "{:?}", asked.borrow());

    // An activity shows the session: the resource asks now.
    p.page_execution(false, true);
    assert!(p.pump(2.).is_none());
    assert_eq!(*asked.borrow(), ["feed"]);
}

/// A session booted with no view for a wake (LLP 1133.017 D7), as Android's
/// host boots one for a job: hidden, never painted. Its data module, whose
/// image is still loading at the boot, activates with storage configured
/// before the wake is raised, so the handler's send is answered and what it
/// keeps is written to disk.
#[cfg(unix)]
#[test]
fn a_session_nothing_shows_activates_its_data_for_a_wakes_handler() {
    use exact_runner::{Answer, DataError, Store, Value};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    const CHILD: &str = "EXACT_PAINTED_UNSHOWN_TEST";
    let Some(root) = std::env::var_os(CHILD).map(PathBuf::from) else {
        let root =
            std::env::temp_dir().join(format!("exact-painted-unshown-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "presenter::pump::tests::a_session_nothing_shows_activates_its_data_for_a_wakes_handler",
            ])
            .env(CHILD, &root)
            .env("HOME", &root)
            .env("XDG_DATA_HOME", root.join("share"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env_remove("EXACT_AGENT")
            .env_remove("EXACT_STORE")
            .output()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };

    /// A module a host loads after its boot: its image loads off the
    /// thread, and it answers once it is activated.
    type Wake = Box<dyn FnOnce() + Send>;
    #[derive(Default)]
    struct Module {
        loaded: Arc<AtomicBool>,
        loading: Arc<Mutex<Option<Wake>>>,
        storage: Rc<RefCell<Option<PathBuf>>>,
        active: bool,
    }
    impl DataSource for Module {
        fn app_id(&self) -> &str {
            "test.exact.unshown"
        }
        fn grants(&self) -> &'static str {
            "secret.keep token\n"
        }
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            unreachable!()
        }
        fn preload(&self) -> Result<bool, DataError> {
            Ok(self.loaded.load(Ordering::Acquire))
        }
        fn when_preloaded(&self, wake: Wake) {
            *self.loading.lock().unwrap() = Some(wake);
        }
        fn configure_storage(
            &mut self,
            data: PathBuf,
            _: PathBuf,
            _: PathBuf,
        ) -> Result<(), DataError> {
            *self.storage.borrow_mut() = Some(data);
            Ok(())
        }
        fn activate(&mut self) -> Result<(), DataError> {
            self.active = true;
            Ok(())
        }
        fn ready(&self) -> bool {
            self.active
        }
        fn answer(
            &mut self,
            store: &mut Store,
            _: &str,
            args: &[Value],
        ) -> Result<Answer, DataError> {
            let tag = args[0].as_str().unwrap();
            store.set("token", tag)?;
            Ok(Answer::Now(Value::str(tag)))
        }
    }

    // The app, with a package outside the tree whose event a wake raises.
    let app = root.join("app");
    let fixture = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contract/cli/tests/tide"
    ));
    let package = app.join("node_modules/@fixture/tide");
    std::fs::create_dir_all(package.join("web")).unwrap();
    for file in [
        "exact.json",
        "package.json",
        "web/exact.json",
        "web/index.js",
    ] {
        std::fs::copy(fixture.join(file), package.join(file)).unwrap();
    }
    std::fs::write(
        app.join("app.contract"),
        r#"use tideOnce, tide from "@fixture/tide"

component App
  mutation saved as shape string
  action ebbed(tag: string)
    send saved = keep(tag)
  action once
    tideOnce(tag="o1")
  view
    column tide=ebbed
      button press=once
        text "Once"
"#,
    )
    .unwrap();
    let (plan, _) = contract::compile_path_all(&app.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));

    let module = Module::default();
    let (loaded, loading, storage) = (
        module.loaded.clone(),
        module.loading.clone(),
        module.storage.clone(),
    );
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        module,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    // The package's implementation, bound as in production: what it hears.
    let heard: Arc<Mutex<Vec<serde_json::Value>>> = Default::default();
    let hears = heard.clone();
    let runner = p.host.runner_mut();
    runner.native_slot().host(Some(Arc::new(
        move |body: Vec<u8>, reply: exact_runner::Reply| {
            hears
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap());
            reply.send(exact_runner::Outcome::Response(exact_runner::Response {
                status: 200,
                headers: Vec::new(),
                body: b"null".to_vec(),
            }));
        },
    )));
    runner.bind_extensions(&["tide"], &[]);
    // No activity: the page is hidden and may not execute.
    p.page_execution(true, false);

    p.activate_unshown();
    assert_eq!(p.data_state(), 0, "the module's image is still loading");
    assert!(storage.borrow().is_none());
    loaded.store(true, Ordering::Release);
    loading.lock().unwrap().take().expect("asked to wake")();
    assert!(p.pump(0.).is_none());
    assert_eq!(p.data_state(), 1, "{:?}", p.activation_error());
    let data = root.join("share/exact/test.exact.unshown/data");
    assert_eq!(storage.borrow().as_deref(), Some(data.as_path()));

    // The session is reported ready, and the package raises its wake.
    let wake =
        serde_json::json!({"extension": "tide", "event": "tide", "args": ["t1"], "id": "job-1"});
    p.host
        .runner()
        .native_slot()
        .changed(&format!("exact-wake:{wake}"));
    let woke = |heard: &Mutex<Vec<serde_json::Value>>| {
        let heard = heard.lock().unwrap();
        heard.iter().find(|body| body["woke"] == "job-1").cloned()
    };
    for turn in 0..8 {
        if woke(&heard).is_some() {
            break;
        }
        assert!(p.pump(f64::from(turn)).is_none());
    }
    let logs = exact_runner::agent::logs(p.host.runner(), 0).to_string();
    assert_eq!(
        woke(&heard).map(|body| body["outcome"].clone()),
        Some(serde_json::json!("completed")),
        "{logs}"
    );
    assert_eq!(
        format!("{:?}", p.host.runner().slot("saved")),
        format!("{:?}", Some(Value::some(Value::str("t1")))),
        "the handler's send was answered: {logs}"
    );
    // What Android's host waits for before it tells the package `completed`.
    assert!(crate::host::drain_store_writes(
        std::time::Duration::from_secs(10)
    ));
    let kept = root.join("share/exact/test.exact.unshown/secrets/token");
    assert_eq!(std::fs::read_to_string(kept).unwrap(), "t1");
}
