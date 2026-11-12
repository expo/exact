//! LLP 1133.021 §9 items 1 and 2, through `@fixture/pinboard`, a package
//! written beside the app as an installed one: a view's record and list
//! props reach the implementation as JSON under their names; a prop's type
//! is held to the declaration (`contract-view-prop`); and an event the view
//! declares beyond the nine runs its handler with the typed record the
//! implementation sent.

use exact_kernel::{Dimension, ObjectFit, PropId, StyleId};
use exact_runner::{Event, Runner, Value};
use serde_json::json;
use std::path::PathBuf;

const INTERFACE: &str = r#"{
  "kind": "interface",
  "name": "@fixture/pinboard",
  "version": "1.0.0",
  "shapes": {
    "Pin": { "id": "string", "latitude": "number", "longitude": "number", "title": "option<string>" },
    "Camera": { "latitude": "number", "longitude": "number", "zoom": "number" }
  },
  "views": {
    "pin-board": {
      "props": { "latitude": "number", "zoomLevel": "option<number>", "pins": "option<Pin[]>", "focus": "option<Pin>", "object-fit": "option<string>", "fontSize": "option<number>" },
      "events": ["change", { "event": "moveend", "args": ["Camera"] }]
    }
  },
  "names": []
}"#;

const APP: &str = r#"use Pin, Camera from "@fixture/pinboard"

component App
  state zoom = 11
  state moved = ""
  state picked = ""
  derive pins = [Pin(id="ferry", latitude=37.7955, longitude=-122.3937, title=some("Ferry \"Building\"")), Pin(id="pier", latitude=37.8087, longitude=-122.4098, title=none)]
  action ended(c: Camera)
    moved = `${c.latitude} ${c.longitude} ${c.zoom}`
    zoom = c.zoom
  action pick(id: string)
    picked = id
  view
    column
      pin-board testId="board" latitude=37.8 zoomLevel=zoom pins=pins moveend=ended change=pick
"#;

struct App(PathBuf);
impl App {
    fn new(name: &str, app: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("exact-typed-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let at = path.join("node_modules/@fixture/pinboard");
        std::fs::create_dir_all(&at).unwrap();
        let manifest = json!({ "name": "@fixture/pinboard", "version": "1.0.0", "exports": { ".": "./exact.json" } });
        std::fs::write(at.join("package.json"), manifest.to_string()).unwrap();
        std::fs::write(at.join("exact.json"), INTERFACE).unwrap();
        let manifest = json!({ "name": "app", "dependencies": { "@fixture/pinboard": "1.0.0" } });
        std::fs::write(path.join("package.json"), manifest.to_string()).unwrap();
        std::fs::write(path.join("app.contract"), app).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn compile(&self) -> Result<exact_plan::Plan, Vec<contract::CompileError>> {
        contract::compile_path_all(&self.0.join("app.contract"), false).map(|(plan, _)| plan)
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct NoData;
impl exact_runner::DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
}

fn boot(app: &App) -> Runner<NoData> {
    Runner::boot(
        app.compile().unwrap_or_else(|e| panic!("{e:?}")),
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn props(r: &Runner<NoData>) -> serde_json::Value {
    let key = r.kernel().find_by_test_id("board")[0];
    let node = r.kernel().node_by_key(key).unwrap();
    serde_json::from_str(node.props.str(PropId::NativeViewProps).unwrap()).unwrap()
}

#[test]
fn a_record_list_prop_reaches_the_view_as_its_json() {
    let app = App::new("props", APP);
    let r = boot(&app);
    let props = props(&r);
    assert_eq!(props["latitude"], "37.8");
    assert_eq!(props["zoomLevel"], "11");
    let pins: serde_json::Value = serde_json::from_str(props["pins"].as_str().unwrap()).unwrap();
    assert_eq!(
        pins,
        json!([
            { "id": "ferry", "latitude": 37.7955, "longitude": -122.3937, "title": "Ferry \"Building\"" },
            { "id": "pier", "latitude": 37.8087, "longitude": -122.4098 }
        ])
    );
}

/// A non-layout prop may share a known attribute's name, as `exact-camera`'s
/// `object-fit` does; layout attributes still bind to the box.
#[test]
fn a_declared_prop_the_table_also_names_reaches_the_view() {
    let app = App::new(
        "table",
        &APP.replace(
            "  state picked = \"\"\n",
            "  state picked = \"\"\n  state fit = \"cover\"\n",
        )
        .replace(
            "testId=\"board\"",
            "testId=\"board\" object-fit=fit width=200 height=200",
        ),
    );
    let r = boot(&app);
    let props = props(&r);
    assert_eq!(props["object-fit"], "cover", "{props}");
    assert!(props.get("width").is_none(), "{props}");
    assert!(props.get("height").is_none(), "{props}");
    let key = r.kernel().find_by_test_id("board")[0];
    let style = &r.kernel().node_by_key(key).unwrap().style;
    assert!(style.mask.has(StyleId::Width));
    assert_eq!(style.width, Dimension::Points(200.0));
    assert_eq!(style.height, Dimension::Points(200.0));
}

#[test]
fn a_package_cannot_declare_a_layout_attribute_as_a_view_prop() {
    for name in ["width", "min-width", "padding", "flex", "border"] {
        for attr in ["", " width=200", " class=Wide"] {
            let app = App::new(
                "layout",
                &APP.replace(
                    "\ncomponent App",
                    "\nstyle Wide\n  width=\"100%\"\n\ncomponent App",
                )
                .replace("testId=\"board\"", &format!("testId=\"board\"{attr}")),
            );
            let mut declaration: serde_json::Value = serde_json::from_str(INTERFACE).unwrap();
            declaration["views"]["pin-board"]["props"][name] =
                json!({ "type": "option<number>", "binds": true });
            std::fs::write(
                app.0.join("node_modules/@fixture/pinboard/exact.json"),
                declaration.to_string(),
            )
            .unwrap();
            let errors = app.compile().expect_err("a layout prop must be refused");
            assert!(
                errors.iter().any(|e| e.id == "bake-view-prop-layout"
                    && e.message.contains("@fixture/pinboard")
                    && e.message.contains("`pin-board`")
                    && e.message.contains(&format!("`{name}`"))),
                "{name}{attr}: {errors:?}"
            );
        }
    }
}

#[test]
fn a_class_styles_the_box_without_setting_a_declared_view_prop() {
    for (attr, expected) in [("", None), (" object-fit=\"cover\"", Some("cover"))] {
        let app = App::new(
            "classed",
            &APP.replace(
                "\ncomponent App",
                "\nstyle Preview\n  width=\"100%\"\n  object-fit=\"contain\"\n\ncomponent App",
            )
            .replace(
                "testId=\"board\"",
                &format!("testId=\"board\" class=Preview{attr}"),
            ),
        );
        let r = boot(&app);
        assert_eq!(props(&r)["object-fit"].as_str(), expected);
        let key = r.kernel().find_by_test_id("board")[0];
        let style = &r.kernel().node_by_key(key).unwrap().style;
        assert!(style.mask.has(StyleId::Width));
        assert_eq!(style.width, Dimension::Percent(100.0));
        assert!(style.mask.has(StyleId::ObjectFit));
        assert_eq!(style.object_fit, ObjectFit::Contain);
    }
}

#[test]
fn the_camera_declaration_still_accepts_object_fit() {
    let app = App::new(
        "camera",
        "component App\n  state fit = \"cover\"\n  view\n    exact-camera testId=\"board\" object-fit=fit width=200\n",
    );
    std::fs::write(
        app.0.join("node_modules/@fixture/pinboard/exact.json"),
        include_str!("../../../../packages/camera/exact.json"),
    )
    .unwrap();
    let r = boot(&app);
    assert_eq!(props(&r)["object-fit"], "cover");
    let key = r.kernel().find_by_test_id("board")[0];
    assert_eq!(
        r.kernel().node_by_key(key).unwrap().style.width,
        Dimension::Points(200.0)
    );
}

/// A renamed spelling is refused with its CSS name even where the view
/// declares it: `fontSize` is `font-size` whatever a package says.
#[test]
fn a_declared_prop_under_a_renamed_spelling_is_still_refused() {
    let app = App::new(
        "renamed",
        &APP.replace("testId=\"board\"", "testId=\"board\" fontSize=16"),
    );
    let refused = app.compile().unwrap_err();
    assert!(
        refused.iter().any(|e| e.message.contains("`font-size`")),
        "{refused:?}"
    );
}

#[test]
fn a_prop_is_held_to_its_declared_type() {
    let wrong = App::new(
        "wrong",
        &APP.replace("latitude=37.8 zoomLevel", "latitude=\"north\" zoomLevel"),
    );
    let refused = wrong.compile().unwrap_err();
    assert!(
        refused
            .iter()
            .any(|e| e.id == "contract-view-prop" && e.message.contains("`latitude`")),
        "{refused:?}"
    );
    let record = App::new("record", &APP.replace("pins=pins", "pins=first(pins)"));
    let refused = record.compile().unwrap_err();
    assert!(
        refused
            .iter()
            .any(|e| e.id == "contract-view-prop" && e.message.contains("`pins`")),
        "{refused:?}"
    );
}

#[test]
fn a_declared_event_runs_its_handler_with_the_record_sent() {
    let app = App::new("event", APP);
    let mut r = boot(&app);
    let key = r.kernel().find_by_test_id("board")[0];
    let id = r.kernel().node_by_key(key).unwrap().id;
    let sent = json!({ "exact:event": "moveend", "args": [{ "latitude": 37.79, "longitude": -122.4, "zoom": 13 }] });
    r.dispatch(id, Event::Message(sent.to_string())).unwrap();
    assert_eq!(r.slot("moved"), Some(&Value::str("37.79 -122.4 13")));
    assert_eq!(props(&r)["zoomLevel"], "13");
    // An event the view does not declare runs nothing.
    let other = json!({ "exact:event": "zoomend", "args": [] });
    r.dispatch(id, Event::Message(other.to_string())).unwrap();
    assert_eq!(r.slot("moved"), Some(&Value::str("37.79 -122.4 13")));
    // A handler whose action does not take the event's record is refused.
    let wrong = App::new(
        "arity",
        &APP.replace("action ended(c: Camera)", "action ended(c: Pin)")
            .replace(
                "    moved = `${c.latitude} ${c.longitude} ${c.zoom}`\n    zoom = c.zoom\n",
                "    moved = c.id\n",
            ),
    );
    let refused = wrong.compile().unwrap_err();
    assert!(
        refused.iter().any(|e| e.id == "lower-handler-type"),
        "{refused:?}"
    );
}
