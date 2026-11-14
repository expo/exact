//! An app's `console` lines, the runner half (LLP 1027.009): a scripted
//! module whose journal lines are given, no engine.
use super::super::*;
use super::Console::{self, *};
use crate::Store;
use exact_kernel::Kernel;

/// `say(n)` answers `n` now, its journal the lines in `lines`.
#[derive(Default)]
struct Saying {
    lines: Vec<String>,
    logs: Vec<String>,
}

impl DataSource for Saying {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the fixture implements answer")
    }

    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        self.logs.append(&mut self.lines);
        Ok(Answer::Now(Value::Record(vec![args[0].clone()].into())))
    }

    fn take_logs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.logs)
    }
}

fn runner() -> Runner<Saying> {
    let plan = contract::compile(
        r#"
shape Said
  n: number
component App
  mutation said as shape Said
  action say(n: number)
    send said = say(n)
  view
    text "saying"
"#,
    )
    .unwrap();
    Runner::boot(
        plan,
        Saying::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn say(r: &mut Runner<Saying>, lines: &[String]) {
    r.data().lines = lines.to_vec();
    r.act("say", vec![Value::Number(1.)]).unwrap();
}

/// The journal's module lines, without their clock stamp.
fn journaled(r: &Runner<Saying>) -> Vec<String> {
    r.journal()
        .filter_map(|l| l.split_once(' ').map(|(_, l)| l.to_string()))
        .filter(|l| l.starts_with("console") || l.starts_with("storage"))
        .collect()
}

/// A sink that keeps what it is handed.
fn sink(r: &mut Runner<Saying>) -> std::rc::Rc<std::cell::RefCell<Vec<(Console, String)>>> {
    let written = std::rc::Rc::<std::cell::RefCell<Vec<(Console, String)>>>::default();
    let into = written.clone();
    r.set_console(move |method, line| into.borrow_mut().push((method, line.to_string())));
    written
}

/// D1, D2: each line the app's `console` wrote reaches the host's sink once,
/// in order, with the method that wrote it, as the journal takes it, in a
/// runner that measures nothing (D3); the journal says `console: …` as it
/// did. The runtime's own lines, a `console: …` among them, reach only the
/// journal.
#[test]
fn console_lines_reach_the_sink_once_with_their_method_and_the_journal_as_before() {
    let mut r = runner();
    let written = sink(&mut r);
    let methods = [Debug, Log, Info, Warn, Error];
    let mut lines: Vec<String> = methods.iter().map(|m| m.mark(m.name())).collect();
    lines.insert(2, "console: the module's top level took 80 ms".into());
    lines.push("storage failed: full".into());
    say(&mut r, &lines);
    let named = |m: &Console| (*m, m.name().to_string());
    assert_eq!(
        written.take(),
        methods.iter().map(named).collect::<Vec<_>>()
    );
    assert_eq!(
        journaled(&r),
        [
            "console: debug",
            "console: log",
            "console: the module's top level took 80 ms",
            "console: info",
            "console: warn",
            "console: error",
            "storage failed: full",
        ]
    );
    assert!(r.take_journal().is_empty() && r.take_alerts().is_empty());

    say(&mut r, &[Warn.mark("again"), Log.mark("")]);
    assert_eq!(
        written.take(),
        [(Warn, "again".to_string()), (Log, String::new())]
    );
    let journal = journaled(&r);
    assert_eq!(
        journal[journal.len() - 2..],
        ["console: again", "console: "]
    );
}

/// Lines journaled before a sink is installed are held no longer than the
/// journal keeps them, and written once, in order, when one is; with a sink
/// installed nothing is held, so nothing is dropped.
#[test]
fn lines_before_a_sink_are_held_and_none_are_dropped_after() {
    let mut r = runner();
    let many = |from: usize| -> Vec<String> {
        (from..from + JOURNAL_RING + 2)
            .map(|i| Log.mark(&i.to_string()))
            .collect()
    };
    say(&mut r, &many(0));
    let written = sink(&mut r);
    let held = written.take();
    assert_eq!(held.len(), JOURNAL_RING);
    assert_eq!(held[0], (Log, "2".to_string()));
    assert_eq!(
        held[JOURNAL_RING - 1],
        (Log, (JOURNAL_RING + 1).to_string())
    );
    say(&mut r, &many(10_000));
    let after = written.take();
    assert_eq!(after.len(), JOURNAL_RING + 2);
    assert_eq!(after[0], (Log, "10000".to_string()));
}

/// While measuring, an app's `console` line reaches the platform's log only
/// through the sink: it is journaled and written once, and the measuring
/// queue, which a development host also writes there, carries only the
/// module's other lines.
#[test]
fn the_measuring_queue_does_not_carry_console_lines_again() {
    let mut r = runner();
    r.measure(true, false);
    r.take_journal();
    let written = sink(&mut r);
    say(&mut r, &[Debug.mark("once"), "storage failed: full".into()]);
    assert_eq!(written.take(), [(Debug, "once".to_string())]);
    assert!(journaled(&r).iter().any(|l| l == "console: once"));
    let queued = r.take_journal();
    assert!(
        queued.iter().any(|l| l == "storage failed: full"),
        "{queued:?}"
    );
    assert!(!queued.iter().any(|l| l.contains("once")), "{queued:?}");
}

/// A widget's refresh source: it logs, then answers an empty timeline, or
/// fails when `fail` is set. With `ask` it logs, asks for a request and
/// answers when the reply comes, logging again; with `again` too, the
/// request's continuation is first asked again (`Dispatch::Again`), which
/// logs and asks for the request.
#[derive(Default)]
struct Refreshing {
    fail: bool,
    ask: bool,
    again: bool,
    asking_again: bool,
    logs: Vec<String>,
}

const FEED: &str = "https://example.com/feed";

impl DataSource for Refreshing {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the fixture implements answer")
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        if self.fail {
            self.logs.push(Error.mark("refusing"));
            return Err(DataError::Unavailable("down".into()));
        }
        if self.ask {
            self.logs.push(Log.mark("asking"));
            let mut request = crate::Request::get(FEED);
            request.continuation = self.again.then_some(7);
            self.asking_again = self.again;
            return Ok(Answer::Later(request));
        }
        self.logs.push(Log.mark("refreshing"));
        self.logs.push(Warn.mark("refreshed"));
        Ok(Answer::Now(Value::list(Vec::new())))
    }

    fn dispatch(&mut self, _: u64, _: &Store) -> crate::Dispatch {
        crate::Dispatch::Again
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: crate::Outcome,
    ) -> Result<Answer, DataError> {
        if std::mem::take(&mut self.asking_again) {
            self.logs.push(Info.mark("asked again"));
            return Ok(Answer::Later(crate::Request::get(FEED)));
        }
        self.logs.push(Warn.mark("answered"));
        Ok(Answer::Now(Value::list(Vec::new())))
    }

    fn take_logs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.logs)
    }
}

/// A widget's refresh writes what it logged once, in order, whether it
/// answers or fails, as each answer and reply ends (D2): what it logged
/// before a request is written before the request runs. Its host closes the
/// session after it.
#[test]
fn a_snapshot_refresh_writes_its_lines_however_it_ends() {
    let widget = contract::Widget {
        kind: "card".into(),
        title: None,
        description: None,
        families: vec!["systemSmall".into()],
        watch_families: vec![],
        component: "Card".into(),
        props: "Props".into(),
        refresh: "refresh".into(),
        requires_secrets: false,
        preview: serde_json::json!({"title": "preview"}),
        configuration: None,
    };
    let plan = contract::snapshot::compile(
        "shape Props\n  title: string\ncomponent Card\n  props\n    title: string\n  view\n    text title\n",
        &widget,
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        Refreshing::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let written = std::rc::Rc::<std::cell::RefCell<Vec<(Console, String)>>>::default();
    let into = written.clone();
    r.set_console(move |method, line| into.borrow_mut().push((method, line.to_string())));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let line = |method: Console, line: &str| (method, line.to_string());
    let ran = std::cell::Cell::new(0);
    let refresh = |r: &mut Runner<Refreshing>, before: &[(Console, String)]| {
        r.snapshot_timeline(1.0, Vec::new(), deadline, |request, _| {
            ran.set(ran.get() + 1);
            assert_eq!(request.url, FEED);
            assert_eq!(*written.borrow(), before, "written before the request runs");
            Ok(crate::Outcome::Failed {
                kind: crate::FailureKind::Network,
                message: "offline".into(),
            })
        })
    };
    assert_eq!(refresh(&mut r, &[]).as_deref(), Ok("[]"));
    assert_eq!(ran.get(), 0);
    assert_eq!(
        written.take(),
        [line(Log, "refreshing"), line(Warn, "refreshed")]
    );

    r.data().ask = true;
    assert_eq!(refresh(&mut r, &[line(Log, "asking")]).as_deref(), Ok("[]"));
    assert_eq!(ran.get(), 1);
    assert_eq!(
        written.take(),
        [line(Log, "asking"), line(Warn, "answered")]
    );

    r.data().again = true;
    let before = [line(Log, "asking"), line(Info, "asked again")];
    assert_eq!(refresh(&mut r, &before).as_deref(), Ok("[]"));
    assert_eq!(ran.get(), 2);
    assert_eq!(
        written.take(),
        [
            line(Log, "asking"),
            line(Info, "asked again"),
            line(Warn, "answered")
        ]
    );

    r.data().fail = true;
    assert!(refresh(&mut r, &[]).is_err());
    assert_eq!(written.take(), [line(Error, "refusing")]);
}

#[test]
fn a_method_survives_its_mark() {
    for m in [Log, Info, Warn, Error, Debug] {
        assert_eq!(Console::named(m.name()), Some(m));
        let marked = m.mark("a: b");
        assert_eq!(Console::unmark(&marked), Some((m, "a: b")));
    }
    assert_eq!(Console::unmark("console: plain"), None);
    assert_eq!(Console::unmark("console.trace: x"), None);
}
