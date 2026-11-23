use crate::compat::compatibility_with_trust;
use contract::Manifest;
use serde_json::{json, Value};
use std::path::Path;

const SOURCE: &str = r#"use getUserMedia, MediaStream from "@exact/media-capture"

routes nav
  home "/"

shape Started
  id: string
shape Rec
  startedAt: number

component App
  derive current = top(nav)
  state on = false
  mutation activity as shape Started
  mutation opened as shape MediaStream
  tool record writes "Record." = record
  tool silent writes "Record without an activity." = silent
  tool glance writes "Show the activity." = glance
  tool plain writes "Toggle." = plain
  tool film writes "Film." = film
  action record
    send activity = startActivity("recording", Rec(startedAt=0))
    send opened = getUserMedia(audio=true)
  action silent
    send opened = getUserMedia(audio=true)
  action glance
    send activity = startActivity("recording", Rec(startedAt=0))
  action plain
    on = not on
  action film
    send opened = getUserMedia(video=true)
  action follow(url: string)
    nav = go(nav, url)
  view
    main navigationKey=`${current.id}` navigate=follow
      text "App"
"#;

const MIC: &str = "device.microphone mic\ndevice.microphone-background bg";

fn app(name: &str) -> crate::TempDir {
    let dir = std::env::temp_dir().join(format!("exact-controls-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data/src")).unwrap();
    std::fs::write(dir.join("data/src/lib.rs"), "pub struct App;\n").unwrap();
    std::fs::write(
        dir.join("data/Cargo.toml"),
        "[package]\nname = \"app-data\"\n\n[workspace]\n",
    )
    .unwrap();
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/media-capture");
    std::fs::create_dir_all(dir.join("node_modules/@exact")).unwrap();
    #[cfg(unix)]
    use std::os::unix::fs::symlink as symlink_dir;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_dir;
    symlink_dir(
        packages.canonicalize().unwrap(),
        dir.join("node_modules/@exact/media-capture"),
    )
    .unwrap();
    std::fs::write(dir.join("app.contract"), SOURCE).unwrap();
    crate::TempDir(dir)
}

/// The bake of `controls` with `activities` declared, on each target:
/// `inputs.controls` and the id, or the refusal.
fn bake(
    dir: &Path,
    controls: Value,
    activities: bool,
    grants: &str,
    target: (&str, &str),
) -> Result<(Value, String), String> {
    baked(
        dir,
        controls,
        activities,
        grants,
        target,
        ("development", false),
    )
}

/// [`bake`] with `trust` as the build's, as a qualification build when `qualify`.
fn baked(
    dir: &Path,
    controls: Value,
    activities: bool,
    grants: &str,
    (platform, target): (&str, &str),
    (trust, qualify): (&str, bool),
) -> Result<(Value, String), String> {
    let mut manifest = json!({
        "name": "App", "icons": [{"src": "assets/a.png"}],
        "app": {"id": "com.example.app", "name": "App"},
        "host": {"ios": {"minimumOS": "18.0"}},
        "deploy": {"origin": "continuous", "bundles": "continuous"},
        "controls": controls,
    });
    if activities {
        manifest["live_activities"] = json!([{ "kind": "recording", "component": "App", "state": "Rec", "preview": { "startedAt": 0 } }]);
    }
    std::fs::write(dir.join("app.json"), manifest.to_string()).unwrap();
    let plan = contract::compile_path(&dir.join("app.contract")).map_err(|e| format!("{e:?}"))?;
    let m = Manifest::read(dir).unwrap();
    let mut c = compatibility_with_trust(dir, platform, target, &m, Some(grants), "development")?;
    // A production compat needs signing keys this fixture has none of; the trust is what the placement reads.
    c.embedded["trust"] = json!(trust);
    super::place(&mut c, &plan, &m, (platform, target), None, qualify)?;
    Ok((c.inputs["controls"].clone(), c.id))
}

const IOS: (&str, &str) = ("ios", "aarch64-apple-ios");

fn control(kind: &str, tool: &str) -> Value {
    json!({ "kind": kind, "title": "Go", "symbol": "mic.fill", "tool": tool })
}

#[test]
fn ios_offers_url_and_activity_controls_and_names_what_it_omits() {
    let dir = app("placement");
    let rows = json!([
        { "kind": "open", "title": "Open", "symbol": "house", "url": "/" },
        control("glance", "glance"),
        control("plain", "plain"),
        control("film", "film"),
        control("record", "record"),
    ]);
    let (controls, id) = bake(&dir, rows.clone(), true, MIC, IOS).unwrap();
    let row = |kind: &str| {
        let all = controls.as_array().unwrap();
        all.iter().find(|c| c["control"]["kind"] == kind).unwrap()
    };
    let placement = |kind: &str| &row(kind)["placement"];
    assert_eq!(placement("open")["offered"][0]["intent"], "open");
    assert_eq!(placement("glance")["offered"][0]["from"], "18.0");
    assert_eq!(row("glance")["conformances"], json!(["LiveActivityIntent"]));
    assert_eq!(placement("plain")["omitted"], "control-process");
    assert_eq!(placement("film")["omitted"], "control-capture");
    assert_eq!(
        placement("record")["omitted"],
        "control-recording-unqualified"
    );
    assert_eq!(
        row("record")["conformances"],
        json!(["LiveActivityIntent", "AudioRecordingIntent"])
    );
    assert_eq!(row("record")["control"]["authentication"], "required");
    assert_eq!(row("record")["digest"].as_str().unwrap().len(), 64);
    let reversed = Value::Array(rows.as_array().unwrap().iter().rev().cloned().collect());
    let (_, again) = bake(&dir, reversed, true, MIC, IOS).unwrap();
    assert_eq!(id, again, "reordering the rows keeps the id");

    for target in [
        ("ios", "aarch64-apple-tvos"),
        ("macos", "aarch64-apple-darwin"),
        ("android", "aarch64-linux-android"),
        ("linux", "x86_64-unknown-linux-gnu"),
    ] {
        let (controls, _) = bake(&dir, rows.clone(), true, MIC, target).unwrap();
        let omitted: Vec<&Value> = (0..5)
            .map(|i| &controls[i]["placement"]["omitted"])
            .collect();
        assert!(
            omitted.iter().all(|o| *o == "control-host"),
            "{target:?}: {controls}"
        );
    }
    let (web, _) = bake(&dir, rows, true, MIC, ("web", "wasm32-unknown-unknown")).unwrap();
    assert!(
        web.is_null(),
        "the web has no binary to compile a control into"
    );
}

#[test]
fn a_recording_control_needs_its_activity_and_the_background_grant() {
    let dir = app("prerequisites");
    let refused = |row: Value, activities: bool, grants: &str| {
        bake(&dir, json!([row]), activities, grants, IOS).unwrap_err()
    };
    let without = refused(control("r", "record"), false, MIC);
    assert!(
        without.starts_with("control-recording-activity"),
        "{without}"
    );
    let silent = refused(control("r", "silent"), true, MIC);
    assert!(silent.starts_with("control-recording-activity"), "{silent}");
    let foreground = refused(control("r", "record"), true, "device.microphone mic");
    assert!(foreground.starts_with("grant-needs"), "{foreground}");
    let android = ("android", "aarch64-linux-android");
    bake(&dir, json!([control("r", "silent")]), false, MIC, android)
        .expect("only iOS stops a recording without its activity");
}

#[test]
fn a_control_moves_each_native_binary_it_is_compiled_or_named_in() {
    let dir = app("identity");
    let ids = |title: &str| {
        let row = json!([{ "kind": "glance", "title": title, "symbol": "s", "tool": "glance" }]);
        [
            IOS,
            ("android", "aarch64-linux-android"),
            ("web", "wasm32-unknown-unknown"),
        ]
        .map(|t| bake(&dir, row.clone(), true, MIC, t).unwrap().1)
    };
    let (first, retitled) = (ids("Show"), ids("Glance"));
    assert_ne!(first[0], retitled[0], "iOS compiles the title");
    assert_ne!(first[1], retitled[1], "Android's build names its omission");
    assert_eq!(first[2], retitled[2], "the web has none");
}

#[test]
fn a_qualification_build_offers_what_awaits_a_device_and_production_refuses_it() {
    let dir = app("qualify");
    let rows = json!([
        control("plain", "plain"),
        control("record", "record"),
        control("film", "film")
    ]);
    let (controls, id) = baked(&dir, rows.clone(), true, MIC, IOS, ("development", true)).unwrap();
    let placement = |kind: &str| {
        let all = controls.as_array().unwrap();
        &all.iter().find(|c| c["control"]["kind"] == kind).unwrap()["placement"]
    };
    assert_eq!(
        placement("plain"),
        &json!({ "offered": [{ "from": "26.0", "intent": "tool" }], "qualifying": "control-process" })
    );
    assert_eq!(
        placement("record"),
        &json!({ "offered": [{ "from": "18.0", "intent": "recording" }], "qualifying": "control-recording-unqualified" })
    );
    assert_eq!(placement("film")["omitted"], "control-capture");
    let (_, ordinary) = bake(&dir, rows.clone(), true, MIC, IOS).unwrap();
    assert_ne!(
        id, ordinary,
        "a qualification binary is never an ordinary build's cohort"
    );
    let refused = baked(&dir, rows.clone(), true, MIC, IOS, ("production", true)).unwrap_err();
    assert!(refused.starts_with("EXACT_QUALIFY_CONTROLS"), "{refused}");
    let m = Manifest::read(&dir).unwrap();
    let plan = contract::compile_path(&dir.join("app.contract")).unwrap();
    let mut c = compatibility_with_trust(&dir, "ios", IOS.1, &m, Some(MIC), "development").unwrap();
    super::place(&mut c, &plan, &m, IOS, Some("controls,intents"), true).unwrap();
    let record = c.inputs["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["control"]["kind"] == "record")
        .unwrap()
        .clone();
    assert_eq!(
        record["placement"]["omitted"], "control-recording-unqualified",
        "a build without its activity offers no recording"
    );
    c.embedded["trust"] = json!("production");
    let web = super::place(
        &mut c,
        &plan,
        &m,
        ("web", "wasm32-unknown-unknown"),
        None,
        true,
    )
    .unwrap_err();
    assert!(
        web.starts_with("EXACT_QUALIFY_CONTROLS"),
        "refused where no control is placed too: {web}"
    );
    let missing = baked(
        &dir,
        json!([control("r", "silent")]),
        true,
        MIC,
        IOS,
        ("development", true),
    )
    .unwrap_err();
    assert!(
        missing.starts_with("control-recording-activity"),
        "the prerequisites stand: {missing}"
    );
}
