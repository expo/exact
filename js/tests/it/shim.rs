//! Console diagnostics through the native Hermes bridge.
#![cfg(exact_js_engine)]

use exact_js::Module;
use exact_runner::DataSource;

fn console(source: &str) -> (String, Vec<String>) {
    let plan = contract::compile(&format!(
        "component App\n  resource result = {source}() as shape string\n  view\n    text result\n"
    ))
    .unwrap();
    let mut module = Module::new(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        "",
    );
    module.set_budget_ms(f64::INFINITY);
    module.bind(&plan);
    module.activate().unwrap();
    let mut store = exact_runner::Store::new("", vec![]);
    let value = super::castle::now(module.answer(&mut store, source, &[]).unwrap());
    let logs = crate::authored_logs(module.take_logs());
    assert!(module.take_logs().is_empty(), "logs are taken once");
    (value.as_str().unwrap().to_owned(), logs)
}

#[test]
fn console_reports_throwing_abort_listeners() {
    let (answer, logs) = console("consoleAbortListeners");
    assert_eq!(answer, "reached the return");
    assert_eq!(
        logs,
        [
            "Uncaught Error: boom",
            "Uncaught RangeError: listener",
            "later listener",
        ]
    );
}

#[test]
fn console_trailing_newline_does_not_add_an_empty_line() {
    let (answer, logs) = console("consoleTrailingNewline");
    assert_eq!(answer, "reached the return");
    assert_eq!(logs, ["done"]);
}

#[test]
fn console_keeps_a_trailing_carriage_return() {
    let (answer, logs) = console("consoleTrailingCarriageReturn");
    assert_eq!(answer, "reached the return");
    assert_eq!(logs, ["100%\r"]);
}

#[test]
fn console_splits_lines_within_each_record() {
    let (answer, logs) = console("consoleLineEndings");
    assert_eq!(answer, "reached the return");
    assert_eq!(
        logs,
        [
            "done",
            "100%\r",
            "next",
            "",
            "",
            "a",
            "b",
            "x\0é😀",
            "end",
            ""
        ]
    );
}

#[test]
fn console_errors_keep_their_name_message_and_stack() {
    let (stack, logs) = console("consoleErrors");
    assert!(stack.starts_with("TypeError: boom\n"), "{stack}");
    assert!(stack.lines().count() > 1, "the error has a stack");
    let expected = format!("{stack}\nload failed {stack}");
    assert_eq!(logs, expected.lines().collect::<Vec<_>>());
}

#[test]
fn console_cycles_do_not_fail_the_answer() {
    let (answer, logs) = console("consoleCycle");
    assert_eq!(answer, "reached the return");
    assert_eq!(logs, ["row [object Object]", "after"]);
}

#[test]
fn console_bigints_do_not_fail_the_answer() {
    let (answer, logs) = console("consoleBigInt");
    assert_eq!(answer, "reached the return");
    assert_eq!(logs, ["bigint 1 [object Object]"]);
}

#[test]
fn console_falls_back_even_when_application_formatters_throw() {
    let (answer, logs) = console("consoleFallback");
    assert_eq!(answer, "reached the return");
    assert_eq!(
        logs,
        [
            "RangeError: no stack",
            "fallback",
            "[unprintable] after",
            "null undefined true 42 Symbol(label) {\"ok\":true}",
        ]
    );
}

#[test]
fn console_keeps_empty_lines() {
    let (answer, logs) = console("consoleEmpty");
    assert_eq!(answer, "reached the return");
    assert_eq!(logs, [""]);
    let (_, logs) = console("consoleEmptyBatch");
    assert_eq!(logs, ["", "middle", "", ""]);
}

/// LLP 1027.009 D1, D2: each line reaches the host's sink with the method
/// that wrote it, once and in order, the lines journaled before the sink was
/// installed first, and the journal says `console: …` as before. While that
/// sink is installed, a module evaluated, answered and dropped, and a runner
/// booted and dropped without a sink of its own, write nothing.
#[test]
fn console_lines_reach_the_host_with_their_method() {
    use exact_runner::Console::{self, *};
    let source = "component App\n  resource result = consoleLevels() as shape string\n  view\n    text result\n";
    let plan = contract::compile(source).unwrap();
    let module = || {
        let mut module = Module::new(
            include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
            "test.pure",
            "",
        );
        module.set_budget_ms(f64::INFINITY);
        module
    };
    let boot = || {
        let mut module = module();
        module.load().expect("the fixture loads");
        exact_runner::Runner::boot(
            plan.clone(),
            module,
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap()
    };
    let mut r = boot();
    assert_eq!(
        r.resource("result").and_then(exact_plan::Value::as_str),
        Some("reached the return")
    );
    let written = std::rc::Rc::<std::cell::RefCell<Vec<(Console, String)>>>::default();
    let into = written.clone();
    r.set_console(move |method, line| into.borrow_mut().push((method, line.to_string())));
    let line = |method: Console, line: &str| (method, line.to_string());
    assert_eq!(
        *written.borrow(),
        [
            line(Debug, "d"),
            line(Log, "l"),
            line(Info, "i"),
            line(Warn, "w"),
            line(Error, "e"),
            line(Error, "second"),
        ]
    );
    let journal: Vec<&str> = r
        .journal()
        .filter_map(|l| l.split_once(' ').map(|(_, l)| l))
        .filter(|l| l.starts_with("console") && !l.contains("top level took"))
        .collect();
    assert_eq!(
        journal,
        [
            "console: d",
            "console: l",
            "console: i",
            "console: w",
            "console: e",
            "console: second"
        ]
    );

    let mut dropped = module();
    dropped.bind(&plan);
    dropped.activate().unwrap();
    let mut store = exact_runner::Store::new("", vec![]);
    super::castle::now(dropped.answer(&mut store, "consoleLevels", &[]).unwrap());
    drop(dropped);
    let unadopted = boot();
    assert!(unadopted.journal().any(|l| l.ends_with("console: second")));
    drop(unadopted);
    assert_eq!(written.borrow().len(), 6, "{:?}", written.borrow());
}

/// What the `runtime-lines` fixture's `source` left (LLP 1027.009 D1).
struct Said {
    answer: String,
    /// The journal's `console` lines.
    journal: Vec<String>,
    /// What reached the host's sink.
    written: Vec<(exact_runner::Console, String)>,
}

/// Boots the `runtime-lines` fixture on `source`, its door's journal op
/// refused when `no_door`, and installs a sink.
fn runtime_lines(source: &str, no_door: bool) -> Said {
    let plan = contract::compile(&format!(
        "component App\n  resource result = {source}() as shape string\n  view\n    text result\n"
    ))
    .unwrap();
    let mut module = Module::new(
        include_bytes!(concat!(env!("OUT_DIR"), "/runtime-lines.hbc")).to_vec(),
        "test.runtime-lines",
        "",
    );
    module.set_budget_ms(f64::INFINITY);
    if no_door {
        module.refuse_door_op(13);
    }
    module.load().expect("the fixture loads");
    let mut r = exact_runner::Runner::boot(
        plan,
        module,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let written = std::rc::Rc::<std::cell::RefCell<Vec<_>>>::default();
    let into = written.clone();
    r.set_console(move |method, line| into.borrow_mut().push((method, line.to_string())));
    Said {
        answer: (r.resource("result").and_then(exact_plan::Value::as_str))
            .unwrap_or_default()
            .to_string(),
        journal: r
            .journal()
            .filter_map(|l| l.split_once(' ').map(|(_, l)| l.to_string()))
            .filter(|l| l.starts_with("console") && !l.contains("top level took"))
            .collect(),
        written: written.take(),
    }
}

/// The app's own `console.error` lines, at load and in `source`'s answer:
/// all that reaches the sink.
fn the_apps(source: &str) -> Vec<(exact_runner::Console, String)> {
    use exact_runner::Console::Error;
    vec![
        (Error, "the app at load".into()),
        (Error, format!("the app's {source}")),
    ]
}

/// The prelude's refusal of a fetch with no answer running stays in the
/// journal; the hand-offs that carry the runtime's line are gone before the
/// app loads.
#[test]
fn a_refused_fetch_is_the_runtimes_line() {
    let said = runtime_lines("handoffs", false);
    assert_eq!(said.answer, "[]", "nothing the app could call remains");
    assert_eq!(
        said.journal,
        [
            "console: the app at load",
            "console: fetch() called outside an answer: it was never run. Start a fetch inside an answer",
            "console: the app's handoffs",
        ]
    );
    assert_eq!(said.written, the_apps("handoffs"));
}

/// An error a `queueMicrotask` callback did not catch (js/src/standard.js).
#[test]
fn a_microtasks_uncaught_error_is_the_runtimes_line() {
    let said = runtime_lines("microtask", false);
    assert!(
        said.journal
            .iter()
            .any(|l| l == "console: Uncaught Error: task"),
        "{:#?}",
        said.journal
    );
    assert_eq!(said.written, the_apps("microtask"));
}

/// An error an abort listener did not catch (Ibex's abort binding, through
/// console.js's `reportError`).
#[test]
fn an_abort_listeners_uncaught_error_is_the_runtimes_line() {
    let said = runtime_lines("abort", false);
    assert!(
        (said.journal.iter()).any(|l| l == "console: Uncaught RangeError: listener"),
        "{:#?}",
        said.journal
    );
    assert_eq!(said.written, the_apps("abort"));
}

/// A compact notation the host formats in full (js/src/intl.js); an engine
/// that formats it says nothing.
#[test]
fn an_unsupported_compact_notation_is_the_runtimes_line() {
    let said = runtime_lines("compact", false);
    let note = "console: Intl.NumberFormat: notation \"compact\" (long, decimal) in en-US is not available on this host; numbers are formatted in full";
    assert_eq!(
        said.journal.iter().any(|l| l == note),
        said.answer == "1,234",
        "{}: {:#?}",
        said.answer,
        said.journal
    );
    assert_eq!(said.written, the_apps("compact"));
}

/// A runtime journal line through a door without the journal's op is
/// written through the console, as the runtime's (js/src/prelude.js
/// `journal`).
#[test]
fn a_journal_line_without_its_door_is_the_runtimes_line() {
    let said = runtime_lines("rejection", true);
    assert!(
        (said.journal.iter()).any(|l| l == "console: data: unhandled rejection: lost"),
        "{:#?}",
        said.journal
    );
    assert_eq!(said.written, the_apps("rejection"));
}
