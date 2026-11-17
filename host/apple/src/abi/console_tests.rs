//! An app's `console` lines through the Apple host (LLP 1027.009 D1, D2):
//! each reaches the runtime's callback as the runner journals it, in the
//! journal's order, whatever the batch around it carries; a candidate that
//! is never committed writes nothing.
#![allow(unsafe_code)]
use super::super::{Bridge, Hooks};
use exact_plan::Value;
use exact_runner::{Answer, Console, DataError, DataSource, InFlight, Request, Store};
use std::cell::RefCell;

thread_local! {
    static WRITTEN: RefCell<Vec<(u8, String)>> = const { RefCell::new(Vec::new()) };
}

extern "C" fn record(method: u8, line: *const u8, len: usize) {
    // SAFETY: the host lends the line's bytes for the call.
    let bytes = unsafe { std::slice::from_raw_parts(line, len) };
    let line = String::from_utf8_lossy(bytes).into_owned();
    WRITTEN.with(|w| w.borrow_mut().push((method, line)));
}

/// What the callback was handed since the last call.
fn written() -> Vec<(u8, String)> {
    WRITTEN.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

fn hooks() -> Hooks {
    Hooks {
        console: Some(record),
        ..Hooks::none()
    }
}

/// `read(id)` logs and answers `id`, or with `later` asks a fetch that never
/// lands; `say()` logs twice; a request let go logs from its `finally`.
#[derive(Default)]
struct Logging {
    later: bool,
    logs: Vec<String>,
}

impl DataSource for Logging {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        match source {
            "read" => {
                let id = args.first().and_then(Value::as_str).unwrap_or("");
                self.logs.push(Console::Log.mark(&format!("read {id}")));
                Ok(if self.later {
                    Answer::Later(Request::get(&format!("https://console.invalid/{id}")))
                } else {
                    Answer::Now(Value::str(id))
                })
            }
            "say" => {
                self.logs.push(Console::Debug.mark("said"));
                self.logs.push(Console::Error.mark("said again"));
                Ok(Answer::Now(Value::Record(vec![Value::Number(1.)].into())))
            }
            _ => Err(DataError::UnknownSource(source.into())),
        }
    }

    fn forgotten(&mut self, _: &Store, in_flight: &[InFlight<'_>]) {
        if self.later {
            let line = format!("finally, {} in flight", in_flight.len());
            self.logs.push(Console::Warn.mark(&line));
        }
    }

    fn take_logs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.logs)
    }
}

const APP: &str = r#"shape Said
  n: number
component App
  state id = "a"
  resource page = read(id) as shape string
  mutation said as shape Said
  action say
    send said = say()
  action next
    id = "b"
  view
    column
      button press=say testId="say"
        text "say"
      button press=next testId="next"
        text "next"
"#;

fn plan() -> Vec<u8> {
    contract::compile(APP).unwrap().encode()
}

/// A reload's plan: one resource more, which its candidate asks at boot.
fn reloaded() -> Vec<u8> {
    let page = "  resource page = read(id) as shape string\n";
    let more = format!("{page}  resource more = read(\"more\") as shape string\n");
    contract::compile(&APP.replace(page, &more))
        .unwrap()
        .encode()
}

fn booted(source: Logging) -> Bridge<Logging> {
    let mut bridge = Bridge::new();
    bridge.boot(&plan(), source, hooks(), 390.0, 844.0);
    bridge.data_ready();
    bridge
}

fn press(bridge: &mut Bridge<Logging>, test_id: &str) -> String {
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    let view = kernel
        .node_by_key(kernel.find_by_test_id(test_id)[0])
        .unwrap()
        .id;
    let n = bridge.dispatch(view, 0, 0, 0.0);
    String::from_utf8_lossy(bridge.output_bytes(n as usize)).into_owned()
}

/// The journal's `console` lines, as the app wrote them.
fn journaled(bridge: &Bridge<Logging>) -> Vec<String> {
    let runner = bridge.host.as_ref().unwrap().runner();
    runner
        .journal()
        .filter_map(|l| l.split_once(" console: ").map(|(_, l)| l.to_string()))
        .collect()
}

/// A handler whose batch changes nothing on screen still has its lines
/// written, at their levels.
#[test]
fn a_line_whose_batch_changes_nothing_reaches_the_callback() {
    let mut bridge = booted(Logging::default());
    assert_eq!(written(), [(0, "read a".to_string())]);
    let batch = press(&mut bridge, "say");
    assert!(batch.contains("\"ops\":[]"), "{batch}");
    assert_eq!(
        written(),
        [(4, "said".to_string()), (3, "said again".to_string())]
    );
}

/// Every line reaches the callback once, in the journal's order, across
/// answers, including one journaled when the host takes the requests: the
/// `finally` of a fetch the commit let go.
#[test]
fn lines_reach_the_callback_in_the_journals_order() {
    let mut bridge = booted(Logging {
        later: true,
        ..Logging::default()
    });
    press(&mut bridge, "next");
    press(&mut bridge, "say");
    let journal = journaled(&bridge);
    assert!(
        journal.iter().any(|l| l.starts_with("finally")),
        "{journal:?}"
    );
    let lines: Vec<String> = written().into_iter().map(|(_, l)| l).collect();
    assert_eq!(lines, journal);
}

/// A candidate prepared and then dropped or replaced writes nothing; the one
/// committed writes the lines it journaled once, in order.
#[test]
fn only_a_committed_candidate_writes_its_lines() {
    let bytes = reloaded();
    let mut bridge = booted(Logging::default());
    written();
    let prepare = |bridge: &mut Bridge<Logging>| {
        bridge.input_write(&bytes);
        bridge.prepare_plan(bytes.len(), Logging::default(), hooks(), 390.0, 844.0);
        let candidate = &bridge.prepared.as_ref().expect("prepared").host;
        let held: Vec<&str> = candidate
            .runner()
            .journal()
            .filter(|l| l.contains(" console: "))
            .collect();
        assert!(!held.is_empty(), "the candidate journals the app's lines");
    };
    prepare(&mut bridge);
    bridge.discard_plan();
    prepare(&mut bridge);
    prepare(&mut bridge);
    assert!(
        written().is_empty(),
        "a candidate not committed writes nothing"
    );
    let held = {
        let candidate = &bridge.prepared.as_ref().unwrap().host;
        let runner = candidate.runner();
        runner
            .journal()
            .filter_map(|l| l.split_once(" console: ").map(|(_, l)| l.to_string()))
            .collect::<Vec<_>>()
    };
    bridge.commit_plan();
    assert_eq!(held, ["read more"]);
    let lines: Vec<String> = written().into_iter().map(|(_, l)| l).collect();
    assert_eq!(lines, held);
    press(&mut bridge, "say");
    assert_eq!(
        written(),
        [(4, "said".to_string()), (3, "said again".to_string())]
    );
}
