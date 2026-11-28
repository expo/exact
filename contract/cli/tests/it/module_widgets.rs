//! LLP 1126.005 D4: `placedWidgets` from the repository's `@exact/widgets`
//! is checked against `app.json` wherever the program reads it, here from a
//! second source file of the app.

use crate::scratch::Scratch;
use crate::source::{boxed, NoData};
use exact_runner::{Outcome, Reply, Response, Runner};
use serde_json::{json, Value as Json};
use std::path::Path;
use std::sync::Arc;

const STATS: &str = r#"use placedWidgets from "@exact/widgets"

shape Choice
  book: option<string>

shape Placed
  ready: bool
  placements: Choice[]

shape Listed
  ready: bool
  placements: string[]

component Stats
  resource placed = placedWidgets(RESOURCE) as shape ANSWER else empty()
  view
    text "stats"
"#;

const APP: &str = r#"use Stats, Choice from "./stats.contract"

component App
  view
    Stats()

shape Props
  title: string

component Card
  props
    title: string
  INJECT
  view
    Line()

component Line
  INNER
  view
    text "card"
"#;

fn compile(
    resource: &str,
    answer: &str,
    inject: &str,
    inner: &str,
) -> Result<(), Vec<contract::CompileError>> {
    let dir = Scratch::new(format!("exact-placed-widgets-{}", std::process::id()));
    let package = dir.join("node_modules/@exact/widgets");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/widgets");
    for file in ["package.json", "exact.json", "apple/exact.json"] {
        std::fs::create_dir_all(package.join(file).parent().unwrap()).unwrap();
        std::fs::copy(source.join(file), package.join(file)).unwrap();
    }
    let widget = |kind: &str, configured: bool| {
        let mut widget = json!({"kind":kind,"families":["systemSmall"],"component":"Card",
            "props":"Props","refresh":"refresh","preview":{"title":"Preview"}});
        if configured {
            widget["configuration"] = json!({"title":"Card","parameters":[
                {"name":"book","title":"Book","type":"entity","entity":"book"}]});
        }
        widget
    };
    let manifest = json!({"name":"T","app":{"id":"com.example.t","name":"T"},
        "intents":{"entities":{"book":{"title":"Book"}}},
        "widgets":[widget("card", true), widget("count", false)]});
    std::fs::write(dir.join("app.json"), manifest.to_string()).unwrap();
    std::fs::write(
        dir.join("stats.contract"),
        STATS
            .replace("RESOURCE", resource)
            .replace("ANSWER", answer),
    )
    .unwrap();
    std::fs::write(
        dir.join("app.contract"),
        APP.replace("INJECT", inject).replace("INNER", inner),
    )
    .unwrap();
    contract::compile_path_all(&dir.join("app.contract"), false).map(|_| ())
}

const SLOT: &str = "inject\n    snapshot_configuration: Choice";

#[test]
fn placed_widgets_in_another_file_answers_the_configuration_its_widget_injects() {
    compile("kind=\"card\"", "Placed", SLOT, "").unwrap();
    compile("kind=\"card\"", "Placed", "", SLOT)
        .expect("a component the widget uses may inject it");
    for (resource, answer, inject, edit) in [
        ("kind=\"cards\"", "Placed", SLOT, "`placedWidgets(kind=\"cards\")` names no widget of app.json; name one of: card"),
        ("kind=\"count\"", "Placed", SLOT, "widget `count` has no configuration, so its placements carry nothing to read"),
        ("kind=\"ca\" + \"rd\"", "Placed", SLOT, "`placed` reads `placedWidgets` with `kind` a string literal"),
        ("kind=\"card\"", "Listed", SLOT, "`placed` answers `ready: bool` and `placements: Choice[]`, the shape widget `card` injects as its configuration"),
        ("kind=\"card\"", "Choice", SLOT, "`placed` answers `ready: bool` and `placements: Choice[]`"),
        ("kind=\"card\"", "Placed", "", "widget `card` reads its configuration in no component, so `placed` has no shape to answer; inject `snapshot_configuration: <shape>` in `Card` or one it uses"),
    ] {
        let errors = compile(resource, answer, inject, "").unwrap_err();
        let found = errors.iter().find(|e| e.id == "snapshot-placed-widgets");
        assert!(
            found.is_some_and(|e| e.message.contains(edit)
                && e.file.as_deref().is_some_and(|f| f.ends_with("stats.contract"))),
            "{resource} as {answer}: {errors:?}"
        );
    }
}

const TWO_KINDS: &str = r#"use placedWidgets from "@exact/widgets"

shape Book
  book: option<string>

shape ShelfBook
  shelf: option<string>
  book: option<string>

shape PlacedLibrary
  ready: bool
  placements: Book[]

shape PlacedProgress
  ready: bool
  placements: ShelfBook[]

component App
  FIRST
  view
    Other()

component Other
  SECOND
  view
    text "other"

shape Props
  title: string

component Library
  props
    title: string
  inject
    snapshot_configuration: Book
  view
    text "library"

component Progress
  props
    title: string
  inject
    snapshot_configuration: ShelfBook
  view
    text "progress"
"#;

/// One app reads the placements of two kinds whose configurations differ
/// (LLP 1126.005 D4), in one component or in two, and each resource holds
/// the answer for its own kind.
#[test]
fn placed_widgets_of_two_kinds_each_answer_their_own_configuration() {
    let library =
        "resource library = placedWidgets(kind=\"library\") as shape PlacedLibrary else empty()";
    let progress =
        "resource progress = placedWidgets(kind=\"progress\") as shape PlacedProgress else empty()";
    for (first, second) in [
        (format!("{library}\n  {progress}"), String::new()),
        (library.to_owned(), progress.to_owned()),
    ] {
        let dir = Scratch::new(format!(
            "exact-placed-two-{}-{}",
            std::process::id(),
            second.len()
        ));
        let package = dir.join("node_modules/@exact/widgets");
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/widgets");
        for file in ["package.json", "exact.json", "apple/exact.json"] {
            std::fs::create_dir_all(package.join(file).parent().unwrap()).unwrap();
            std::fs::copy(source.join(file), package.join(file)).unwrap();
        }
        let row = |name: &str| json!({"name":name,"title":name,"type":"entity","entity":name});
        let widget = |kind: &str, component: &str, rows: Json| {
            json!({"kind":kind,"families":["systemSmall"],
            "component":component,"props":"Props","refresh":format!("refresh{component}"),"preview":{"title":"Preview"},
            "configuration":{"title":kind,"parameters":rows}})
        };
        let manifest = json!({"name":"T","app":{"id":"com.example.t","name":"T"},
            "intents":{"entities":{"shelf":{"title":"Shelf"},"book":{"title":"Book"}}},
            "widgets":[widget("library", "Library", json!([row("book")])),
                widget("progress", "Progress", json!([row("shelf"), row("book")]))]});
        std::fs::write(dir.join("app.json"), manifest.to_string()).unwrap();
        std::fs::write(
            dir.join("app.contract"),
            TWO_KINDS
                .replace("FIRST", &first)
                .replace("SECOND", &second),
        )
        .unwrap();
        let (plan, _) = contract::compile_path_all(&dir.join("app.contract"), false)
            .unwrap_or_else(|e| panic!("{first} / {second}: {e:?}"));
        let mut r = Runner::boot(
            plan,
            boxed(NoData),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        r.set_module_target("ios");
        r.bind_extensions(&["widgets"], &[]);
        r.native_slot().host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            let asked: Json = serde_json::from_slice(&body).unwrap();
            let answer = match asked["args"]["kind"].as_str() {
                Some("library") => json!({"ready":true,"placements":[{"book":"b1"}]}),
                Some("progress") => json!({"ready":true,"placements":[{"shelf":"reading","book":null},{"shelf":"done","book":"b2"}]}),
                _ => json!({"error":"NotSupportedError"}),
            };
            reply.send(Outcome::Response(Response { status: 200, headers: Vec::new(), body: answer.to_string().into_bytes() }));
        })));
        crate::module_seam::pump(&mut r, &mut Vec::new());
        let state: Json =
            serde_json::from_str(&exact_runner::agent::handle(&r, r#"{"op":"state"}"#)).unwrap();
        let held = |name: &str| {
            let resources = state["resources"].as_object().unwrap();
            let (_, value) = (resources.iter())
                .find(|(key, _)| key.split('#').next() == Some(name))
                .unwrap();
            value.clone()
        };
        assert_eq!(
            held("library"),
            json!({"ready":true,"placements":[{"book":"b1"}]})
        );
        assert_eq!(
            held("progress"),
            json!({"ready":true,"placements":[{"shelf":"reading","book":null},{"shelf":"done","book":"b2"}]})
        );
    }
}
