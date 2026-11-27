//! Shelf's book screen owns its photo's read (LLP 1035.005.004 D6): on Shelf's
//! own plan, a read whose book screen left, by Back or by another book taking
//! its place, opens no quote sheet when its answer comes. The test holds the
//! read's answer, as no authored test can hold the platform's recognizer.

use crate::source::{boxed, Source};
use exact_plan::{Plan, Value};
use exact_runner::{
    ControlValue, DataError, DataSource, Dispatch, Event, Outcome, Picked, Reply, RequestOut,
    Response, Runner, Work,
};
use serde_json::{json, Value as Json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// Shelf's sources: two books, and every other resource's empty value.
#[derive(Default)]
struct Library(BTreeMap<String, Value>);
impl DataSource for Library {
    fn bind(&mut self, plan: &Plan) {
        for row in &plan.resources {
            if let Ok(value) = exact_runner::surface_record::decode(plan, row.ty, None) {
                self.0.insert(plan.str(row.source).into(), value);
            }
        }
    }
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        let book = |id: &str, title: &str| {
            Value::record(vec![
                Value::str(id),
                Value::str(title),
                Value::str(""),
                Value::Number(300.),
                Value::Number(0.),
                Value::str("want"),
                Value::Number(0.),
                Value::str(""),
                Value::Number(0.),
                Value::str(""),
                Value::str(""),
                Value::Number(0.),
            ])
        };
        match source {
            "library" => Ok(Value::record(vec![
                Value::Bool(true),
                Value::list(vec![book("seed-1", "One"), book("seed-2", "Two")]),
                Value::Number(12.),
                Value::Number(0.),
                Value::str("—"),
            ])),
            _ => {
                (self.0.get(source).cloned()).ok_or_else(|| DataError::UnknownSource(source.into()))
            }
        }
    }
    fn grants(&self) -> &str {
        "fs.read app:/tmp/picked\n"
    }
}

/// Shelf on macOS, its text detection played by the native slot, which says
/// the recognizer is available.
fn boot() -> Runner<Source> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/shelf/app.contract");
    let mut r = Runner::boot(
        contract::compile_path(&path).unwrap(),
        boxed(Library::default()),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.native_slot()
        .host(Some(Arc::new(|_: Vec<u8>, reply: Reply| {
            reply.send(answer(
                json!({ "availability": "available", "languages": ["en-US"] }),
            ));
        })));
    r.set_module_target("macos");
    r.bind_extensions(&[], &["text-detection"]);
    assert!(pump(&mut r).is_empty());
    r
}

fn answer(body: Json) -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: Vec::new(),
        body: body.to_string().into_bytes(),
    })
}

/// The host's part, until nothing is left to do: each request run and
/// fulfilled, except the book screen's read, which is returned unanswered.
fn pump(r: &mut Runner<Source>) -> Vec<RequestOut> {
    let mut reads = Vec::new();
    for _ in 0..8 {
        let outs = r.take_requests();
        let released = r.release_work();
        if outs.is_empty() && released.is_empty() && r.apply_announced().0.is_empty() {
            break;
        }
        for (_, dispatch) in released {
            if let Dispatch::Run(Work::Now(work)) = dispatch {
                work();
            }
        }
        for out in outs {
            if out.target.contains("quoted") {
                reads.push(out);
                continue;
            }
            let dispatch = match out.request.continuation {
                Some(token) => r.dispatch_work(token),
                None => r.native_work(&out.request),
            };
            let outcome = match dispatch {
                Dispatch::Run(Work::Now(work)) => work(),
                Dispatch::Run(Work::Later(work)) => {
                    let (tx, rx) = std::sync::mpsc::channel();
                    work(Reply::new(move |o| tx.send(o).unwrap()));
                    rx.recv().unwrap()
                }
                _ => continue,
            };
            let _ = r.fulfill(out.ticket, outcome);
        }
    }
    reads
}

fn state(r: &Runner<Source>) -> Json {
    serde_json::from_str(&exact_runner::agent::state(r)).unwrap()
}

/// The selected tab's stack, from root to top, as the router holds its entries.
fn entries(r: &Runner<Source>) -> Vec<Json> {
    let nav = &state(r)["slots"]["nav"];
    let tabs = nav["tabs"].as_array().unwrap();
    let shown = tabs.iter().find(|t| t["name"] == nav["tab"]).unwrap();
    shown["stack"].as_array().unwrap().clone()
}

/// The selected tab's stack, as `(entry id, route name)` from root to top.
fn stack(r: &Runner<Source>) -> Vec<(u64, String)> {
    (entries(r).iter())
        .map(|e| {
            (
                e["id"].as_u64().unwrap(),
                e["name"].as_str().unwrap().into(),
            )
        })
        .collect()
}

/// The `id` of the book shown, the top of the selected tab's stack.
fn book_shown(r: &Runner<Source>) -> Json {
    let top = entries(r).pop().unwrap();
    assert_eq!(top["name"], "book", "{top}");
    top["params"]["id"].clone()
}

/// A link to `location` followed through Shelf's navigation root, as a row's is.
fn follow(r: &mut Runner<Source>, location: &str) {
    let root = r.kernel().roots()[0];
    r.dispatch(root, Event::Navigate(location.into())).unwrap();
}

/// A photo picked for a quote on the book shown: the read's request, held.
fn read_a_photo(r: &mut Runner<Source>) -> RequestOut {
    let key = r.kernel().find_by_test_id("quote-photo")[0];
    let input = r.kernel().node_by_key(key).unwrap().id;
    let files = Picked::payload("app:/tmp/picked/1-1.png\tquote.png\timage/png\t9\t\t\t").unwrap();
    r.dispatch(input, Event::Change(ControlValue::Files(files)))
        .unwrap();
    let mut reads = pump(r);
    assert_eq!(reads.len(), 1, "one read: {}", state(r));
    let read = reads.remove(0);
    assert!(r.holds(read.ticket));
    read
}

/// The read's answer after its screen left: no commit, no `then`, no sheet.
fn answer_lands_nowhere(r: &mut Runner<Source>, read: &RequestOut) {
    assert!(!r.holds(read.ticket));
    let lines = json!({ "texts": [{ "rawValue": "It is a truth", "boundingBox": { "x": 0, "y": 0, "width": 1, "height": 1 }, "cornerPoints": [] }], "width": 1, "height": 1 });
    assert_eq!(r.fulfill(read.ticket, answer(lines)).unwrap(), None);
    let landed = r.land_then();
    assert!(landed.error.is_none(), "{:?}", landed.error);
    assert!(landed.receipts.is_empty());
    assert_eq!(state(r)["slots"]["quoteLines"], json!([]));
    assert!(
        stack(r).iter().all(|(_, name)| name != "quote"),
        "{:?}",
        stack(r)
    );
}

#[test]
fn a_photo_read_for_a_book_screen_that_left_opens_no_quote_sheet() {
    // Back to the library before the read answers.
    let mut r = boot();
    follow(&mut r, "/book/seed-2");
    assert_eq!(book_shown(&r), "seed-2");
    let read = read_a_photo(&mut r);
    r.act("back", vec![]).unwrap();
    assert_eq!(stack(&r).last().unwrap().1, "library");
    answer_lands_nowhere(&mut r, &read);

    // Another book takes its place, and the read's own book comes back as a
    // new visit in that place: the guard on the book shown passes, so only
    // the screen's ownership keeps the old read out.
    follow(&mut r, "/book/seed-2");
    let first = stack(&r);
    let read = read_a_photo(&mut r);
    follow(&mut r, "/book/seed-1");
    assert_eq!(book_shown(&r), "seed-1");
    follow(&mut r, "/book/seed-2");
    assert_eq!(book_shown(&r), "seed-2");
    let again = stack(&r);
    assert_eq!(again.len(), first.len());
    assert_eq!(again.last().unwrap().1, "book");
    assert_ne!(
        again.last().unwrap().0,
        first.last().unwrap().0,
        "a new visit"
    );
    answer_lands_nowhere(&mut r, &read);
    assert_eq!(stack(&r), again);
}
