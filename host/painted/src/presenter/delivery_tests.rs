//! Replacement with requests in flight: delivery waits for them; a dev
//! generation lets them go after `LET_GO_AFTER` (LLP 1007 §6).
use super::*;
use exact_runner::{Answer, DataError, Store, Value};

/// The host work these tests hand out (LLP 1047.001 D12).
struct TestWork;
impl exact_runner::HostWork for TestWork {}

/// Every answer waits on a continuation until the test supplies its outcome.
#[derive(Default)]
struct Held;
impl DataSource for Held {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(
            exact_runner::Request::continuation::<TestWork>(1),
        ))
    }
    fn dispatch(&mut self, _: u64, _: &Store) -> exact_runner::Dispatch {
        exact_runner::Dispatch::Again
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: exact_runner::Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::Number(42.)))
    }
    fn replacement(&self, _: &[u8], _: &str, _: Vec<u8>) -> Result<Self, DataError> {
        Ok(Held)
    }
}

const APP: &str = "component App\n  state typed = \"\"\n  resource w = wait(1) as shape number\n  action type(v)\n    typed = v\n  view\n    column width=200 height=200\n      input value=typed change=type testId=\"field\"\n";

fn module() -> Option<crate::delivery::Module> {
    Some(crate::delivery::Module {
        receipt: String::new(),
        bytes: Vec::new(),
    })
}

#[derive(Clone)]
struct Preloading(std::rc::Rc<std::cell::Cell<bool>>);
impl DataSource for Preloading {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn preload(&self) -> Result<bool, DataError> {
        Ok(self.0.get())
    }
    fn replacement(&self, _: &[u8], _: &str, _: Vec<u8>) -> Result<Self, DataError> {
        Ok(self.clone())
    }
}

#[test]
fn url_generations_and_updates_report_a_module_hold() {
    let plan = crate::test_plan("component App\n  view\n    text \"ready\"\n");
    for update in [false, true] {
        let data = Preloading(std::rc::Rc::new(std::cell::Cell::new(true)));
        let (mut p, error) = Presenter::boot_with(
            &plan,
            data.clone(),
            (200., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host.activate_data().unwrap();
        p.painted = true;
        p.compat = r#"{"inputs":{"executors":["hermes"]}}"#.into();
        data.0.set(false);
        if update {
            p.set_updates(Some(Box::new(StagedModule {
                plan: plan.clone().into(),
                log: Arc::new(std::sync::Mutex::new(UpdateLog {
                    has_module: true,
                    seq: 1,
                    ..UpdateLog::default()
                })),
            })));
            p.owe_update(false);
            p.run_commands(|| data.clone());
        } else {
            p.set_development(Some("http://127.0.0.1:9/".into()), None);
            p.keep_generation(crate::fetch::Generation {
                identity: "preloading".into(),
                plan: plan.clone(),
                module: module(),
            });
            p.poll_development(|| data.clone());
        }
        assert_eq!(p.replacement_pending(), Some("module"), "update: {update}");
        data.0.set(true);
        p.run_commands(|| data.clone());
        if !update {
            p.poll_development(|| data.clone());
        }
        assert_eq!(p.replacement_pending(), None);
    }
}

#[derive(Default)]
struct UpdateLog {
    line: Option<String>,
    commits: usize,
    attempts: usize,
    asset_reads: usize,
    has_module: bool,
    receipt: Option<Vec<u8>>,
    module_reads: usize,
    names: Option<Vec<String>>,
    seq: u64,
}

struct StagedModule {
    plan: Arc<[u8]>,
    log: Arc<std::sync::Mutex<UpdateLog>>,
}

impl crate::delivery::Store for StagedModule {
    fn prepare_selected(&mut self) -> Option<crate::delivery::Selection> {
        None
    }
    fn boot_started(&mut self) {}
    fn entry_refused(&mut self, _: &str, _: &str) {}
    fn selection_corrupt(&mut self, _: &str) {}
    fn boot_succeeded(&mut self) {}
    fn take_note(&mut self) -> Option<String> {
        None
    }
    #[cfg(unix)]
    fn fd(&self) -> std::os::unix::io::RawFd {
        -1
    }
    fn check(&self) -> bool {
        true
    }
    fn take_line(&mut self) -> Option<String> {
        self.log.lock().unwrap().line.take()
    }
    fn prepare_activation(&self) -> Result<Option<crate::delivery::Selection>, String> {
        self.log.lock().unwrap().attempts += 1;
        let (seq, has_module) = {
            let log = self.log.lock().unwrap();
            (log.seq, log.has_module)
        };
        if self.log.lock().unwrap().commits > 0 {
            return Ok(None);
        }
        let log = self.log.clone();
        Ok(Some(crate::delivery::Selection {
            entry: Some("candidate".into()),
            seq,
            plan: self.plan.clone(),
            assets: Arc::new(move |name| {
                let mut log = log.lock().unwrap();
                log.asset_reads += 1;
                if log
                    .names
                    .as_ref()
                    .is_some_and(|names| !names.iter().any(|n| n == name))
                {
                    return Ok(None);
                }
                if name.ends_with("/app.module.json") {
                    Ok(Some(Arc::from(log.receipt.as_deref().unwrap_or(b"{}"))))
                } else {
                    log.module_reads += 1;
                    Ok(Some(Arc::from(b"module".as_slice())))
                }
            }),
            names: if let Some(names) = self.log.lock().unwrap().names.clone() {
                names
            } else if has_module {
                vec![
                    "typescript/app.module.json".into(),
                    "typescript/app.hbc".into(),
                ]
            } else {
                vec![]
            },
            dir: None,
            verify_assets: Box::new(|| Ok(())),
        }))
    }
    fn commit_activation(&mut self, entry: Option<String>, seq: u64) -> Result<(), String> {
        assert_eq!(entry.as_deref(), Some("candidate"));
        assert_eq!(seq, self.log.lock().unwrap().seq);
        self.log.lock().unwrap().commits += 1;
        Ok(())
    }
    fn staged_stream_into(&self, _: &mut exact_runner::Delivery) {}
    fn status_into(&self, delivery: &mut exact_runner::Delivery) {
        let committed = self.log.lock().unwrap().commits > 0;
        delivery.seq = if committed {
            self.log.lock().unwrap().seq
        } else {
            0
        };
        delivery.staged = !committed;
    }
}

#[test]
fn an_immediate_update_retries_after_a_request_finishes_or_is_cancelled() {
    for kind in ["resource", "mutation", "cancelled"] {
        let source = if kind == "resource" {
            APP.to_string()
        } else {
            APP.replace(
                "  resource w = wait(1) as shape number",
                "  mutation w as shape number\n  action start\n    send w = wait(1)\n  action cancel\n    w = none",
            ) + "      button press=start testId=\"start\"\n        text \"Start\"\n      button press=cancel testId=\"cancel\"\n        text \"Cancel\"\n"
        } + "      text \"old\"\n";
        let plan = crate::test_plan(&source);
        let (mut p, error) = Presenter::boot_with(
            &plan,
            Held,
            (200., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host.activate_data().unwrap();
        p.painted = true;
        p.compat =
            r#"{"inputs":{"executors":["hermes"]},"delivery":{"activate":"immediate"}}"#.into();
        let id = |p: &Presenter<Held>, name| {
            let k = p.host.kernel();
            k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
        };
        p.host.dispatch_at(
            id(&p, "field"),
            exact_runner::Event::Change("ada".into()),
            0.,
        );
        if kind != "resource" {
            p.tap(id(&p, "start")).unwrap();
        }
        let pending = p.host.runner().pending();
        assert_eq!(pending.len(), 1, "{kind}");
        assert!(p.host.runner().has_pending());
        let log = Arc::new(std::sync::Mutex::new(UpdateLog {
            line: Some("staged seq 1".into()),
            seq: 1,
            has_module: true,
            ..UpdateLog::default()
        }));
        p.set_updates(Some(Box::new(StagedModule {
            plan: crate::test_plan(&source.replace("\"old\"", "\"new\"")).into(),
            log: log.clone(),
        })));
        p.update_foreground(true);
        assert!(p.poll_update());
        for _ in 0..3 {
            p.run_commands(|| Held);
            assert_eq!(log.lock().unwrap().commits, 0, "wait for {kind}");
            assert_eq!(p.host.runner().pending(), pending);
            assert_eq!(p.replacement_pending(), Some("requests"));
        }
        {
            let log = log.lock().unwrap();
            assert_eq!(log.module_reads, 0);
            assert_eq!(
                (
                    log.attempts,
                    log.asset_reads,
                    p.delivery_pending(),
                    p.module_pending()
                ),
                (1, 1, false, false),
                "blocked turns neither prepare the candidate nor request delivery wakes: {kind}"
            );
        }
        // Another check says current while the same generation is still staged.
        log.lock().unwrap().line = Some("current".into());
        assert!(p.poll_update());
        p.run_commands(|| Held);
        assert_eq!(
            log.lock().unwrap().attempts,
            1,
            "current keeps waiting: {kind}"
        );
        assert!(!p.delivery_pending());
        if kind == "cancelled" {
            p.tap(id(&p, "cancel")).unwrap();
        } else {
            assert!(p
                .host
                .fulfill_all(
                    vec![(pending[0].1, exact_runner::Outcome::Storage(vec![]), None)],
                    p.host.now(),
                )
                .is_none());
        }
        assert!(!p.host.runner().has_pending());
        assert!(p.delivery_pending(), "ready after {kind}");
        p.run_commands(|| Held);
        assert_eq!(log.lock().unwrap().commits, 1, "activate after {kind}");
        assert!(!p.update_owed());
        assert_eq!(log.lock().unwrap().attempts, 2, "one retry after {kind}");
        assert_eq!(p.host.runner().slot("typed"), Some(&Value::str("ada")));
        assert_eq!(p.host.runner().delivery().seq, 1);
        assert!(!p.host.runner().delivery().staged);
        let tree = p.host.agent(r#"{"op":"tree"}"#);
        assert!(tree.contains("new"), "{kind}: {tree}");
        let attempts = log.lock().unwrap().attempts;
        p.run_commands(|| Held);
        assert_eq!(log.lock().unwrap().attempts, attempts, "activated once");
    }
}

#[test]
fn a_plan_only_update_activates_with_a_request_pending() {
    let plan = crate::test_plan(APP);
    let (mut p, error) = Presenter::boot_with(
        &plan,
        Held,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.painted = true;
    p.compat = r#"{"delivery":{"activate":"immediate"}}"#.into();
    let log = Arc::new(std::sync::Mutex::new(UpdateLog {
        line: Some("staged seq 1".into()),
        seq: 1,
        ..UpdateLog::default()
    }));
    p.set_updates(Some(Box::new(StagedModule {
        plan: crate::test_plan(&(APP.to_string() + "      text \"new\"\n")).into(),
        log: log.clone(),
    })));
    p.update_foreground(true);
    assert!(p.poll_update());
    assert!(p.host.runner().has_pending());
    p.run_commands(|| Held);
    assert_eq!(log.lock().unwrap().commits, 1);
    assert_eq!(log.lock().unwrap().attempts, 1);
    assert!(!p.update_owed());
    assert!(p.host.agent(r#"{"op":"tree"}"#).contains("new"));
}

/// One candidate waits, whichever producer brought it: a local reload, the
/// development URL's generation or the staged update (LLP 1029.000.002 D9).
#[test]
fn the_newest_candidate_supersedes_one_that_waits_whichever_brought_it() {
    const PRODUCERS: [&str; 3] = ["reload", "generation", "update"];
    let plan = |name: &str| crate::test_plan(&format!("{APP}      text \"{name}\"\n"));
    let pairs = PRODUCERS
        .iter()
        .flat_map(|first| PRODUCERS.iter().map(move |last| (*first, *last)))
        .filter(|(first, last)| first != last);
    for (first, last) in pairs {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(APP),
            Held,
            (200., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.painted = true;
        p.compat = r#"{"delivery":{"activate":"immediate"}}"#.into();
        let log = Arc::new(std::sync::Mutex::new(UpdateLog {
            line: Some("staged seq 1".into()),
            seq: 1,
            ..UpdateLog::default()
        }));
        p.set_updates(Some(Box::new(StagedModule {
            plan: plan("update").into(),
            log: log.clone(),
        })));
        p.set_development(Some("http://127.0.0.1:9/".into()), None);
        let ended = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let bring = |p: &mut Presenter<Held>, producer: &str| match producer {
            "reload" => {
                let ended = ended.clone();
                let told = move |outcome| ended.borrow_mut().push(outcome);
                let held = p.reload_with_outcome(&plan("reload"), Held, None, false, told);
                assert_eq!(held.unwrap(), ReloadResult::Held("pointer"));
            }
            "generation" => {
                p.keep_generation(crate::fetch::Generation {
                    identity: "generation".into(),
                    plan: plan("generation"),
                    module: None,
                });
                p.poll_development(|| Held);
            }
            _ => {
                p.update_foreground(true);
                assert!(p.poll_update());
                p.run_commands(|| Held);
            }
        };
        p.pointer_down(100., 150., 0.).unwrap();
        bring(&mut p, first);
        bring(&mut p, last);
        assert_eq!(p.replacement_pending(), Some("pointer"));
        p.pointer_up(100., 150., 1.).unwrap();
        for _ in 0..2 {
            p.run_commands(|| Held);
            p.poll_development(|| Held);
        }
        let tree = p.host.agent(r#"{"op":"tree"}"#);
        // The label's text: the reply's tags name a `generation` too.
        let only_last = PRODUCERS
            .iter()
            .all(|name| tree.contains(&format!("\"text\":\"{name}\"")) == (*name == last));
        assert!(only_last, "{first} then {last}: {tree}");
        assert_eq!(
            log.lock().unwrap().commits,
            usize::from(last == "update"),
            "{first} then {last}"
        );
        let told: &[ReplacementOutcome] = match (first, last) {
            ("reload", _) => &[ReplacementOutcome::Superseded],
            (_, "reload") => &[ReplacementOutcome::Applied],
            _ => &[],
        };
        assert_eq!(*ended.borrow(), told, "{first} then {last}");
        assert_eq!(p.replacement_pending(), None);
    }
}

/// `deliveryActivate` with nothing staged brings no candidate.
#[test]
fn an_activation_with_nothing_staged_leaves_the_candidate_that_waits() {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(APP),
        Held,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.painted = true;
    p.set_updates(Some(Box::new(StagedModule {
        plan: crate::test_plan(APP).into(),
        log: Arc::new(std::sync::Mutex::new(UpdateLog {
            commits: 1,
            ..UpdateLog::default()
        })),
    })));
    p.pointer_down(100., 150., 0.).unwrap();
    let reloaded = crate::test_plan(&(APP.to_string() + "      text \"reload\"\n"));
    assert_eq!(
        p.reload(&reloaded, Held).unwrap(),
        ReloadResult::Held("pointer")
    );
    p.owe_update(true);
    p.pointer_up(100., 150., 1.).unwrap();
    p.run_commands(|| Held);
    assert!(p.host.agent(r#"{"op":"tree"}"#).contains("reload"));
}

fn staged_update() -> (Presenter<Held>, Arc<std::sync::Mutex<UpdateLog>>) {
    let source = APP.replace(
        "  action type(v)",
        "  action apply\n    deliveryActivate()\n  action type(v)",
    ) + "      button press=apply testId=\"apply\"\n        text \"Apply\"\n";
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(&source),
        Held,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.host.activate_data().unwrap();
    p.painted = true;
    p.compat = r#"{"inputs":{"executors":["hermes"]},"delivery":{"activate":"immediate"}}"#.into();
    let log = Arc::new(std::sync::Mutex::new(UpdateLog {
        line: Some("staged seq 1".into()),
        seq: 1,
        has_module: true,
        ..UpdateLog::default()
    }));
    p.set_updates(Some(Box::new(StagedModule {
        plan: crate::test_plan(&(source + "      text \"new\"\n")).into(),
        log: log.clone(),
    })));
    (p, log)
}

fn waiting_update() -> (Presenter<Held>, Arc<std::sync::Mutex<UpdateLog>>) {
    let (mut p, log) = staged_update();
    p.update_foreground(true);
    assert!(p.poll_update());
    p.run_commands(|| Held);
    assert!(p.update_hold() == Some("requests"));
    assert_eq!(log.lock().unwrap().attempts, 1);
    (p, log)
}

#[test]
fn an_automatic_retry_waits_for_the_foreground_after_background_completion() {
    let (mut p, log) = waiting_update();
    p.update_foreground(false);
    let pending = p.host.runner().pending();
    assert_eq!(pending.len(), 1);
    assert!(p
        .host
        .fulfill_all(
            vec![(pending[0].1, exact_runner::Outcome::Storage(vec![]), None)],
            p.host.now(),
        )
        .is_none());
    assert!(!p.host.runner().has_pending());
    assert!(p.pump(p.host.now()).is_none());
    p.run_commands(|| Held);
    assert_eq!(
        log.lock().unwrap().commits,
        0,
        "no activation while stopped"
    );
    assert_eq!(log.lock().unwrap().attempts, 1);
    assert!(p.update_owed(), "retain the automatic activation");
    assert!(!p.delivery_pending(), "no delivery polling while stopped");
    assert!(!p.module_pending());
    p.executor.begin_pump();
    assert!(!p.executor.take_wake(), "the stopped host is idle");
    p.update_foreground(true);
    assert!(
        p.executor.take_wake(),
        "resume wakes an idle host to activate"
    );
    assert!(p.delivery_pending(), "resume delivery polling on start");
    p.executor.begin_pump();
    p.update_foreground(true);
    assert!(
        !p.executor.take_wake(),
        "an unchanged foreground needs no wake"
    );
    p.run_commands(|| Held);
    assert_eq!(log.lock().unwrap().commits, 1);
    assert_eq!(p.host.runner().delivery().seq, 1);
    assert!(p.host.agent(r#"{"op":"tree"}"#).contains("new"));
}

#[test]
fn delivery_activate_without_a_foreground_signal_applies_and_retries() {
    for finish_before_command in [true, false] {
        let (mut p, log) = staged_update();
        assert!(p.poll_update());
        assert!(!p.update_owed(), "headless runs do not auto-activate");
        let pending = p.host.runner().pending();
        let finish = |p: &mut Presenter<Held>| {
            assert!(p
                .host
                .fulfill_all(
                    vec![(pending[0].1, exact_runner::Outcome::Storage(vec![]), None)],
                    p.host.now(),
                )
                .is_none());
        };
        if finish_before_command {
            finish(&mut p);
        }
        let k = p.host.kernel();
        let id = k.node_by_key(k.find_by_test_id("apply")[0]).unwrap().id;
        p.tap(id).unwrap();
        p.run_commands(|| Held);
        assert_eq!(log.lock().unwrap().attempts, 1, "the explicit command runs");
        if !finish_before_command {
            assert!(p.update_hold() == Some("requests"));
            assert!(!p.delivery_pending());
            finish(&mut p);
            p.run_commands(|| Held);
        }
        assert_eq!(log.lock().unwrap().commits, 1);
        assert!(!p.update_owed());
        assert!(p.host.agent(r#"{"op":"tree"}"#).contains("new"));
    }
}

#[test]
fn an_uninstallable_module_roster_is_refused_without_waiting_for_requests() {
    for (names, compat, refusal) in [
        (
            vec!["typescript/app.module.json", "typescript/app.hbc"],
            r#"{"inputs":{}}"#,
            "this binary runs no TypeScript module",
        ),
        (
            vec!["rust/app.module.json", "rust/app.module.wasm"],
            r#"{"inputs":{}}"#,
            "Rust replacement is disabled in this binary",
        ),
        (
            vec!["rust/app.module.json", "typescript/app.hbc"],
            r#"{"inputs":{}}"#,
            "a generation carries one module pair, not two",
        ),
        (
            vec!["typescript/app.module.json"],
            r#"{"inputs":{"executors":["hermes"]}}"#,
            "invalid signed typescript module pair",
        ),
        (
            vec![
                "typescript/app.module.json",
                "typescript/app.hbc",
                "typescript/app.js",
            ],
            r#"{"inputs":{"executors":["hermes"]}}"#,
            "invalid signed typescript module pair",
        ),
    ] {
        let (mut p, log) = waiting_update();
        p.compat = compat.into();
        log.lock().unwrap().names = Some(names.iter().map(|name| name.to_string()).collect());
        assert!(p.host.runner().has_pending());
        assert!(
            matches!(p.activate_update(Held), Err(HostError::Asset(e)) if e == refusal),
            "{names:?}"
        );
        let k = p.host.kernel();
        let id = k.node_by_key(k.find_by_test_id("apply")[0]).unwrap().id;
        p.tap(id).unwrap();
        p.run_commands(|| Held);
        assert!(
            !p.update_owed(),
            "terminal refusal ends the attempt: {names:?}"
        );
        assert!(p.update_hold().is_none());
        assert_eq!(log.lock().unwrap().commits, 0);
        assert_eq!(log.lock().unwrap().asset_reads, 1);
        assert_eq!(log.lock().unwrap().module_reads, 0);
    }
}

#[test]
fn two_name_rosters_require_both_halves_before_waiting_for_requests() {
    let mut failures = Vec::new();
    for (names, refusal) in [
        (
            ["typescript/app.hbc", "typescript/extra.dat"],
            "missing module pairing receipt",
        ),
        (
            ["typescript/app.module.json", "typescript/app.js"],
            "invalid signed typescript module pair",
        ),
        (
            ["rust/app.module.wasm", "rust/extra.dat"],
            "missing module pairing receipt",
        ),
        (
            ["rust/app.module.json", "rust/extra.dat"],
            "invalid signed rust module pair",
        ),
    ] {
        let (mut p, log) = waiting_update();
        p.compat = r#"{"inputs":{"executors":["hermes"],"rustMode":"wasm"}}"#.into();
        log.lock().unwrap().names = Some(names.map(str::to_string).to_vec());
        assert!(p.host.runner().has_pending());
        let result = p.activate_update(Held);
        assert_eq!(log.lock().unwrap().module_reads, 0);
        if !matches!(result, Err(HostError::Asset(ref e)) if e == refusal) {
            failures.push(format!("{names:?}: {result:?}"));
        }
        p.owe_update(true);
        p.run_commands(|| Held);
        if p.update_owed() {
            failures.push(format!("{names:?}: terminal refusal was queued"));
        }
        assert_eq!(log.lock().unwrap().module_reads, 0);
        assert_eq!(log.lock().unwrap().commits, 0);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn invalid_receipts_are_refused_before_waiting_for_requests() {
    let mut failures = Vec::new();
    for (dir, module, receipt) in [
        ("typescript", "app.hbc", b"{".to_vec()),
        ("typescript", "app.hbc", vec![0xff]),
        ("typescript", "app.hbc", vec![b' '; (1 << 20) + 1]),
        ("rust", "app.module.wasm", b"{".to_vec()),
        ("rust", "app.module.wasm", vec![0xff]),
        ("rust", "app.module.wasm", vec![b' '; (1 << 20) + 1]),
        (
            "rust",
            "app.module.wasm",
            br#"{"module":{"file":"../x.so"}}"#.to_vec(),
        ),
        (
            "rust",
            "app.module.wasm",
            br#"{"module":{"file":"app.module.so"}}"#.to_vec(),
        ),
    ] {
        let (mut p, log) = waiting_update();
        p.compat = r#"{"inputs":{"executors":["hermes"],"rustMode":"wasm"}}"#.into();
        {
            let mut log = log.lock().unwrap();
            log.names = Some(vec![
                format!("{dir}/app.module.json"),
                format!("{dir}/{module}"),
            ]);
            log.receipt = Some(receipt);
        }
        assert!(p.host.runner().has_pending());
        let result = p.activate_update(Held);
        assert_eq!(log.lock().unwrap().module_reads, 0);
        if !matches!(result, Err(HostError::Asset(_))) {
            failures.push(format!("{result:?}"));
        }
        p.owe_update(true);
        p.run_commands(|| Held);
        assert_eq!(log.lock().unwrap().module_reads, 0);
        if p.update_owed() {
            failures.push("terminal receipt refusal was queued".into());
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn an_invalid_plan_is_refused_before_waiting_for_requests() {
    let (mut p, log) = waiting_update();
    p.set_updates(Some(Box::new(StagedModule {
        plan: Arc::from(b"invalid".as_slice()),
        log: log.clone(),
    })));
    assert!(p.host.runner().has_pending());
    assert!(matches!(p.activate_update(Held), Err(HostError::Plan(_))));
    assert_eq!(log.lock().unwrap().module_reads, 0);
    p.owe_update(true);
    p.run_commands(|| Held);
    assert!(!p.update_owed());
    assert_eq!(log.lock().unwrap().module_reads, 0);
}

#[test]
fn unlinked_needs_are_refused_before_waiting_for_requests() {
    let mut failures = Vec::new();
    for permissions in [false, true] {
        let (mut p, log) = waiting_update();
        p.compat = r#"{"inputs":{"executors":["hermes"],"androidLinked":{"uses":[],"extensions":[],"permissions":[]}}}"#.into();
        if permissions {
            log.lock().unwrap().receipt =
                Some(br#"{"androidPermissions":["android.permission.WRITE_CALENDAR"]}"#.to_vec());
        } else {
            p.set_updates(Some(Box::new(StagedModule {
                plan: crate::test_plan("component App\n  state day = \"2026-09-14\"\n  action setDay(v: string)\n    day = v\n  view\n    input type=\"date\" value=day change=setDay\n").into(),
                log: log.clone(),
            })));
        }
        assert!(p.host.runner().has_pending());
        let result = p.activate_update(Held);
        assert_eq!(log.lock().unwrap().module_reads, 0);
        if !matches!(result, Err(HostError::Unlinked(_))) {
            failures.push(format!("permissions={permissions}: {result:?}"));
        }
        p.owe_update(true);
        p.run_commands(|| Held);
        if p.update_owed() {
            failures.push(format!(
                "permissions={permissions}: terminal refusal was queued"
            ));
        }
        assert_eq!(log.lock().unwrap().module_reads, 0);
        assert_eq!(log.lock().unwrap().commits, 0);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn rearm_plan_only(foreground: bool) {
    let (mut p, log) = waiting_update();
    if foreground {
        p.update_foreground(false);
    }
    {
        let mut log = log.lock().unwrap();
        log.has_module = false;
        log.seq = 2;
        log.line = Some("staged seq 2".into());
    }
    if foreground {
        assert!(p.poll_update());
        p.run_commands(|| Held);
        assert_eq!(log.lock().unwrap().attempts, 1, "no retry while stopped");
        assert!(!p.delivery_pending());
        p.update_foreground(true);
    } else {
        let k = p.host.kernel();
        let id = k.node_by_key(k.find_by_test_id("apply")[0]).unwrap().id;
        p.tap(id).unwrap();
    }
    assert!(p.host.runner().has_pending());
    p.run_commands(|| Held);
    assert_eq!(log.lock().unwrap().commits, 1);
    assert_eq!(log.lock().unwrap().attempts, 2);
    assert_eq!(p.host.runner().delivery().seq, 2);
    assert!(p.host.agent(r#"{"op":"tree"}"#).contains("new"));
}

#[test]
fn foreground_rechecks_a_plan_only_candidate_after_a_module_wait() {
    rearm_plan_only(true);
}

#[test]
fn delivery_activate_rechecks_a_plan_only_candidate_after_a_module_wait() {
    rearm_plan_only(false);
}

#[test]
fn an_update_refuses_and_a_dev_generation_lets_requests_go_after_its_window() {
    let plan = crate::test_plan(APP);
    let (mut p, error) = Presenter::boot_with(
        &plan,
        Held,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.painted = true;
    let k = p.host.runner().kernel();
    let field = k.node_by_key(k.find_by_test_id("field")[0]).unwrap().id;
    let now = p.host.now();
    p.host
        .dispatch_at(field, exact_runner::Event::Change("ada".into()), now);
    assert!(p.host.runner().has_pending());
    assert!(matches!(
        p.reload_module(&plan, Held, module(), false),
        Ok(ReloadResult::Held("requests"))
    ));
    p.waiting = None;

    // The dev loop's generation: refused and held while the window runs,
    // then swapped, carrying what was typed.
    p.set_development(Some("http://127.0.0.1:9/".into()), None);
    let generation = || crate::fetch::Generation {
        identity: "gen".into(),
        plan: plan.clone(),
        module: module(),
    };
    p.keep_generation(generation());
    p.poll_development(|| Held);
    assert!(p.generation_waits(), "held while the window runs");
    assert_eq!(p.replacement_pending(), Some("requests"));
    let since = p.dev.as_ref().unwrap().held_since.expect("the wait began");
    p.dev.as_mut().unwrap().held_since = Some(since - crate::host::LET_GO_AFTER);
    p.poll_development(|| Held);
    assert!(!p.generation_waits());
    assert_eq!(
        p.dev.as_ref().unwrap().identity.as_deref(),
        Some("gen"),
        "installed"
    );
    assert!(p.dev.as_ref().unwrap().held_since.is_none());
    assert_eq!(p.host.runner().slot("typed"), Some(&Value::str("ada")));
}

/// A held generation is the development loop's that brought it: a loop set
/// in its place, or none, drops it (LLP 1029.000.002 D9).
#[test]
fn a_held_generation_goes_with_the_development_loop_that_brought_it() {
    let plan = crate::test_plan(APP);
    let (mut p, error) = Presenter::boot_with(
        &plan,
        Held,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.painted = true;
    for next in [Some("http://127.0.0.1:10/".to_string()), None] {
        p.set_development(Some("http://127.0.0.1:9/".into()), None);
        p.set_touching(true);
        p.keep_generation(crate::fetch::Generation {
            identity: "gen".into(),
            plan: plan.clone(),
            module: module(),
        });
        p.poll_development(|| Held);
        assert_eq!(p.replacement_pending(), Some("pointer"), "{next:?}");
        p.set_development(next.clone(), None);
        p.set_touching(false);
        p.poll_development(|| Held);
        assert!(!p.generation_waits(), "{next:?}");
        assert_eq!(p.replacement_pending(), None, "{next:?}");
        if let Some(dev) = &p.dev {
            assert_eq!(dev.identity, None, "the new loop installed nothing");
        }
    }
}

/// LLP 1122 D4 on the Android reader: a host that turns only when woken is
/// told when the development poll wants its next turn, and is woken through
/// the executor's fd when the poll's answer lands.
#[test]
fn a_woken_host_is_told_the_development_polls_turn_and_woken_by_its_answer() {
    let plan = crate::test_plan(APP);
    let (mut p, _) = Presenter::boot_with(
        &plan,
        Held,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(p.turn_due_ms(p.host.now()), None, "no timer, no dev loop");
    // A port nothing listens on: the poll's answer is a refused connection.
    p.set_development(Some("http://127.0.0.1:9/".into()), None);
    assert_eq!(
        p.turn_due_ms(p.host.now()),
        None,
        "it polls after the first pixel"
    );
    assert!(
        p.dev.as_ref().unwrap().waker.lock().unwrap().is_some(),
        "the executor's waker"
    );
    // Each answer wakes the host (a counting waker stands in for the fd's).
    let woken = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = woken.clone();
    *p.dev.as_mut().unwrap().waker.lock().unwrap() = Some(std::sync::Arc::new(move || {
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    p.painted = true;
    let due = p.turn_due_ms(p.host.now()).expect("the poll is due");
    assert!(due <= 500.0, "{due}");
    // A tick alone (a quick frame's) starts the due poll: it is due no
    // longer, so a host asking again does not spin on a zero deadline.
    p.tick_development();
    let after = p.turn_due_ms(p.host.now());
    assert!(after.is_none_or(|d| d > 0.0), "{after:?}");
    let start = std::time::Instant::now();
    while woken.load(std::sync::atomic::Ordering::SeqCst) == 0 && start.elapsed().as_secs() < 10 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        woken.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "the poll's answer wakes the host"
    );
}

/// A loop that answers every request with its producers' status, and how
/// many it has answered.
fn starting_loop() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let served = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = served.clone();
    std::thread::spawn(move || {
        let body = r#"{"exact":1,"status":"preparing","message":"building the module producer"}"#;
        while let Ok((mut stream, _)) = listener.accept() {
            let mut head = [0u8; 4096];
            let _ = stream.read(&mut head);
            let _ = stream.write_all(format!("HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/vnd.exact.dev-status+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    });
    (url, served)
}

/// The presenter's poll against `url` until it has `answers` answers in.
fn poll_until(p: &mut Presenter<Held>, served: &std::sync::atomic::AtomicUsize, answers: usize) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while served.load(std::sync::atomic::Ordering::SeqCst) < answers
        && std::time::Instant::now() < end
    {
        p.tick_development();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    p.tick_development();
    assert!(served.load(std::sync::atomic::Ordering::SeqCst) >= answers);
}

/// A dev loop whose producers are starting (dtool2's pass 3 #8; theme2's
/// condition c): the presenter's poll, which the Linux display and the
/// Android reader's canvas host both turn, says its status once however
/// many answers come; and not at all when the boot's fetch said it already.
#[test]
fn a_starting_loops_status_is_said_once_by_the_poll() {
    let plan = crate::test_plan(APP);
    let (url, served) = starting_loop();
    let (mut p, _) = Presenter::boot_with(
        &plan,
        Held,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    p.painted = true;
    p.set_development(Some(url.clone()), None);
    poll_until(&mut p, &served, 4);
    assert_eq!(p.dev.as_ref().unwrap().lines_said, 1);

    let (url, served) = starting_loop();
    let (mut p, _) = Presenter::boot_with(
        &plan,
        Held,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    p.painted = true;
    p.set_development(Some(url), None);
    p.said_at_boot(Some("dev loop: building the module producer".into()));
    poll_until(&mut p, &served, 3);
    assert_eq!(
        p.dev.as_ref().unwrap().lines_said,
        0,
        "said at boot, not again"
    );
}

/// A login item's hidden launch (LLP 1133.025 D6): its window takes no
/// present, so the picture it owes stays owed, and a frame painted to pixels
/// nothing presents is its first pixel all the same.
#[test]
fn a_window_that_is_not_shown_reaches_first_pixel_without_a_present() {
    let plan = crate::test_plan("component App\n  view\n    text \"Hidden\"\n");
    let (mut p, error) = Presenter::boot_with(
        &plan,
        (),
        (200., 200.),
        1.,
        PathBuf::new(),
        crate::presenter::PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.repaint();
    p.first_pixel();
    assert!(!p.first_pixel_told(), "nothing was painted");
    p.first_pixel_unshown();
    assert!(p.first_pixel_told());
}

#[test]
fn module_validation_keeps_the_live_sequence_and_root_drop_report() {
    let plan = |second: bool| {
        crate::test_plan(&format!(
        "component App\n  state kept = 1\n  state retyped = {}\n{}  view\n    text `${{kept}}`\n",
        if second { "\"new\"" } else { "1" },
        if second { "" } else { "  state removed = 1\n" },
    ))
    };
    let (mut p, error) = Presenter::boot_with(
        &plan(false),
        Held,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.host.activate_data().unwrap();
    p.painted = true;
    let epoch = p.host.runner().kernel().epoch();
    assert_eq!(
        p.reload_module(&plan(true), Held, module(), false).unwrap(),
        ReloadResult::Applied(None)
    );
    let runner = p.host.runner();
    let state = exact_runner::agent::state(runner);
    assert!(
        state.contains("{\"what\":\"state\",\"name\":\"retyped\",\"why\":\"type changed\"}"),
        "{state}"
    );
    assert!(
        state.contains("{\"what\":\"state\",\"name\":\"removed\",\"why\":\"no place\"}"),
        "{state}"
    );
    assert_eq!(runner.generation(), 2);
    assert!(runner.kernel().epoch() > epoch);
}

/// A replacement waits while a contact is down or a field composes, kept
/// as bytes and prepared at its commit, and its producer hears how it ended
/// (LLP 1029.000.002 D7, D9).
#[test]
fn a_replacement_waits_for_a_contact_and_a_composition_and_tells_its_end() {
    use std::{cell::RefCell, rc::Rc};
    let plan = |label: &str| {
        crate::test_plan(&format!(
            "component App\n  state typed = \"\"\n  action type(v)\n    typed = v\n  view\n    column width=200 height=200\n      input value=typed change=type testId=\"field\" width=180 height=30\n      text \"{label}\" testId=\"label\"\n"
        ))
    };
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan("one"),
        (),
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let ended = Rc::new(RefCell::new(Vec::new()));
    let tell = |ended: &Rc<RefCell<Vec<ReplacementOutcome>>>| {
        let ended = ended.clone();
        move |outcome| ended.borrow_mut().push(outcome)
    };
    let label = |p: &Presenter<()>| {
        let k = p.host.kernel();
        let node = k.node_by_key(k.find_by_test_id("label")[0]).unwrap();
        node.props.str(PropId::Text).unwrap().to_string()
    };
    let field = |p: &Presenter<()>| {
        let k = p.host.kernel();
        k.node_by_key(k.find_by_test_id("field")[0]).unwrap().id
    };

    // A press holds it, and a newer candidate supersedes it.
    p.pointer_down(100., 150., 0.).unwrap();
    assert!(p.contact_position().is_some());
    let held = p.reload_with_outcome(&plan("two"), (), None, false, tell(&ended));
    assert_eq!(held.unwrap(), ReloadResult::Held("pointer"));
    let held = p.reload_with_outcome(&plan("three"), (), None, false, tell(&ended));
    assert_eq!(held.unwrap(), ReloadResult::Held("pointer"));
    assert_eq!(*ended.borrow(), [ReplacementOutcome::Superseded]);
    assert_eq!(p.replacement_pending(), Some("pointer"));
    // What changes during the hold is what it carries.
    let now = p.host.now();
    p.host
        .dispatch_at(field(&p), exact_runner::Event::Change("ada".into()), now);
    p.run_commands(|| ());
    assert_eq!(label(&p), "one");
    p.pointer_up(100., 150., 1.).unwrap();
    p.run_commands(|| ());
    assert_eq!(p.replacement_pending(), None);
    assert_eq!(label(&p), "three");
    assert_eq!(p.host.runner().slot("typed"), Some(&Value::str("ada")));
    assert_eq!(
        *ended.borrow(),
        [ReplacementOutcome::Superseded, ReplacementOutcome::Applied]
    );

    // A composition holds it until the composition ends.
    ended.borrow_mut().clear();
    p.set_focus(Some(field(&p)), 2.);
    let target = p.ime_target().unwrap();
    assert!(p.ime_begin(target));
    p.ime_preedit("に", None);
    let held = p.reload_with_outcome(&plan("four"), (), None, false, tell(&ended));
    assert_eq!(held.unwrap(), ReloadResult::Held("composition"));
    p.run_commands(|| ());
    assert_eq!(label(&p), "three");
    p.ime_commit("日");
    p.run_commands(|| ());
    assert_eq!(label(&p), "four");
    assert_eq!(*ended.borrow(), [ReplacementOutcome::Applied]);

    // Auxiliary buttons hold a replacement even without a primary contact.
    for bit in [2, 4] {
        ended.borrow_mut().clear();
        p.pointer_aux(bit, true, 100., 150., 3.);
        let held = p.reload_with_outcome(&plan("four"), (), None, false, tell(&ended));
        assert_eq!(held.unwrap(), ReloadResult::Held("pointer"));
        p.run_commands(|| ());
        assert!(ended.borrow().is_empty());
        p.pointer_aux(bit, false, 100., 150., 4.);
        p.run_commands(|| ());
        assert_eq!(*ended.borrow(), [ReplacementOutcome::Applied]);
        assert_eq!(p.replacement_pending(), None);
    }

    // One refused when it is prepared is told so, and not tried again; one
    // its presenter outlives is dropped.
    ended.borrow_mut().clear();
    p.pointer_down(100., 150., 3.).unwrap();
    let held = p.reload_with_outcome(b"not a plan", (), None, false, tell(&ended));
    assert_eq!(held.unwrap(), ReloadResult::Held("pointer"));
    p.pointer_up(100., 150., 4.).unwrap();
    p.run_commands(|| ());
    p.run_commands(|| ());
    assert!(matches!(
        ended.borrow()[..],
        [ReplacementOutcome::Refused(_)]
    ));
    assert_eq!(label(&p), "four");
    p.pointer_down(100., 150., 5.).unwrap();
    let held = p.reload_with_outcome(&plan("five"), (), None, false, tell(&ended));
    assert_eq!(held.unwrap(), ReloadResult::Held("pointer"));
    drop(p);
    assert_eq!(ended.borrow()[1..], [ReplacementOutcome::Dropped]);
}

/// A press the presenter gives a scrollbar, a sheet or a menu makes no
/// contact, and one it cancels (a wheel taking over) ends its contact: the
/// button is down all the same, and holds a replacement until its release
/// (LLP 1029.000.002 D9).
#[test]
fn a_press_with_no_contact_holds_a_replacement_until_its_release() {
    let plan = |label: &str| {
        crate::test_plan(&format!(
            "component App\n  view\n    column width=240 height=200 overflow-y=\"auto\"\n      text \"{label}\" testId=\"label\" width=200 height=1000 flex-shrink=0\n"
        ))
    };
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan("one"),
        (),
        (240., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let label = |p: &Presenter<()>| {
        let k = p.host.kernel();
        let node = k.node_by_key(k.find_by_test_id("label")[0]).unwrap();
        node.props.str(PropId::Text).unwrap().to_string()
    };
    p.set_scrollbars(false);
    p.frame();
    let bar = p.brush.scrollbars.bars[0];
    let (x, y) = (
        bar.thumb.0 + bar.thumb.2 / 2.,
        bar.thumb.1 + bar.thumb.3 / 2.,
    );
    p.pointer_down(x, y, 0.).unwrap();
    assert!(
        p.contact_position().is_none(),
        "a thumb's drag is no contact"
    );
    assert_eq!(
        p.reload(&plan("two"), ()).unwrap(),
        ReloadResult::Held("pointer")
    );
    p.pointer_move(x, y + 20., 1.).unwrap();
    p.run_commands(|| ());
    assert_eq!(label(&p), "one");
    p.pointer_up(x, y + 20., 2.).unwrap();
    p.run_commands(|| ());
    assert_eq!(p.replacement_pending(), None);
    assert_eq!(label(&p), "two");

    p.pointer_down(100., 100., 3.).unwrap();
    p.pointer_cancel(4.).unwrap();
    assert!(p.contact_position().is_none());
    assert_eq!(
        p.reload(&plan("three"), ()).unwrap(),
        ReloadResult::Held("pointer")
    );
    p.pointer_up(100., 100., 5.).unwrap();
    p.run_commands(|| ());
    assert_eq!(label(&p), "three");
    // Escape abandons the press and leaves the button down.
    p.pointer_down(100., 100., 6.).unwrap();
    p.pointer_abandoned(7.).unwrap();
    assert_eq!(
        p.reload(&plan("four"), ()).unwrap(),
        ReloadResult::Held("pointer")
    );
    p.pointer_up(100., 100., 8.).unwrap();
    p.run_commands(|| ());
    assert_eq!(label(&p), "four");
    // A pointer the device lost has no button down.
    p.pointer_down(100., 100., 9.).unwrap();
    p.pointer_lost(10.).unwrap();
    assert_eq!(
        p.reload(&plan("five"), ()).unwrap(),
        ReloadResult::Applied(None)
    );
    // A finger on a shell's bar stays down when it leaves its target.
    p.bar_event(crate::presenter::bars::TOP, 0, 10., 10., 11.);
    p.bar_event(crate::presenter::bars::TOP, 2, 10., 400., 12.);
    p.bar_event(crate::presenter::bars::TOP, 4, 10., 400., 12.);
    assert_eq!(
        p.reload(&plan("six"), ()).unwrap(),
        ReloadResult::Held("pointer")
    );
    p.bar_event(crate::presenter::bars::TOP, 1, 10., 400., 13.);
    p.run_commands(|| ());
    assert_eq!(label(&p), "six");
    // A finger on each bar: lifting either leaves the other down.
    use crate::presenter::bars::{BOTTOM, TOP};
    for (first, second) in [(TOP, BOTTOM), (BOTTOM, TOP)] {
        p.bar_event(first, 0, 10., 10., 14.);
        p.bar_event(second, 0, 10., 10., 14.);
        p.bar_event(first, 1, 10., 10., 15.);
        assert_eq!(
            p.reload(&plan("seven"), ()).unwrap(),
            ReloadResult::Held("pointer"),
            "bar {second} still down"
        );
        p.bar_event(second, 3, 10., 10., 16.);
        p.run_commands(|| ());
        assert_eq!(label(&p), "seven");
        p.reload(&plan("six"), ()).unwrap();
    }
}

/// Android's reader gives a touch that is still down to its scroll
/// (`Contacts.scrollClaimed`), a field or a hosted view: exact2 hears a cancel
/// and no up, and the finger holds a replacement until the reader says it
/// lifted.
#[test]
fn a_touch_the_reader_took_holds_a_replacement_until_the_finger_lifts() {
    let plan = |label: &str| {
        crate::test_plan(&format!(
            "component App\n  view\n    text \"{label}\" width=200 height=200\n"
        ))
    };
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan("one"),
        (),
        (240., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    assert!(!p.set_touching(true));
    p.reader_touch(0, 100., 100., 0.).unwrap();
    p.reader_touch(2, 100., 140., 1.).unwrap();
    p.reader_touch(3, 100., 140., 2.).unwrap();
    assert!(p.contact_position().is_none());
    let held = p.reload(&plan("two"), ()).unwrap();
    assert_eq!(held, ReloadResult::Held("pointer"));
    p.run_commands(|| ());
    assert_eq!(p.replacement_pending(), Some("pointer"));
    assert!(p.set_touching(false), "the lift asks for a turn");
    p.run_commands(|| ());
    assert_eq!(p.replacement_pending(), None);
    // A touch a field or a hosted view has from its down reaches no
    // `pointer_down` at all.
    assert!(!p.set_touching(true));
    let held = p.reload(&plan("three"), ()).unwrap();
    assert_eq!(held, ReloadResult::Held("pointer"));
    assert!(p.set_touching(false));
}
