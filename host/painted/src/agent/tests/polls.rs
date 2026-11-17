//! Bound package calls made by a poll during an agent clock operation.
use super::*;
use crate::package::{json, Context, Entry, Json, Package, Reply};
use crate::presenter::Handed;

struct Counter {
    context: Context,
    calls: usize,
}

impl Package for Counter {
    fn new(context: Context) -> Self {
        Self { context, calls: 0 }
    }

    fn later(&mut self, name: &str, _: &Json, _: Option<&Json>, reply: Reply) {
        assert_eq!(name, "polled");
        self.calls += 1;
        self.context
            .log(format!("call {} at {}", self.calls, self.context.now()));
        reply.send(json!(self.calls));
    }
}

#[test]
fn a_clock_jump_services_each_polled_package_call_before_replying() {
    let plan = contract::compile(
        "component App\n  resource value = polled() as shape number\n  view\n    text value.toString()\n",
    )
    .unwrap();
    let source = exact_plan::SourcesId(
        plan.sources
            .iter()
            .position(|s| plan.str(s.name) == "polled")
            .unwrap() as u32,
    );
    let mut builder = exact_plan::builder::PlanBuilder::from_plan(plan);
    builder.module(
        source,
        ("counter", "@fixture/counter"),
        "polled",
        false,
        true,
        "never",
        "",
        r#"{"default":0,"poll":{"option":"interval","default":100}}"#,
    );
    let (mut p, error) = Presenter::boot_with(
        &builder.finish().unwrap().encode(),
        (),
        (100., 40.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_packages(vec![Entry::of::<Counter>("counter")]);
    let bound = std::time::Duration::from_secs(2);
    let initial = handle(&mut p, r#"{"op":"clock","data":true}"#);
    assert!(initial.contains("\"settled\":true"), "{initial}");
    assert_eq!(
        p.host().runner().resource("value"),
        Some(&Value::Number(1.))
    );

    // A poll is outside the timer wheel: crossing several periods still asks once,
    // at the landing time, and seeking to that same time asks nothing more.
    for (to, calls) in [(100, 2), (200, 3), (1000, 4), (1000, 4)] {
        let reply = clock_within(&mut p, &format!(r#"{{"op":"clock","to":{to}}}"#), bound);
        assert_eq!(p.host().now(), f64::from(to), "{reply}");
        assert_eq!(
            p.host().runner().resource("value"),
            Some(&Value::Number(f64::from(calls))),
            "the poll's reply must commit before the clock answers: {reply}"
        );
        assert!(!p.host().runner().polls_asking(), "{reply}");
    }
    let calls: Vec<_> = p
        .host()
        .runner()
        .journal()
        .filter(|line| line.contains("counter: call "))
        .collect();
    assert_eq!(
        calls,
        [
            "t=0 counter: call 1 at 0",
            "t=100 counter: call 2 at 100",
            "t=200 counter: call 3 at 200",
            "t=1000 counter: call 4 at 1000"
        ]
    );
}

#[test]
fn a_clock_wait_services_the_readers_requests_and_answers_on_its_owner() {
    let plan = contract::compile(
        "component App\n  resource value = polled() as shape number\n  view\n    text value.toString()\n",
    ).unwrap();
    let source = exact_plan::SourcesId(
        plan.sources
            .iter()
            .position(|s| plan.str(s.name) == "polled")
            .unwrap() as u32,
    );
    let mut builder = exact_plan::builder::PlanBuilder::from_plan(plan);
    builder.module(
        source,
        ("counter", "@fixture/counter"),
        "polled",
        false,
        true,
        "never",
        "",
        r#"{"default":0,"poll":{"option":"interval","default":100}}"#,
    );
    let (mut p, error) = Presenter::boot_with(
        &builder.finish().unwrap().encode(),
        (),
        (100., 40.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_requests();
    p.bind_modules(vec!["counter".into()], vec![]);
    let owner = std::thread::current().id();
    let mut calls = 0;
    let mut replies = Vec::new();
    p.set_agent_turn(Some(Box::new(move |_now, words| {
        assert_eq!(std::thread::current().id(), owner);
        let ready = std::mem::take(&mut replies);
        if let Some(words) = words {
            let mut at = 0;
            while at < words.len() {
                let end = at + 3 + words[at + 2] as usize;
                if words[at] == 11 {
                    calls += 1;
                    replies.push(Handed::Answer(words[at + 1], Some(calls.to_string())));
                }
                at = end;
            }
        }
        ready
    })));
    let bound = std::time::Duration::from_secs(1);
    p.pump(0.);
    p.package_turn();
    assert!(p.pending(), "the bound module has an unanswered request");
    let initial = clock_within(&mut p, r#"{"op":"clock","settle":true}"#, bound);
    assert!(
        initial.contains("\"settled\":true"),
        "the reader's answer waited behind the clock op: {initial}"
    );
    assert_eq!(
        p.host().runner().resource("value"),
        Some(&Value::Number(1.))
    );
    for (to, calls) in [(100, 2), (200, 3), (1000, 4), (1000, 4)] {
        let reply = clock_within(&mut p, &format!(r#"{{"op":"clock","to":{to}}}"#), bound);
        assert_eq!(
            p.host().runner().resource("value"),
            Some(&Value::Number(f64::from(calls))),
            "{reply}"
        );
        assert!(!p.host().runner().polls_asking(), "{reply}");
    }
}

/// The app's grants: the line the fixture's pictures need.
#[derive(Default)]
struct Grants;

impl DataSource for Grants {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }

    fn grants(&self) -> &str {
        "device.photos\n"
    }
}

/// `prefer extension` tells the reader's extension, which acts on it on its
/// own thread: a `clock settle` waits for the tell's answer, and what the
/// extension handed the owner before answering lands within the same settle:
/// a topic, which asks its reader again, a picture it revoked, and a journal
/// line (the photos selection drive, `packages/photos/test/android.mjs`).
#[test]
fn a_settle_waits_for_a_tell_and_what_the_extension_handed_before_it() {
    let plan = contract::compile(
        "component App\n  resource value = counted() as shape number\n  view\n    column\n      text value.toString()\n      image \"counter:a\" width=10 height=10 alt=\"a\"\n",
    )
    .unwrap();
    let source = exact_plan::SourcesId(
        plan.sources
            .iter()
            .position(|s| plan.str(s.name) == "counted")
            .unwrap() as u32,
    );
    let mut builder = exact_plan::builder::PlanBuilder::from_plan(plan);
    builder.module(
        source,
        ("counter", "@fixture/counter"),
        "counted",
        false,
        true,
        "never",
        "",
        r#"{"default":0,"implementation":true,"agent":{"word":"string"},"images":{"scheme":"counter","grants":["device.photos"]}}"#,
    );
    let (mut p, error) = Presenter::boot_with(
        &builder.finish().unwrap().encode(),
        Grants,
        (100., 40.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_requests();
    p.bind_modules(vec![], vec!["counter".into()]);
    p.bind_pictures();
    // The reader: what a turn hands it is answered 100 ms later, as its UI
    // thread answers when it gets to it. A picture is answered with pixels
    // by another door (`Native.picture`), here the test's, between operations.
    let pictures = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let asked = pictures.clone();
    let (mut calls, mut later) = (0, Vec::new());
    let mut due = std::time::Instant::now();
    p.set_agent_turn(Some(Box::new(move |_now, words| {
        let now = std::time::Instant::now();
        let ready = if now >= due {
            std::mem::take(&mut later)
        } else {
            Vec::new()
        };
        let words = words.unwrap_or_default();
        if !words.is_empty() && later.is_empty() {
            due = now + std::time::Duration::from_millis(100);
        }
        let mut at = 0;
        while at < words.len() {
            let end = at + 3 + words[at + 2] as usize;
            if words[at] == 11 {
                let bytes: Vec<u8> = (0..words[at + 3] as usize)
                    .map(|k| (words[at + 4 + k / 4] >> (8 * (k % 4))) as u8)
                    .collect();
                let body = String::from_utf8(bytes).unwrap();
                if body.contains("\"picture\"") {
                    asked.borrow_mut().push(words[at + 1] as u32);
                } else if body.contains("\"source\"") {
                    calls += 1;
                    later.push(Handed::Answer(words[at + 1], Some(calls.to_string())));
                } else {
                    if body.contains("\"preferred\"") {
                        later.push(Handed::Changed("counter.counted".into()));
                    }
                    if body.contains("\"word\"") {
                        later.push(Handed::Picture {
                            picture: "counter:a".into(),
                            revoked: true,
                        });
                        later.push(Handed::Journal {
                            journal: "counter: revoked counter:a".into(),
                        });
                    }
                    later.push(Handed::Answer(words[at + 1], Some("{}".into())));
                }
            }
            at = end;
        }
        ready
    })));
    let settle = r#"{"op":"clock","settle":true}"#;
    let first = handle(&mut p, settle);
    assert!(first.contains("\"settled\":true"), "{first}");
    // Binding tells the extension too, which announces: asked twice.
    assert_eq!(
        p.host().runner().resource("value"),
        Some(&Value::Number(2.))
    );
    let ids: Vec<u32> = pictures.borrow_mut().drain(..).collect();
    assert_eq!(ids.len(), 1, "the picture is asked for once");
    assert!(p.picture_answered(ids[0], Ok(tiny_skia::Pixmap::new(4, 4).unwrap())));
    assert_eq!(p.images().bitmaps.len(), 1, "the picture is drawn");
    let told = handle(
        &mut p,
        r#"{"op":"prefer","extension":{"counter.word":"x"}}"#,
    );
    assert!(!told.contains("error"), "{told}");
    let reply = handle(&mut p, settle);
    assert!(reply.contains("\"settled\":true"), "{reply}");
    assert_eq!(
        p.host().runner().resource("value"),
        Some(&Value::Number(3.)),
        "the settle answered before the tell's announced change asked again: {reply}"
    );
    assert!(
        p.images().bitmaps.is_empty(),
        "the settle answered before the tell's revocation emptied the box: {reply}"
    );
    assert!(
        pictures.borrow().is_empty(),
        "a revoked picture is not asked for again"
    );
    assert!(
        (p.host().runner().journal()).any(|l| l.ends_with("counter: revoked counter:a")),
        "the settle answered before the tell's journal line"
    );
}
