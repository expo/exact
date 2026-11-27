//! LLP 1126.005 D4: `placedWidgets` from the repository's `@exact/widgets`
//! is checked against `app.json` wherever the program reads it, here from a
//! second source file of the app.

use crate::scratch::Scratch;
use serde_json::json;
use std::path::Path;

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
