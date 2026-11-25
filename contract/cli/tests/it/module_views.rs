//! A package carries a view, and its implementation hears whether the view
//! is displayed and who made it so (LLP 1133.001 D1, LLP 1133.008 D8 items
//! 1 and 2), through `@fixture/swatch`, a package written beside the app as
//! an installed one, whose implementation the test plays as the host's
//! native slot.

use crate::source::{boxed, NoData, Source};
use exact_runner::{CallOrigin, Reply, Runner};
use serde_json::{json, Value as Json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const INTERFACE: &str = r#"{
  "kind": "interface",
  "name": "@fixture/swatch",
  "version": "1.0.0",
  "views": {
    "swatch-box": { "props": { "tint": { "type": "option<string>", "binds": true }, "caption": "option<string>" },
      "events": ["message"], "displayed": true }
  },
  "names": []
}"#;

const APP: &str = r#"component App
  state shown = false
  state hidden = false
  state tint = "red"
  action show
    shown = true
  action unshow
    shown = false
  action hide
    hidden = true
  action reveal
    hidden = false
  action recolor
    tint = "blue"
  view
    column
      if shown
        swatch-box tint=tint caption="a" display=(hidden ? "none" : "block") testId="box"
"#;

struct App(PathBuf);
impl App {
    /// The app, depending on `@fixture/swatch` and on each of `others`
    /// (a package name and its `exact.json`).
    fn new(name: &str, app: &str, others: &[(&str, &str)]) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("exact-views-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let mut deps = serde_json::Map::new();
        for (package, declaration) in [("@fixture/swatch", INTERFACE)].iter().chain(others) {
            let at = path.join("node_modules").join(package);
            std::fs::create_dir_all(&at).unwrap();
            let manifest =
                json!({ "name": package, "version": "1.0.0", "exports": { ".": "./exact.json" } });
            std::fs::write(at.join("package.json"), manifest.to_string()).unwrap();
            std::fs::write(at.join("exact.json"), declaration).unwrap();
            deps.insert(package.to_string(), json!("1.0.0"));
        }
        let manifest = json!({ "name": "app", "dependencies": deps });
        std::fs::write(path.join("package.json"), manifest.to_string()).unwrap();
        std::fs::write(path.join("app.contract"), app).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_package_gpu_surface_checks_its_positional_arity() {
    let declaration = r#"{"kind":"interface","name":"@fixture/sky","version":"1.0.0",
        "gpu":{"crate":"rust","exactGpu":"0.4","surfaces":{"sky":{"arity":1,"factory":"sky::surface"}}}}"#;
    let app = App::new(
        "gpu-arity",
        "component App\n  view\n    canvas surface=sky(1, 2)\n",
        &[("@fixture/sky", declaration)],
    );
    let errors = contract::compile_path(&app.0.join("app.contract")).unwrap_err();
    assert!(errors.message.contains("sky"), "{}", errors.message);
    assert_eq!(errors.id, "analyze-surface-arity");
    for surface in ["sky()", "sky(1)", "sky(color=1)"] {
        std::fs::write(
            app.0.join("app.contract"),
            format!("component App\n  view\n    canvas surface={surface}\n"),
        )
        .unwrap();
        contract::compile_path(&app.0.join("app.contract")).unwrap();
    }
    std::fs::write(app.0.join("app.contract"), "component App\n  view\n    text \"no canvas\"\ncomponent Unused\n  view\n    canvas surface=sky(1, 2)\n").unwrap();
    contract::compile_path(&app.0.join("app.contract")).unwrap();
    std::fs::write(
        app.0.join("app.json"),
        r#"{"app":{"id":"com.exact.gpu-arity","name":"GPU arity"},"gpu":{"modules":{"other":["sky"]}}}"#,
    )
    .unwrap();
    let error = contract::compile_path(&app.0.join("app.contract")).unwrap_err();
    assert!(
        error
            .message
            .contains("both gpu.modules.other and @fixture/sky"),
        "{}",
        error.message
    );
}

#[test]
fn a_package_gpu_declaration_checks_device_requirements_at_build() {
    let app = App::new(
        "gpu-requirements",
        "component App\n  view\n    canvas surface=sky()\n",
        &[(
            "@fixture/sky",
            r#"{"kind":"interface","name":"@fixture/sky","version":"1.0.0"}"#,
        )],
    );
    let path = app.0.join("node_modules/@fixture/sky/exact.json");
    let gpu = json!({"crate":"rust", "exactGpu":"0.4", "surfaces":{"sky":{"arity":0,"factory":"sky::surface"}},
        "limits":{"required":{"maxStorageBufferBindingSize":268435456},"optional":{"maxBufferSize":1073741824}},
        "features":{"required":["subgroups", "primitive-index"],"optional":["float32-filterable"]}});
    let write = |gpu: &Json| {
        std::fs::write(
            &path,
            json!({"kind":"interface","name":"@fixture/sky","version":"1.0.0","gpu":gpu})
                .to_string(),
        )
        .unwrap()
    };
    write(&gpu);
    contract::compile_path(&app.0.join("app.contract")).unwrap();
    for (key, value, expected) in [
        (
            "limits",
            json!({"required":{"max_buffer_size":512}}),
            "max_buffer_size",
        ),
        (
            "limits",
            json!({"required":{"maxBufferSize":-1}}),
            "maxBufferSize",
        ),
        (
            "limits",
            json!({"optional":{"maxBindGroups":1.5}}),
            "maxBindGroups",
        ),
        (
            "limits",
            json!({"required":{"maxBufferSize":9007199254740992_u64}}),
            "integer from 0 to 9007199254740991",
        ),
        (
            "limits",
            json!({"required":{"maxBindGroups":4294967296_u64}}),
            "integer from 0 to 4294967295",
        ),
        ("limits", Json::Null, "object"),
        ("limits", json!({"required":[]}), "object"),
        ("features", json!({"optional":[3]}), "feature names"),
        (
            "limits",
            json!({"required":{"minStorageBufferOffsetAlignment":3}}),
            "power of two",
        ),
        (
            "limits",
            json!({"required":{"maxBindGroups":4},"optional":{"maxBindGroups":8}}),
            "both required and optional",
        ),
        (
            "features",
            json!({"required":["FLOAT32_FILTERABLE"]}),
            "FLOAT32_FILTERABLE",
        ),
        (
            "features",
            json!({"required":["subgroups"],"optional":["subgroups"]}),
            "more than once",
        ),
        ("features", json!({"optional":"subgroups"}), "array"),
        (
            "limits",
            json!({"require":{"maxBufferSize":512}}),
            "expected required or optional",
        ),
    ] {
        let mut bad = gpu.clone();
        bad[key] = value;
        write(&bad);
        let error = contract::compile_path(&app.0.join("app.contract")).unwrap_err();
        assert_eq!(error.id, "bake-gpu-declaration");
        assert!(error.message.contains(expected), "{}", error.message);
    }
}

#[test]
fn a_package_view_in_an_uninstantiated_component_is_not_in_the_plan() {
    let app = App::new("unused", "component App\n  view\n    text \"plain\"\ncomponent Tile\n  view\n    swatch-box tint=\"red\"\n", &[]);
    let plan = contract::compile_path(&app.0.join("app.contract")).unwrap();
    assert!(plan.views.is_empty());
}

#[test]
fn a_package_with_no_module_names_accepts_its_views_agent_words() {
    let interface = r#"{
      "kind": "interface", "name": "@fixture/words", "version": "1.0.0",
      "agent": { "heard": "string", "levels": "number[]", "enabled": "bool" },
      "views": { "words-panel": {"displayed": true} }, "names": []
    }"#;
    let app = App::new(
        "words-without-sources",
        "component App\n  view\n    words-panel\n",
        &[("@fixture/words", interface)],
    );
    let (mut r, told) = boot(&app);
    assert!(r.plan().modules.is_empty());
    assert_eq!(r.plan().views.len(), 1);
    let prefer = |r: &mut Runner<Source>, values: Json| {
        let request = json!({ "op": "prefer", "extension": values });
        r.extension_agent(&request.to_string()).unwrap().1
    };
    prefer(&mut r, json!({ "words.heard": "hello" })).unwrap();
    r.bind_extensions(&[], &["words"]);
    assert_eq!(
        told.lock().unwrap().first().unwrap()["preferred"]["heard"],
        "hello"
    );
    prefer(
        &mut r,
        json!({ "words.heard": "again", "words.levels": [1, 2], "words.enabled": true }),
    )
    .unwrap();
    assert_eq!(
        told.lock().unwrap().last().unwrap()["preferred"],
        json!({
            "heard": "again", "levels": [1, 2], "enabled": true
        })
    );
    for (key, value, ty) in [
        ("words.heard", json!(7), "string"),
        ("words.levels", json!([1, "two"]), "number[]"),
        ("words.enabled", json!("true"), "bool"),
    ] {
        let error = prefer(&mut r, json!({ key: value })).unwrap_err();
        assert!(error.contains(&format!("`{key}` is a {ty}")), "{error}");
    }
    for key in ["words.unknown", "unknown.heard"] {
        assert!(prefer(&mut r, json!({ key: "hello" }))
            .unwrap_err()
            .contains("no source"));
    }
    prefer(&mut r, json!({ "words.heard": "clear" })).unwrap();
    assert!(told.lock().unwrap().last().unwrap()["preferred"]
        .get("heard")
        .is_none());
    let state: Json = serde_json::from_str(&r.extensions_json().unwrap()).unwrap();
    assert_eq!(
        state["words"]["preferred"],
        json!({ "levels": [1, 2], "enabled": true })
    );
    told.lock().unwrap().clear();
    r.bind_extensions(&["words"], &["words"]);
    prefer(&mut r, json!({ "words.heard": "real" })).unwrap();
    assert!(told
        .lock()
        .unwrap()
        .iter()
        .all(|call| call.get("preferred").is_none()));
    std::fs::write(
        app.0.join("app.contract"),
        "component App\n  view\n    text \"plain\"\ncomponent Unused\n  view\n    words-panel\n",
    )
    .unwrap();
    let (mut r, _) = boot(&app);
    assert!(prefer(&mut r, json!({ "words.heard": "hello" }))
        .unwrap_err()
        .contains("no source"));
}

#[test]
fn a_view_only_map_accepts_its_declared_tile_preference() {
    let app = App::new(
        "map-tiles",
        "component App\n  view\n    exact-map latitude=0 longitude=0\n",
        &[(
            "@exact/maps",
            include_str!("../../../../packages/maps/exact.json"),
        )],
    );
    let (mut r, told) = boot(&app);
    assert!(r.plan().modules.is_empty());
    r.bind_extensions(&[], &["maps"]);
    let prefer = |r: &mut Runner<Source>, value: Json| {
        let request = json!({ "op": "prefer", "extension": { "maps.tiles": value } });
        r.extension_agent(&request.to_string()).unwrap().1
    };
    prefer(&mut r, json!("live")).unwrap();
    assert_eq!(
        told.lock().unwrap().last().unwrap()["preferred"],
        json!({ "tiles": "live" })
    );
    assert!(prefer(&mut r, json!(true))
        .unwrap_err()
        .contains("`maps.tiles` is a string"));
    prefer(&mut r, json!("clear")).unwrap();
    assert_eq!(told.lock().unwrap().last().unwrap()["preferred"], json!({}));
}

#[test]
fn a_view_only_package_keeps_agent_word_types_and_source_precedence() {
    let interface = r#"{
      "kind": "interface", "name": "@fixture/words", "version": "1.0.0",
      "substitute": "implementation",
      "agent": { "heard": "string", "levels": "number[]", "enabled": "bool", "status": "string" },
      "views": { "words-panel": {} },
      "names": [
        { "source": "status", "answers": "bool" },
        { "source": "dataStatus", "answers": "bool", "contract": false }
      ]
    }"#;
    let app = App::new(
        "agent-words",
        "component App\n  view\n    words-panel\n",
        &[("@fixture/words", interface)],
    );
    let (mut r, told) = boot(&app);
    r.bind_extensions(&[], &["words"]);
    for (key, value, ty) in [
        ("words.heard", json!(7), "string"),
        ("words.levels", json!([1, "two"]), "number[]"),
        ("words.enabled", json!("true"), "bool"),
    ] {
        let request = json!({ "op": "prefer", "extension": { key: value } });
        let error = r
            .extension_agent(&request.to_string())
            .unwrap()
            .1
            .unwrap_err();
        assert!(error.contains(&format!("`{key}` is a {ty}")), "{error}");
    }
    let request = json!({ "op": "prefer", "extension": {
        "words.heard": "a phrase", "words.levels": [1, 2], "words.enabled": true, "words.status": true
    } });
    r.extension_agent(&request.to_string()).unwrap().1.unwrap();
    let calls = told.lock().unwrap();
    let preferred = calls
        .iter()
        .rev()
        .find(|c| c["extension"] == "words" && c["preferred"].is_object())
        .unwrap();
    assert_eq!(
        preferred["preferred"],
        json!({ "heard": "a phrase", "levels": [1, 2], "enabled": true, "status": true })
    );
    drop(calls);
    let request = json!({ "op": "prefer", "extension": { "words.status": "a string" } });
    assert!(
        r.extension_agent(&request.to_string()).unwrap().1.is_err(),
        "the source's bool type wins over the agent word's string"
    );
    let request = json!({ "op": "prefer", "extension": { "words.heard": "clear" } });
    let reply = r.extension_agent(&request.to_string()).unwrap().1.unwrap();
    let reply: Json = serde_json::from_str(&reply).unwrap();
    assert!(reply["extension"].get("words.heard").is_none());

    std::fs::write(
        app.0.join("app.contract"),
        "component App\n  view\n    text \"plain\"\ncomponent Unused\n  view\n    words-panel\n",
    )
    .unwrap();
    let plan = contract::compile_path(&app.0.join("app.contract")).unwrap();
    assert!(plan.views.is_empty());
    assert!(
        plan.modules.is_empty(),
        "an unused view's data-only names stay unused too"
    );
}

type Told = Arc<Mutex<Vec<Json>>>;

/// A runner over the app, every body its native slot is given kept.
fn boot(app: &App) -> (Runner<Source>, Told) {
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let plan = exact_plan::Plan::decode(&plan.encode()).unwrap();
    let r = Runner::boot(
        plan,
        boxed(NoData),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let told: Told = Arc::default();
    let seen = told.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap());
            drop(reply);
        })));
    (r, told)
}

/// What the implementation was told since the last call, each report as
/// `displayed <shown> <origin>` or `bound <prop> <origin>`.
fn reports(told: &Told) -> Vec<String> {
    (told.lock().unwrap().drain(..))
        .filter(|b| b["view"].is_string())
        .map(|b| {
            assert_eq!(b["extension"], "swatch");
            assert_eq!(b["tag"], "swatch-box");
            match &b["displayed"] {
                Json::Bool(shown) => format!("displayed {shown} {}", b["origin"].as_str().unwrap()),
                _ => format!(
                    "bound {} {}",
                    b["bound"].as_str().unwrap(),
                    b["origin"].as_str().unwrap()
                ),
            }
        })
        .collect()
}

fn view_id(r: &Runner<Source>) -> u32 {
    let state = r.extensions_json().unwrap();
    let state: Json = serde_json::from_str(&state).unwrap();
    state["swatch"]["views"][0]["view"].as_u64().unwrap() as u32
}

#[test]
fn device_bindings_follow_the_runner_node_through_both_orders_and_removal() {
    let app = App::new("device-bindings", APP, &[]);
    for (shown, bound, expected) in [
        (CallOrigin::Tool, CallOrigin::App, "tool"),
        (CallOrigin::App, CallOrigin::Tool, "tool"),
        (CallOrigin::Action, CallOrigin::App, "action"),
        (CallOrigin::App, CallOrigin::Wake, "wake"),
    ] {
        let (mut runner, _) = boot(&app);
        runner.bind_extensions(&["swatch"], &[]);
        let snapshots: Told = Arc::default();
        let seen = snapshots.clone();
        runner.observe_view_bindings(
            &[("swatch-box", "tint")],
            Some(Arc::new(move |body| {
                seen.lock()
                    .unwrap()
                    .push(serde_json::from_str(body).unwrap());
            })),
        );
        runner.act_from(shown, "show", Vec::new()).unwrap();
        let id = view_id(&runner).to_string();
        runner.act_from(bound, "recolor", Vec::new()).unwrap();
        let last = || snapshots.lock().unwrap().last().unwrap().clone();
        assert_eq!(
            last()["swatch-box"][&id]["tint"],
            json!({"value":"blue","origin":expected})
        );
        runner.act("unshow", Vec::new()).unwrap();
        assert_eq!(last(), serde_json::json!({}));
        runner.act("show", Vec::new()).unwrap();
        assert_eq!(
            last()["swatch-box"][view_id(&runner).to_string()]["tint"]["origin"],
            "app"
        );
        runner.bind_extensions(&[], &[]);
        assert_eq!(last(), serde_json::json!({}));
    }
}

#[test]
fn another_packages_string_binding_does_not_notify_the_track_observer() {
    let app = App::new("track-filter", APP, &[]);
    let (mut runner, _) = boot(&app);
    runner.bind_extensions(&["swatch"], &[]);
    let snapshots: Told = Arc::default();
    let seen = snapshots.clone();
    runner.observe_view_bindings(
        &[("audio-waveform", "track")],
        Some(Arc::new(move |body| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_str(body).unwrap());
        })),
    );
    runner.act("show", Vec::new()).unwrap();
    runner.act("recolor", Vec::new()).unwrap();
    assert_eq!(*snapshots.lock().unwrap(), [json!({})]);
}

#[test]
fn a_hidden_binding_keeps_its_origin_when_the_node_is_revealed() {
    let app = App::new("binding-before-show", APP, &[]);
    let (mut runner, _) = boot(&app);
    runner.bind_extensions(&["swatch"], &[]);
    let snapshots: Told = Arc::default();
    let seen = snapshots.clone();
    runner.observe_view_bindings(
        &[("swatch-box", "tint")],
        Some(Arc::new(move |body| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_str(body).unwrap());
        })),
    );
    runner.act("show", Vec::new()).unwrap();
    let id = view_id(&runner).to_string();
    runner.act("hide", Vec::new()).unwrap();
    runner
        .act_from(CallOrigin::Tool, "recolor", Vec::new())
        .unwrap();
    assert_eq!(snapshots.lock().unwrap().last(), Some(&json!({})));
    runner.act("reveal", Vec::new()).unwrap();
    assert_eq!(
        snapshots.lock().unwrap().last().unwrap()["swatch-box"][&id]["tint"],
        json!({"value":"blue", "origin":"tool"})
    );
}

/// The plan carries the view's row: its tag, the extension that makes it,
/// that it hears its displayed state, and its one binding prop.
#[test]
fn the_plan_names_a_package_s_view_and_its_extension() {
    let app = App::new("plan", APP, &[]);
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let row = plan.view_named("swatch-box").expect("a views row");
    assert_eq!(plan.str(row.extension), "swatch");
    assert!(row.displayed);
    assert_eq!(plan.str(row.binds), "tint");
}

/// A view the app's own commit shows is reported displayed with the app's
/// origin; a tool's commit that rebinds and hides it, with the tool's; one
/// that leaves the tree while shown is reported hidden.
#[test]
fn a_view_hears_each_change_with_its_commit_s_origin() {
    let app = App::new("origin", APP, &[]);
    let (mut r, told) = boot(&app);
    r.bind_extensions(&["swatch"], &[]);
    assert!(reports(&told).is_empty(), "nothing is placed yet");

    r.act("show", Vec::new()).unwrap();
    assert_eq!(reports(&told), ["bound tint app", "displayed true app"]);
    let id = view_id(&r);
    assert_eq!(r.view_origin(id, None), Some(CallOrigin::App));

    r.act_from(CallOrigin::Tool, "recolor", Vec::new()).unwrap();
    assert_eq!(reports(&told), ["bound tint tool"]);
    assert_eq!(r.view_origin(id, Some("tint")), Some(CallOrigin::Tool));
    assert_eq!(
        r.view_origin(id, None),
        Some(CallOrigin::App),
        "still shown by the app's commit"
    );

    r.act_from(CallOrigin::Tool, "hide", Vec::new()).unwrap();
    assert_eq!(reports(&told), ["displayed false tool"]);
    assert_eq!(r.view_origin(id, None), None);

    r.act("unshow", Vec::new()).unwrap();
    assert!(
        reports(&told).is_empty(),
        "a hidden view leaves without a report"
    );
}

#[test]
fn clearing_and_rebinding_the_same_value_reports_the_new_commits_origin() {
    let app = App::new(
        "unbind",
        r#"component App
  state tint: option<string> = some("red")
  state hidden = false
  action hide
    hidden = true
  action clear
    tint = none
    hidden = false
  action restore
    tint = some("red")
  view
    swatch-box tint=tint display=(hidden ? "none" : "block")
"#,
        &[],
    );
    let (mut r, told) = boot(&app);
    r.bind_extensions(&["swatch"], &[]);
    assert_eq!(reports(&told), ["displayed true app", "bound tint app"]);
    let id = view_id(&r);
    r.act("hide", Vec::new()).unwrap();
    assert_eq!(reports(&told), ["displayed false app"]);
    r.act("clear", Vec::new()).unwrap();
    let changes: Vec<_> = told
        .lock()
        .unwrap()
        .drain(..)
        .filter(|r| r.get("view").is_some())
        .collect();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["bound"], "tint");
    assert_eq!(changes[0].get("value"), Some(&Json::Null));
    assert_eq!(changes[1]["displayed"], true);
    assert_eq!(r.view_origin(id, Some("tint")), None);
    r.act_from(CallOrigin::Action, "restore", Vec::new())
        .unwrap();
    assert_eq!(r.view_origin(id, Some("tint")), Some(CallOrigin::Action));
    assert!(told
        .lock()
        .unwrap()
        .drain(..)
        .any(|r| { r["bound"] == "tint" && r["value"] == "red" && r["origin"] == "action" }));
    r.act("restore", Vec::new()).unwrap();
    assert!(reports(&told).is_empty());
    assert_eq!(r.view_origin(id, Some("tint")), Some(CallOrigin::Action));
}

/// A view a tool's commit shows is reported with the tool's origin, which
/// the runner keeps: a capture start that follows is admitted with it.
#[test]
fn a_view_a_tool_shows_is_reported_with_the_tool_s_origin() {
    let app = App::new("tool", APP, &[]);
    let (mut r, told) = boot(&app);
    r.bind_extensions(&[], &["swatch"]);
    r.act_from(CallOrigin::Tool, "show", Vec::new()).unwrap();
    assert_eq!(reports(&told), ["bound tint tool", "displayed true tool"]);
    assert_eq!(r.view_origin(view_id(&r), None), Some(CallOrigin::Tool));
    r.act("unshow", Vec::new()).unwrap();
    assert_eq!(reports(&told), ["displayed false app"]);
    let state = r.extensions_json().unwrap();
    assert!(state.contains(r#""views":[]"#), "{state}");
}

/// The host binds its implementations after first pixel: a view shown
/// before then is told when its extension is bound, with the kept origin.
#[test]
fn an_extension_bound_late_is_told_its_views_with_their_kept_origins() {
    let app = App::new("late", APP, &[]);
    let (mut r, told) = boot(&app);
    r.act_from(CallOrigin::Tool, "show", Vec::new()).unwrap();
    r.act("recolor", Vec::new()).unwrap();
    assert!(reports(&told).is_empty(), "no implementation is bound");
    r.bind_extensions(&["swatch"], &[]);
    assert_eq!(reports(&told), ["displayed true tool", "bound tint app"]);
}

/// The bake finds a tag in the packages the app depends on: one declared
/// by two packages, or by a package and `app.json` `modules`, is refused
/// naming both; one declared by none is `bake-unknown-module`.
#[test]
fn the_bake_refuses_a_tag_declared_twice_or_by_no_one() {
    let refused = |app: &App| {
        let errors = contract::compile_path_all(&app.0.join("app.contract"), false)
            .err()
            .expect("refused");
        (errors[0].id.clone(), errors[0].message.clone())
    };
    let twice = r#"{ "kind": "interface", "name": "@other/box", "version": "1.0.0",
      "views": { "swatch-box": { "props": {} } }, "names": [] }"#;
    let app = App::new("twice", APP, &[("@other/box", twice)]);
    let (id, message) = refused(&app);
    assert_eq!(id, "bake-view-twice");
    assert!(
        message.contains("@fixture/swatch") && message.contains("@other/box"),
        "{message}"
    );

    let app = App::new("modules", APP, &[]);
    let manifest =
        r#"{ "app": { "id": "com.exact.views", "name": "Views" }, "modules": ["swatch-box"] }"#;
    std::fs::write(app.0.join("app.json"), manifest).unwrap();
    let (id, message) = refused(&app);
    assert_eq!(id, "bake-view-twice");
    assert!(message.contains("app.json"), "{message}");

    let typo = APP.replace("swatch-box tint", "swatch-bx tint");
    let app = App::new("unknown", &typo, &[]);
    let (id, message) = refused(&app);
    assert_eq!(id, "bake-unknown-module");
    assert!(message.contains("did you mean `swatch-box`"), "{message}");
}
