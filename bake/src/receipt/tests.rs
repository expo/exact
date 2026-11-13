use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn package_extensions_follow_placed_library_components() {
    let dir = std::env::temp_dir().join(format!("exact-library-extensions-{}", std::process::id()));
    let put = |path: &str, text: &str| {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    put(
        "package.json",
        r#"{"dependencies":{"@fixture/swatch":"1.0.0","@repro/lib":"1.0.0","@repro/outer":"1.0.0"}}"#,
    );
    put(
        "node_modules/@fixture/swatch/package.json",
        include_str!("../../../packages/fixtures/swatch/package.json"),
    );
    put(
        "node_modules/@fixture/swatch/exact.json",
        include_str!("../../../packages/fixtures/swatch/exact.json"),
    );
    for (target, declaration) in [
        (
            "apple",
            include_str!("../../../packages/fixtures/swatch/apple/exact.json"),
        ),
        (
            "android",
            include_str!("../../../packages/fixtures/swatch/android/exact.json"),
        ),
        (
            "web",
            include_str!("../../../packages/fixtures/swatch/web/exact.json"),
        ),
    ] {
        put(
            &format!("node_modules/@fixture/swatch/{target}/exact.json"),
            declaration,
        );
    }
    for name in ["lib", "outer"] {
        put(
            &format!("node_modules/@repro/{name}/package.json"),
            &format!(
                r#"{{"name":"@repro/{name}","version":"1.0.0","exports":{{".":"./index.contract"}},"dependencies":{{"@fixture/swatch":"1.0.0","@repro/lib":"1.0.0"}}}}"#
            ),
        );
    }
    const TILE: &str = "component Tile\n  view\n    swatch-box tint=\"#d33\" caption=\"lib\" width=160 height=90 testId=\"box\"\n";
    put("node_modules/@repro/lib/index.contract", TILE);
    put(
        "node_modules/@repro/outer/index.contract",
        "use Tile from \"@repro/lib\"\n",
    );
    let graph = |source: &str| {
        put("app.contract", source);
        let plan =
            contract::compile_path(&dir.join("app.contract")).unwrap_or_else(|e| panic!("{e:?}"));
        artifact_graph(&plan, &json!({}), &[], false, "aarch64-apple-darwin").unwrap()
    };
    for package in ["@repro/lib", "@repro/outer"] {
        let source = format!("use Tile from \"{package}\"\ncomponent App\n  view\n    Tile()\n");
        assert_eq!(
            graph(&source)["extensions"],
            json!({"swatch":"@fixture/swatch"}),
            "{package}"
        );
        let unused =
            format!("use Tile from \"{package}\"\ncomponent App\n  view\n    text \"plain\"\n");
        assert_eq!(graph(&unused)["extensions"], json!({}), "unused {package}");
    }
    assert_eq!(graph("use Tile from \"@repro/lib\"\ncomponent App\n  state shown = false\n  view\n    column\n      if shown\n        Tile()\n")["extensions"], json!({"swatch":"@fixture/swatch"}));
    assert_eq!(
        graph(&format!(
            "component App\n  view\n    text \"plain\"\n{TILE}"
        ))["extensions"],
        json!({})
    );
    put(
        "node_modules/@repro/lib/index.contract",
        "use swatchReports, SwatchReports from \"@fixture/swatch\"\n",
    );
    assert_eq!(
        graph("use swatchReports, SwatchReports from \"@repro/lib\"\ncomponent App\n  resource heard = swatchReports() as shape SwatchReports else empty()\n  view\n    text heard.text\n")["extensions"],
        json!({"swatch":"@fixture/swatch"})
    );
    std::fs::remove_dir_all(dir).unwrap();
}

fn check_package_selection(name: &str, source: &str, expected: Result<Value, &str>) {
    let dir = std::env::temp_dir().join(format!(
        "exact-package-selection-{name}-{}",
        std::process::id()
    ));
    let put = |path: &str, value: &str| {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value).unwrap();
    };
    put(
        "package.json",
        r#"{"dependencies":{"@a/swatch":"1.0.0","@b/swatch":"1.0.0","@fixture/bell":"1.0.0"}}"#,
    );
    for (package, tag) in [
        ("@a/swatch", "a-box"),
        ("@b/swatch", "b-box"),
        ("@fixture/bell", ""),
    ] {
        put(
            &format!("node_modules/{package}/package.json"),
            &json!({"name":package,"version":"1.0.0","exports":{".":"./exact.json"}}).to_string(),
        );
        let mut declaration =
            json!({"kind":"interface","name":package,"version":"1.0.0","names":[]});
        if tag.is_empty() {
            declaration["start"] = json!("load");
            declaration["names"] =
                json!([{"event":"chime","on":"root","raised":"emit","args":[],"hold":4}]);
        } else {
            declaration["views"] = json!({tag:{"props":{}}});
        }
        put(
            &format!("node_modules/{package}/exact.json"),
            &declaration.to_string(),
        );
    }
    put("app.contract", source);
    let plan = contract::compile_path(&dir.join("app.contract")).unwrap();
    let plan = exact_plan::Plan::decode(&plan.encode()).unwrap();
    let graph = artifact_graph(&plan, &json!({}), &[], false, "aarch64-apple-darwin");
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(
        graph
            .map(|graph| graph["extensions"].clone())
            .as_ref()
            .map_err(String::as_str),
        expected.as_ref().map_err(|e| *e)
    );
}

#[test]
fn package_selection_distinguishes_scopes() {
    check_package_selection(
        "scopes",
        "component App\n  view\n    a-box\n",
        Ok(json!({"swatch":"@a/swatch"})),
    );
}

#[test]
fn package_selection_refuses_two_used_packages_with_the_same_basename() {
    check_package_selection(
        "conflicting-scopes",
        "component App\n  view\n    column\n      a-box\n      b-box\n",
        Err("bake-extension-ambiguous: extension 'swatch' names both '@a/swatch' and '@b/swatch'; host rosters require one package per extension"),
    );
}

#[test]
fn package_selection_keeps_event_only_packages() {
    check_package_selection("events", "use chime from \"@fixture/bell\"\ncomponent App\n  state heard = false\n  action hear\n    heard = true\n  view\n    column chime=hear\n      text `${heard}`\n", Ok(json!({"bell":"@fixture/bell"})));
}

#[test]
#[cfg(unix)]
fn plist_keys_ignore_translation_prose_and_keep_library_calls() {
    let dir = std::env::temp_dir().join(format!("exact-plist-sources-{}", std::process::id()));
    let put = |path: &str, text: &str| {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    put("node_modules/@exact/.keep", "");
    std::os::unix::fs::symlink(
        repo.join("packages/geolocation"),
        dir.join("node_modules/@exact/geolocation"),
    )
    .unwrap();
    put(
        "package.json",
        r#"{"dependencies":{"@exact/geolocation":"1.0.0","@repro/lib":"1.0.0"}}"#,
    );
    put(
        "node_modules/@repro/lib/package.json",
        r#"{"name":"@repro/lib","version":"1.0.0","exports":{".":"./index.contract"},"dependencies":{"@exact/geolocation":"1.0.0"}}"#,
    );
    put("node_modules/@repro/lib/index.contract", "use requestPreciseLocation, GeolocationStatus from \"@exact/geolocation\"\ncomponent Locate\n  mutation precise as shape GeolocationStatus\n  action ask\n    send precise = requestPreciseLocation(purposeKey=\"nav\")\n  view\n    button press=ask\n      text \"Locate\"\n");
    put("app.contract", "use Locate from \"@repro/lib\"\n");
    put(
        "strings/en.json",
        r#"{"nav":"Precise location for navigation.","help":"Example: requestPreciseLocation(purposeKey='example')"}"#,
    );
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let plan = contract::compile_path(&dir.join("app.contract")).unwrap();
    std::fs::write(out.join("app.plan"), plan.encode()).unwrap();
    let manifest = Manifest::read(&dir).unwrap();
    let mut compat = Compat {
        id: String::new(),
        inputs: json!({"store":{"L":"0"}}),
        channel: String::new(),
        origin: None,
        activate: "next-launch".into(),
        target: String::new(),
        embedded: json!({"version":"2.3.4","buildNumber":"56"}),
        reach: Value::Null,
        painter: None,
        game: false,
    };
    emit(
        &mut compat,
        "development",
        &dir,
        ("macos", "aarch64-apple-darwin"),
        &manifest,
        &out,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(compat.embedded["version"], "2.3.4");
    assert_eq!(compat.embedded["buildNumber"], "56");
    let result = crate::bun()
        .args([
            "--eval",
            r#"
        import assert from 'node:assert/strict';
        import {readFileSync} from 'node:fs';
        import {modulePlist} from './host/apple/plist.mjs';
        import {infoPlist} from './host/apple/build.mjs';
        const dir = process.argv[1];
        const graph = JSON.parse(readFileSync(dir + '/out/artifacts.json', 'utf8'));
        const app = {id:'com.example.plist', displayName:'Plist', dir, manifest:{host:{}}};
        const reach = modulePlist(app, null, 'ios', graph.extensions, graph.contractSources);
        const plist = infoPlist(app, false, {reach});
        assert.match(plist, /<key>nav<\/key>/);
        assert.doesNotMatch(plist, /<key>example<\/key>/);
        assert.ok(graph.contractSources.every(p => p.endsWith('.contract')));
    "#,
        ])
        .arg(&dir)
        .current_dir(repo)
        .output()
        .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

/// A plan pushed to an installed cohort is described by its own format, so a
/// cohort whose binary reads another format is refused, not given a plan it
/// cannot decode.
#[test]
fn a_plan_graph_names_the_plans_own_format() {
    let plan = exact_plan::builder::PlanBuilder::new(7, 0)
        .finish()
        .unwrap()
        .encode();
    let inputs = json!({"formatVersion": 1, "formatDigest": "0000000000000001", "kernelSchema": "0000000000000002"});
    let graph = plan_graph(&plan, &inputs, "aarch64-apple-darwin").unwrap();
    let requires = &graph["artifacts"][0]["requires"];
    assert_eq!(requires["formatVersion"], json!(exact_plan::FORMAT_VERSION));
    assert_eq!(
        requires["formatDigest"],
        json!(format!("{:016x}", exact_plan::FORMAT_DIGEST))
    );
    assert_eq!(requires["kernelSchema"], json!("0000000000000007"));
}

/// A source's signature names an enum's members, in parameters and nested
/// results alike, so an installed source that answers the old vocabulary is
/// not admitted under a plan that refuses it. Their order and an unrelated
/// declaration do not change it.
#[test]
fn a_source_signature_names_its_enum_members() {
    let sources = |members: &[&str], unrelated: bool| {
        let mut builder = exact_plan::builder::PlanBuilder::new(0, 0);
        if unrelated {
            let number = builder.primitive(exact_plan::TypeKind::Number);
            builder.record("Other", &[("n", number)]);
        }
        let members: Vec<String> = members.iter().map(|m| m.to_string()).collect();
        let state = builder.string_enum(&members);
        let list = builder.list(state);
        let result = builder.record("Result", &[("state", state), ("seen", list)]);
        builder.source("read", &[state], result);
        let plan = builder.finish().unwrap().encode();
        plan_graph(&plan, &json!({}), "aarch64-apple-darwin").unwrap()["sources"]["read"].clone()
    };
    let old = sources(&["old", "common"], false);
    assert_eq!(
        old["params"][0],
        json!({"kind":"String","members":["common","old"]})
    );
    assert_eq!(
        old["result"]["fields"][1]["type"]["item"],
        json!({"kind":"String","members":["common","old"]})
    );
    assert_ne!(old, sources(&["new", "common"], false));
    assert_eq!(old, sources(&["common", "old"], true));
    let mut plain = exact_plan::builder::PlanBuilder::new(0, 0);
    let string = plain.primitive(exact_plan::TypeKind::String);
    plain.source("read", &[string], string);
    let plain = plain.finish().unwrap().encode();
    let plain = plan_graph(&plain, &json!({}), "aarch64-apple-darwin").unwrap();
    assert_eq!(plain["sources"]["read"]["params"][0], json!("String"));
}

#[test]
fn rust_plan_receipts_require_the_baked_wasmi_executor() {
    let mut builder = exact_plan::builder::PlanBuilder::new(0, 0);
    let number = builder.primitive(exact_plan::TypeKind::Number);
    builder.source("count", &[], number);
    let plan = builder.finish().unwrap();
    let (root, _, _) = rust_fixture(&plan.encode());
    std::fs::create_dir_all(root.join("linux/src")).unwrap();
    let target = "x86_64-unknown-linux-gnu";
    for (mode, host, expected) in [
        ("off", "Swappable<App>\n", json!(["native", "wasmi"])),
        ("wasm", "", json!(["native", "wasmi"])),
        (
            "tiered",
            "Swappable<App>\n",
            json!(["native", "native-dylib", "wasmi"]),
        ),
    ] {
        std::fs::write(root.join("linux/src/lib.rs"), host).unwrap();
        std::fs::write(
            root.join("app.json"),
            json!({"app":{"id":"com.exact.caltrain","name":"Test"},
                "rust":{"mode":mode,"module":{"package":"test-logic"}}})
            .to_string(),
        )
        .unwrap();
        let manifest = Manifest::read(&root).unwrap();
        let compat =
            crate::compat::compatibility_id(&root, "linux", target, &manifest, Some("")).unwrap();
        let graph = artifact_graph(&plan, &compat.inputs, &[], false, target).unwrap();
        assert_eq!(
            graph["artifacts"][0]["requires"]["executors"], expected,
            "{mode}"
        );
        assert_eq!(
            compat.inputs["store"]["acceptedKinds"],
            json!(["plan", "assets", "wasm"])
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn loopback_callback_ports_retain_the_listener() {
    let plan = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap()
        .encode();
    for callback in [
        "http://127.0.0.1/auth/callback",
        "http://127.0.0.1:1455/auth/callback",
        "http://[::1]:1455/auth/callback",
    ] {
        let inputs = json!({"grantCeiling": format!("auth.callback {callback}")});
        let graph = plan_graph(&plan, &inputs, "aarch64-apple-darwin").unwrap();
        assert!(
            graph["loads"]
                .as_array()
                .unwrap()
                .contains(&json!("listen")),
            "{callback}"
        );
    }
    assert!(!listens(
        &json!({"grantCeiling": "auth.callback https://example.com/auth/callback"})
    ));
}

/// LLP 1012.001.000 D6: a development bake leaves its plan's map beside
/// it, keyed by that plan's digest; a production bake leaves none, and a
/// plan whose nodes the contract no longer compiles to gets none.
#[test]
fn a_development_bake_leaves_its_plans_source_map() {
    let root = std::env::temp_dir().join(format!("exact-map-{}", std::process::id()));
    let (app, out) = (root.join("app"), root.join("out"));
    std::fs::create_dir_all(&app).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        app.join("app.contract"),
        "component App\n  view\n    text \"a\" testId=\"a\"\n",
    )
    .unwrap();
    let plan = contract::compile_path(&app.join("app.contract")).unwrap();
    let bytes = plan.encode();
    let map = out.join("app.plan.map.json");
    source_map("development", &app, &out, &plan, &bytes);
    let written: Value = serde_json::from_slice(&std::fs::read(&map).unwrap()).unwrap();
    assert_eq!(written["digest"], contract::plan_digest(&bytes));
    source_map("production", &app, &out, &plan, &bytes);
    assert!(!map.exists(), "a production bake carries no map");
    let other = contract::compile("component App\n  view\n    column\n      text \"b\"\n").unwrap();
    source_map("development", &app, &out, &other, &other.encode());
    assert!(!map.exists(), "a map for other nodes is never written");
    std::fs::remove_dir_all(&root).unwrap();
}

/// LLP 1096 D8: a declared sound is not a node, so the plan's `sounds`
/// table is what says a fixed build carries the sound arm.
#[test]
fn a_plan_with_a_sound_loads_the_sound_arm() {
    let without = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap();
    let mut b = exact_plan::builder::PlanBuilder::new(0, 0);
    b.sound("assets/kick.wav", 4_800, 48_000, 1, "00");
    let with = b.finish().unwrap();
    for (plan, loads) in [(without, json!([])), (with, json!(["sound"]))] {
        let graph = artifact_graph(&plan, &json!({}), &[], false, "aarch64-apple-darwin").unwrap();
        assert_eq!(graph["loads"], loads);
    }
}

/// LLP 1042 §9: an image's picture in picture is the video arm's, so a
/// plan that asks for it loads the arm with no `video` node.
#[test]
fn a_plan_that_floats_an_image_loads_the_video_arm() {
    let plan = contract::compile(
            "component App\n  action float\n    requestPictureInPicture(\"page\")\n  view\n    column\n      button press=float\n        text \"Float\"\n      image \"assets/a.png\" id=\"page\" width=1 height=1\n",
        )
        .unwrap();
    let graph = artifact_graph(&plan, &json!({}), &[], false, "aarch64-apple-darwin").unwrap();
    assert_eq!(graph["loads"], json!(["video"]));
}

/// LLP 1075.003.000.001 §4.3: the plan requires the hatch words it marks
/// that this platform's module handles, and no others.
#[test]
fn a_plan_requires_the_hatch_words_its_platform_handles() {
    let mut b = exact_plan::builder::PlanBuilder::new(0, 0);
    for word in ["dot", "avatar", "avatar", "badge"] {
        let expr = b.constant(&exact_plan::Value::str(word));
        let bindings = [exact_plan::BindingsRow {
            kind: exact_plan::BindingKind::Prop,
            id: exact_kernel::PropId::Hatch as u16,
            expr,
            written: true,
        }];
        b.node(
            exact_kernel::NodeType::Video as u8,
            None,
            None,
            0,
            &bindings,
            &[],
            None,
        );
    }
    let plan = b.finish().unwrap();
    let requires = |inputs: Value| {
        let graph = artifact_graph(&plan, &inputs, &[], false, "aarch64-apple-darwin").unwrap();
        graph["artifacts"][0]["requires"]["hatches"].clone()
    };
    // This platform handles `avatar` and `dot`; `badge` is another platform's.
    assert_eq!(
        requires(json!({"hatches": ["avatar", "dot", "unused"]})),
        json!(["avatar", "dot"])
    );
    // A platform that handles none of them is asked for nothing.
    assert_eq!(requires(json!({"hatches": null})), Value::Null);
    assert_eq!(requires(json!({})), Value::Null);
}

/// LLP 1108 D6 R2: a TypeScript app's plan requires the prelude runtime
/// its compatibility inputs carry (`exactBodyFrom`), so an older cohort
/// takes no bundle; an app without one requires none.
#[test]
fn a_plan_requires_the_typescript_runtime_its_inputs_carry() {
    let plan = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap();
    let requires = |inputs: Value| {
        let graph = artifact_graph(&plan, &inputs, &[], false, "aarch64-apple-ios").unwrap();
        graph["artifacts"][0]["requires"]["typescriptRuntime"].clone()
    };
    assert_eq!(
        requires(json!({"typescriptRuntime": crate::compat::TYPESCRIPT_RUNTIME})),
        json!(crate::compat::TYPESCRIPT_RUNTIME)
    );
    assert_eq!(requires(json!({})), Value::Null);
}

/// A TypeScript module pair requires exactly the binary's TypeScript grants
/// and its prelude's runtime, as a Rust module requires its own.
#[test]
fn a_typescript_pair_requires_its_grants_and_runtime() {
    let plan = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap();
    let inputs = json!({"javascriptGrants": "net.fetch https://x/", "typescriptRuntime": 2});
    let assets = ["typescript/app.hbc", "typescript/app.module.json"]
        .map(|name| json!({"name": name, "sha256": "00", "bytes": 1}));
    let graph = artifact_graph(&plan, &inputs, &assets, false, "aarch64-apple-ios").unwrap();
    for row in &graph["artifacts"].as_array().unwrap()[1..] {
        assert_eq!(
            row["requires"],
            json!({"javascriptGrants": "net.fetch https://x/", "typescriptRuntime": 2}),
            "{row}"
        );
    }
}

/// LLP 1098 D8: the receipt says whether a node claims the media session.
#[test]
fn a_plan_with_metadata_claims_the_media_session() {
    for (prop, claims) in [
        (exact_kernel::PropId::Src, false),
        (exact_kernel::PropId::MediaTitle, true),
    ] {
        let mut b = exact_plan::builder::PlanBuilder::new(0, 0);
        let expr = b.constant(&exact_plan::Value::str("Episode 1"));
        let bindings = [exact_plan::BindingsRow {
            kind: exact_plan::BindingKind::Prop,
            id: prop as u16,
            expr,
            written: true,
        }];
        b.node(
            exact_kernel::NodeType::Video as u8,
            None,
            None,
            0,
            &bindings,
            &[],
            None,
        );
        let graph = artifact_graph(
            &b.finish().unwrap(),
            &json!({}),
            &[],
            false,
            "aarch64-apple-darwin",
        )
        .unwrap();
        assert_eq!(
            (graph["loads"].clone(), graph["mediaSession"].clone()),
            (json!(["video"]), json!(claims))
        );
    }
}

/// LLP 1116.011 D2.3: the receipt says what the first route stands on, for
/// the iOS launch screen: the grouped grey where the route scrolls a
/// grouped list (stopwatch, round 13), the page's own otherwise.
#[test]
fn a_plan_says_the_ground_its_first_route_stands_on() {
    const SCREEN: &str = "routes nav
  home \"/\"
component App
  view
    main navigationKey=`${top(nav).id}` navigationBack=\"back\"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` navigationScroll=\"l\" ROUTE position=\"absolute\" inset=0
          header
            text \"Laps\" role=\"heading\" aria-level=1
          LIST
            section
              row
                text \"One\"
";
    const GROUPED: &str =
        "list id=\"l\" appearance=\"auto\" listStyle=\"inset-grouped\" flex=1 min-height=0";
    let launch = |route: &str, list: &str| {
        let plan = contract::compile(&SCREEN.replace("ROUTE", route).replace("LIST", list))
            .unwrap_or_else(|e| panic!("{e}"));
        artifact_graph(&plan, &json!({}), &[], false, "aarch64-apple-ios-sim").unwrap()
            ["launchBackground"]
            .clone()
    };
    assert_eq!(launch("", GROUPED), json!("grouped"));
    assert_eq!(
        launch("", &GROUPED.replace("inset-grouped", "grouped")),
        json!("grouped")
    );
    // Any other scroll, or a page of its own, is the platform's.
    assert_eq!(
        launch("", "scroll id=\"l\" flex=1 min-height=0"),
        json!("system")
    );
    assert_eq!(
        launch("background-color=\"-exact-background\"", GROUPED),
        json!("system")
    );
    // No route at all, as a plan with no navigation root is.
    let bare = contract::compile("component App\n  view\n    text \"a\" testId=\"a\"\n").unwrap();
    assert_eq!(
        artifact_graph(&bare, &json!({}), &[], false, "aarch64-apple-ios-sim").unwrap()
            ["launchBackground"],
        json!("system")
    );
}

#[test]
fn a_no_delivery_development_bake_names_its_gpu_dev_module() {
    // Games are store L=0: the cohort has no trust input, yet the
    // host-less proof's split host bake must name the gpu-dev module.
    let card = gpu_card(
        "com.example.game",
        "c",
        "development",
        Some("libg.so".into()),
        None,
    );
    assert_eq!(card.as_ref().unwrap()["name"], "libg.so");
    assert_eq!(card.as_ref().unwrap()["receipt"], true);
    let product = Some(("libg.so".into(), "ab".into()));
    let release = gpu_card(
        "com.example.game",
        "c",
        "production",
        Some("libg.so".into()),
        product,
    );
    assert_eq!(release.as_ref().unwrap()["sha256"], "ab");
    assert_eq!(release.unwrap()["trust"], "production");
    assert!(gpu_card(
        "com.example.game",
        "c",
        "production",
        Some("libg.so".into()),
        None
    )
    .is_none());
}

fn rust_fixture(plan: &[u8]) -> (PathBuf, Value, Value) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "exact-rust-bake-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    // Parallel tests can observe the same clock tick. Each fixture owns
    // its directory exclusively, including cleanup after staging.
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("input")).unwrap();
    // The inputs a Rust cohort's compatibility gives (`compat.rs`).
    let inputs = json!({"app":"com.exact.caltrain","grantCeiling":"","rustGrants":"",
        "rustMode":"wasm","rustAbi":crate::compat::RUST_ABI});
    let module = b"\0asm\x01\0\0\0";
    let receipt = json!({"version":1,"kind":"rust","abi":crate::compat::RUST_ABI,"appId":inputs["app"],"grants":"",
            "target":"wasm32-unknown-unknown","executor":"wasm",
            "plan":{"file":"app.plan","sha256":hash(plan),"bytes":plan.len()},
            "module":{"file":"app.module.wasm","sha256":hash(module),"bytes":module.len()}});
    std::fs::write(root.join("input/app.module.wasm"), module).unwrap();
    std::fs::write(root.join("input/app.module.json"), receipt.to_string()).unwrap();
    (root, inputs, receipt)
}

#[test]
fn supplemental_rust_is_real_paired_bytes_with_executor_requirements() {
    let mut plan = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap();
    plan.app_id = "com.exact.caltrain".into();
    let bytes = plan.encode();
    let (root, inputs, _) = rust_fixture(&bytes);
    let cards = stage_rust_bundle(
        &root.join("input"),
        &root.join("out"),
        &bytes,
        &inputs,
        "aarch64-apple-ios",
    )
    .unwrap();
    assert_eq!(cards.len(), 2);
    for card in &cards {
        let actual = std::fs::read(root.join("out").join(card["name"].as_str().unwrap())).unwrap();
        assert_eq!(card["sha256"], hash(&actual));
        assert_eq!(card["bytes"], actual.len());
    }
    let graph = artifact_graph(&plan, &inputs, &cards, false, "aarch64-apple-ios").unwrap();
    for row in graph["artifacts"].as_array().unwrap() {
        assert_eq!(row["requires"]["rustAbi"], inputs["rustAbi"]);
        assert_eq!(row["requires"]["rustMode"], "wasm");
        assert_eq!(row["requires"]["rustTarget"], "wasm32-unknown-unknown");
    }
    assert!(graph["artifacts"][0]["requires"].get("sources").is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn tiered_assets_bind_the_complete_container_and_exact_native_cohort() {
    let mut plan = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap();
    plan.app_id = "com.exact.caltrain".into();
    let bytes = plan.encode();
    let (root, mut inputs, mut receipt) = rust_fixture(&bytes);
    let target = "aarch64-apple-darwin";
    let module = b"EXLT\x01\0\0\0\x08\0\0\0\0asm\x01\0\0\0native";
    inputs["rustMode"] = json!("tiered");
    receipt["executor"] = json!("tiered");
    receipt["target"] = json!(target);
    receipt["module"] = json!({"file":"app.module.bin","sha256":hash(module),"bytes":module.len()});
    std::fs::write(root.join("input/app.module.bin"), module).unwrap();
    std::fs::write(root.join("input/app.module.json"), receipt.to_string()).unwrap();
    let stage = |host| {
        stage_rust_bundle(
            &root.join("input"),
            &root.join("out"),
            &bytes,
            &inputs,
            host,
        )
    };
    let cards = stage(target).unwrap();
    let graph = artifact_graph(&plan, &inputs, &cards, false, target).unwrap();
    for row in graph["artifacts"].as_array().unwrap() {
        assert_eq!(row["requires"]["rustMode"], "tiered");
        assert_eq!(row["requires"]["rustTarget"], target);
    }
    for incompatible in [
        "x86_64-apple-darwin",
        "aarch64-apple-ios",
        "wasm32-unknown-unknown",
    ] {
        assert!(stage(incompatible).is_err(), "{incompatible}");
    }
    // A changed native companion is covered by the same authenticated digest.
    let mut changed = module.to_vec();
    *changed.last_mut().unwrap() ^= 1;
    std::fs::write(root.join("input/app.module.bin"), changed).unwrap();
    assert!(stage(target).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn supplemental_rust_refuses_policy_identity_grants_target_and_tampered_pair() {
    let plan = b"exact baked plan";
    let (root, mut inputs, receipt) = rust_fixture(plan);
    for (key, value) in [
        ("appId", json!("another.app")),
        ("grants", json!("secret.keep token")),
        ("abi", json!(99)),
        ("kind", json!("javascript")),
        ("target", json!("aarch64-apple-ios")),
        ("executor", json!("native")),
    ] {
        let mut changed = receipt.clone();
        changed[key] = value;
        std::fs::write(root.join("input/app.module.json"), changed.to_string()).unwrap();
        assert!(
            stage_rust_bundle(
                &root.join("input"),
                &root.join("out"),
                plan,
                &inputs,
                "aarch64-apple-ios"
            )
            .is_err(),
            "{key}"
        );
        assert!(!root.join("out/rust").exists());
    }
    std::fs::write(root.join("input/app.module.json"), receipt.to_string()).unwrap();
    inputs["rustMode"] = json!("off");
    assert!(stage_rust_bundle(
        &root.join("input"),
        &root.join("out"),
        plan,
        &inputs,
        "aarch64-apple-ios"
    )
    .is_err());
    inputs["rustMode"] = json!("wasm");
    assert!(stage_rust_bundle(
        &root.join("input"),
        &root.join("out"),
        b"changed plan",
        &inputs,
        "aarch64-apple-ios"
    )
    .is_err());
    std::fs::write(root.join("input/app.module.wasm"), b"changed module").unwrap();
    assert!(stage_rust_bundle(
        &root.join("input"),
        &root.join("out"),
        plan,
        &inputs,
        "aarch64-apple-ios"
    )
    .is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn mixed_rust_receipt_preserves_its_own_scope_under_the_union_ceiling() {
    let plan = b"mixed baked plan";
    let (root, mut inputs, mut receipt) = rust_fixture(plan);
    inputs["grantCeiling"] = json!("fs.read app:/data/rust\nnet.fetch https://example.test/");
    inputs["rustGrants"] = json!("fs.read app:/data/rust");
    inputs["javascriptGrants"] = json!("net.fetch https://example.test/");
    receipt["grants"] = inputs["rustGrants"].clone();
    std::fs::write(root.join("input/app.module.json"), receipt.to_string()).unwrap();
    assert!(stage_rust_bundle(
        &root.join("input"),
        &root.join("out"),
        plan,
        &inputs,
        "aarch64-apple-ios"
    )
    .is_ok());
    receipt["grants"] = inputs["grantCeiling"].clone();
    std::fs::write(root.join("input/app.module.json"), receipt.to_string()).unwrap();
    assert!(
        stage_rust_bundle(
            &root.join("input"),
            &root.join("out"),
            plan,
            &inputs,
            "aarch64-apple-ios"
        )
        .is_err(),
        "a module cannot borrow its sibling's grant"
    );
    receipt["grants"] = inputs["rustGrants"].clone();
    std::fs::write(root.join("input/app.module.json"), receipt.to_string()).unwrap();
    inputs["grantCeiling"] = inputs["javascriptGrants"].clone();
    assert!(
        stage_rust_bundle(
            &root.join("input"),
            &root.join("out"),
            plan,
            &inputs,
            "aarch64-apple-ios"
        )
        .is_err(),
        "source grants must fit the host ceiling"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn signed_release_requires_the_complete_real_supplemental_roster() {
    let (mut compat, mut receipt, plan) = signed_fixture();
    let (root, inputs, _) = rust_fixture(&plan);
    let cards = stage_rust_bundle(
        &root.join("input"),
        &root.join("out"),
        &plan,
        &inputs,
        "aarch64-apple-ios",
    )
    .unwrap();
    for card in &cards {
        receipt["assets"]["assets"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "name":card["name"],"sha256":card["sha256"],"bytes":card["bytes"],
                "url":format!("./{}",card["sha256"].as_str().unwrap())}));
    }
    // The head names the grown index by its canonical bytes.
    let index = exact_update::canonical_bytes(&receipt["assets"].to_string()).unwrap();
    receipt["envelope"]["assets"] = json!({
        "url": format!("../../blobs/{}", exact_update::sha256_hex(&index)),
        "sha256": exact_update::sha256_hex(&index), "bytes": index.len(),
        "count": receipt["assets"]["assets"].as_array().unwrap().len()});
    // A fresh process-local test key: no publisher's private key is stored.
    let canonical = exact_update::canonical_bytes(&receipt["envelope"].to_string()).unwrap();
    let output = crate::bun().args(["-e", r#"
            const {generateKeyPairSync,sign}=require('node:crypto');
            const {publicKey,privateKey}=generateKeyPairSync('ed25519');
            process.stdout.write(JSON.stringify({public:publicKey.export({type:'spki',format:'der'}).subarray(-32).toString('base64'),
                signature:sign(null,Buffer.from(process.argv[1]),privateKey).toString('base64')}));
        "#]).arg(String::from_utf8(canonical).unwrap()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let signing: Value = serde_json::from_slice(&output.stdout).unwrap();
    compat.inputs["keys"] = json!({"fixture":signing["public"]});
    receipt["envelope"]["signature"] = json!({"keyId":"fixture","ed25519":signing["signature"]});
    let receipt = serde_json::to_vec(&receipt).unwrap();
    assert!(apply_release(&mut compat, &receipt, &plan)
        .unwrap_err()
        .contains("complete embedded asset roster"));
    let assets = compat.embedded["assets"].as_array_mut().unwrap();
    assets.extend(cards);
    assets.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    apply_release(&mut compat, &receipt, &plan).unwrap();
    assert_eq!(compat.embedded["seq"], 1);
    std::fs::remove_dir_all(root).unwrap();
}

/// LLP 1141.001 D1: a bundled SVG image whose art is under a `mask`
/// loads the island module, though the plan has no `mask`; the same
/// logo without one, an SVG under another name and a PNG load nothing.
#[test]
#[cfg(unix)]
fn a_masked_svg_image_loads_the_svg_island_module() {
    let root = std::env::temp_dir().join(format!(
        "exact-bake-svg-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("assets/sponsors")).unwrap();
    let art = "<path d=\"M0 0H5V5H0Z\" fill=\"#F2545B\"/>";
    let masked = format!(
            "<svg width=\"24\" height=\"12\" viewBox=\"0 0 24 12\" xmlns=\"http://www.w3.org/2000/svg\">\
             <mask id=\"m\" style=\"mask-type:luminance\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"24\" height=\"12\">\
             <path d=\"M24 0H0V12H24V0Z\" fill=\"white\"/></mask><g mask=\"url(#m)\">{art}</g></svg>"
        );
    let plain =
        format!("<svg viewBox=\"0 0 24 12\" xmlns=\"http://www.w3.org/2000/svg\">{art}</svg>");
    std::fs::write(root.join("assets/sponsors/plain.svg"), &plain).unwrap();
    std::fs::write(root.join("assets/icon.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    let plan = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap();
    let manifest = Manifest::read(&root).unwrap();
    let loads = |root: &Path| {
        let cards = asset_cards(root, &root.join("out"), &manifest, "ios")
            .unwrap()
            .0;
        let pictures = asset_islands(root, &cards);
        artifact_graph(&plan, &json!({}), &cards, pictures, "aarch64-apple-ios").unwrap()["loads"]
            .clone()
    };
    assert_eq!(loads(&root), json!([]));
    // The host sniffs the bytes, so a masked SVG named `.png` counts too.
    std::fs::write(root.join("assets/sponsors/logo.png"), &masked).unwrap();
    assert_eq!(loads(&root), json!(["svg"]));
    std::fs::remove_file(root.join("assets/sponsors/logo.png")).unwrap();
    std::fs::write(root.join("assets/sponsors/gold.svg"), &masked).unwrap();
    assert_eq!(loads(&root), json!(["svg"]));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(unix)]
fn the_embedded_roster_uses_the_copy_gate_and_refuses_symlink_assets() {
    let root = std::env::temp_dir().join(format!(
        "exact-bake-assets-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::create_dir_all(root.join("deck/nested")).unwrap();
    std::fs::write(root.join("assets/image.png"), b"image").unwrap();
    std::fs::write(root.join("deck/nested/index.html"), b"deck").unwrap();
    for folder in ["assets", "deck", "deck/nested"] {
        std::fs::write(root.join(folder).join(".DS_Store"), b"Bud1").unwrap();
    }
    let manifest = Manifest::read(&root).unwrap();
    let cards = asset_cards(&root, &root.join("out"), &manifest, "ios")
        .unwrap()
        .0;
    assert_eq!(
        cards,
        vec![
            json!({"name":"assets/image.png","sha256":hash(b"image"),"bytes":5}),
            json!({"name":"deck/nested/index.html","sha256":hash(b"deck"),"bytes":4}),
        ]
    );
    assert_eq!(
        asset_cards(&root.join("absent/.."), &root.join("out"), &manifest, "ios")
            .unwrap()
            .0,
        cards
    );
    std::fs::write(root.join("assets/two words.png"), b"space").unwrap();
    assert!(asset_cards(&root, &root.join("out"), &manifest, "ios")
        .unwrap_err()
        .contains("bake asset gate: the asset name assets/two words.png is not portable"));
    std::fs::remove_file(root.join("assets/two words.png")).unwrap();
    std::fs::write(root.join("outside"), b"must not be embedded").unwrap();
    std::os::unix::fs::symlink("../outside", root.join("assets/escape")).unwrap();
    assert!(asset_cards(&root, &root.join("out"), &manifest, "ios")
        .unwrap_err()
        .contains("bake asset gate"));
    std::fs::remove_file(root.join("assets/escape")).unwrap();
    std::fs::create_dir(root.join("outside-dir")).unwrap();
    std::fs::write(root.join("outside-dir/file"), b"outside").unwrap();
    for target in ["../outside-dir", "../missing"] {
        std::os::unix::fs::symlink(target, root.join("assets/nested")).unwrap();
        assert!(asset_cards(&root, &root.join("out"), &manifest, "ios")
            .unwrap_err()
            .contains("bake asset gate"));
        std::fs::remove_file(root.join("assets/nested")).unwrap();
    }
    std::fs::remove_dir_all(root.join("assets")).unwrap();
    for target in ["outside-dir", "missing"] {
        std::os::unix::fs::symlink(target, root.join("assets")).unwrap();
        assert!(asset_cards(&root, &root.join("out"), &manifest, "ios")
            .unwrap_err()
            .contains("bake asset gate"));
        std::fs::remove_file(root.join("assets")).unwrap();
    }
    std::fs::remove_dir_all(root.join("deck")).unwrap();
    assert!(asset_cards(&root, &root.join("out"), &manifest, "ios")
        .unwrap()
        .0
        .is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "async lane: inventories a 392 MiB asset tree"]
fn a_large_embedded_roster_hashes_each_file_without_a_tree_reply() {
    let root = std::env::temp_dir().join(format!(
        "exact-bake-large-assets-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("assets")).unwrap();
    let mut expected = Vec::new();
    // Base64 for this tree exceeds the helper's 512 MiB reply buffer.
    for index in 0..7 {
        let bytes = vec![index as u8; 56 << 20];
        let name = format!("assets/{index}.bin");
        std::fs::write(root.join(&name), &bytes).unwrap();
        expected.push(json!({"name":name,"sha256":hash(&bytes),"bytes":bytes.len()}));
    }
    let manifest = Manifest::read(&root).unwrap();
    let cards = asset_cards(&root, &root.join("out"), &manifest, "ios");
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(cards.unwrap().0, expected);
}

/// A baked texture's three files: a Mac's roster holds its BC payload, an
/// iPhone's its ASTC payload, the web's all three. An authored `.tex` with no
/// payloads stays everywhere. A roster that left a file out names the family
/// it kept, which joins the cohort's inputs; one that left none out names none.
#[test]
fn a_native_roster_holds_only_its_platforms_texture_family() {
    let root = std::env::temp_dir().join(format!(
        "exact-bake-textures-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("assets/fox")).unwrap();
    for name in [
        "fox/0.tex",
        "fox/0.bc.tex",
        "fox/0.astc.tex",
        "hand.tex",
        "fox.model",
    ] {
        std::fs::write(root.join("assets").join(name), name).unwrap();
    }
    let manifest = Manifest::read(&root).unwrap();
    let names = |platform| {
        asset_cards(&root, &root.join("out"), &manifest, platform)
            .unwrap()
            .0
            .iter()
            .map(|card| card["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let family = |platform| {
        asset_cards(&root, &root.join("out"), &manifest, platform)
            .unwrap()
            .1
    };
    assert_eq!(family("macos").as_deref(), Some("bc"));
    assert_eq!(family("ios").as_deref(), Some("astc"));
    assert_eq!(family("web"), None);
    assert_eq!(
        names("macos"),
        ["assets/fox.model", "assets/fox/0.bc.tex", "assets/hand.tex"]
    );
    assert_eq!(
        names("ios"),
        [
            "assets/fox.model",
            "assets/fox/0.astc.tex",
            "assets/hand.tex"
        ]
    );
    for platform in ["web", "linux", "windows", "android"] {
        assert_eq!(names(platform).len(), 5, "{platform}");
    }
    for name in ["fox/0.tex", "fox/0.bc.tex", "fox/0.astc.tex"] {
        std::fs::remove_file(root.join("assets").join(name)).unwrap();
    }
    assert_eq!(family("ios"), None, "nothing left out: an older cohort's");
    std::fs::remove_dir_all(root).unwrap();
}

fn signed_fixture() -> (Compat, Value, Vec<u8>) {
    let raw = include_bytes!("../../../update/tests/it/fixtures/publisher/exact.json");
    let index = include_bytes!("../../../update/tests/it/fixtures/publisher/assets.json");
    let mut head = Envelope::parse(raw).unwrap();
    head.read_index(index).unwrap();
    let plan = include_bytes!("../../../update/tests/it/fixtures/publisher/app.plan").to_vec();
    let key = include_str!("../../../update/tests/it/fixtures/publisher/caltrain-2026.pub").trim();
    let mut assets = head
        .assets()
        .unwrap()
        .iter()
        .map(|a| json!({"name":a.name,"sha256":a.sha256,"bytes":a.bytes}))
        .collect::<Vec<_>>();
    assets.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let compat = Compat {
        id: head.stream.compatibility_id.clone(),
        inputs: json!({"app":head.app_id,"trust":"production","keys":{"caltrain-2026":key},"store":{"L":"A"}}),
        channel: head.stream.channel.clone(),
        origin: None,
        activate: "next-launch".into(),
        target: "aarch64-apple-ios".into(),
        reach: Value::Null,
        painter: None,
        game: false,
        embedded: json!({"seq":0,"genesis":true,"entryDigest":null,"plan":{"sha256":head.plan.sha256,"bytes":head.plan.bytes},"assets":assets}),
    };
    (
        compat,
        json!({"envelope":serde_json::from_slice::<Value>(raw).unwrap(),
            "assets":serde_json::from_slice::<Value>(index).unwrap()}),
        plan,
    )
}

#[test]
fn a_production_sequence_is_bound_to_a_signed_matching_complete_bundle() {
    let (mut compat, receipt, plan) = signed_fixture();
    compat.embedded["version"] = json!("2.3.4");
    compat.embedded["buildNumber"] = json!("56");
    apply_release(&mut compat, &serde_json::to_vec(&receipt).unwrap(), &plan).unwrap();
    assert_eq!(compat.embedded["seq"], 1);
    assert_eq!(compat.embedded["version"], "2.3.4");
    assert_eq!(compat.embedded["buildNumber"], "56");
    assert_eq!(compat.embedded["genesis"], false);
    assert_eq!(compat.embedded["entryDigest"].as_str().unwrap().len(), 64);
    let baked = Baked::from_compat(&compat.to_json(), &plan).unwrap();
    assert_eq!(baked.embedded.compatibility_id, compat.id);
    assert_eq!(baked.embedded.seq, 1);

    let (mut wrong, _, _) = signed_fixture();
    wrong.embedded["assets"] = json!([]);
    assert!(
        apply_release(&mut wrong, &serde_json::to_vec(&receipt).unwrap(), &plan)
            .unwrap_err()
            .contains("complete embedded asset roster")
    );
    let (mut wrong, _, _) = signed_fixture();
    wrong.embedded["plan"]["sha256"] = json!("0".repeat(64));
    assert!(
        apply_release(&mut wrong, &serde_json::to_vec(&receipt).unwrap(), &plan)
            .unwrap_err()
            .contains("embedded.plan does not match")
    );
    let (mut wrong, mut forged, _) = signed_fixture();
    forged["envelope"]["stream"]["seq"] = json!(41);
    assert!(apply_release(&mut wrong, &serde_json::to_vec(&forged).unwrap(), &plan).is_err());
    // An index that is not the bytes the signed head names is refused.
    let (mut wrong, mut forged, _) = signed_fixture();
    forged["assets"]["assets"][0]["bytes"] = json!(1);
    assert!(
        apply_release(&mut wrong, &serde_json::to_vec(&forged).unwrap(), &plan)
            .unwrap_err()
            .contains("asset index")
    );
    let (mut wrong, _, _) = signed_fixture();
    wrong.channel = "another".into();
    assert!(apply_release(&mut wrong, &serde_json::to_vec(&receipt).unwrap(), &plan).is_err());
}

#[test]
fn a_canvas_argument_the_gpu_product_does_not_declare_refuses_the_bake() {
    use exact_plan::surface_arguments::{record, record_len};
    const FIELDS: &[(&str, ())] = &[("count", ())];
    static RECORD: [u8; record_len("dice", FIELDS)] = record("dice", FIELDS);
    let dir = std::env::temp_dir().join(format!("exact-surface-arguments-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let product = dir.join("libdice_gpu.dylib");
    std::fs::write(
        &product,
        [b"\xcf\xfa\xed\xfe code ".as_slice(), &RECORD].concat(),
    )
    .unwrap();
    let mut declared = Declared::new();
    gpu_product(&product, &mut declared).unwrap();
    let mut b = exact_plan::builder::PlanBuilder::new(0, 0);
    let one = b.constant(&exact_plan::Value::Number(1.));
    b.surface("dice", &[("count", one)]);
    b.surface("chart", &[("series", one)]);
    assert_eq!(surface_arguments(b.plan(), &declared), Ok(()));
    b.surface("dice", &[("count", one), ("session_carry", one)]);
    let refusal = surface_arguments(b.plan(), &declared).unwrap_err();
    assert!(refusal.starts_with("canvas surface `dice` passes `session_carry`, which the GPU product libdice_gpu.dylib does not declare (it declares count): a source changed after the GPU product compiled"), "{refusal}");
    assert!(refusal.ends_with("build again"), "{refusal}");
    // More values by position than the product declares: the edit added an
    // argument and the canvas passes it by position.
    let mut positional = exact_plan::builder::PlanBuilder::new(0, 0);
    let one = positional.constant(&exact_plan::Value::Number(1.));
    let yes = positional.constant(&exact_plan::Value::Bool(true));
    positional.surface("dice", &[("", one)]);
    assert_eq!(surface_arguments(positional.plan(), &declared), Ok(()));
    positional.surface("dice", &[("", one), ("", yes)]);
    let refusal = surface_arguments(positional.plan(), &declared).unwrap_err();
    assert!(refusal.starts_with("canvas surface `dice` passes 2 values by position, and the GPU product libdice_gpu.dylib declares 1 argument (count): a source changed after the GPU product compiled"), "{refusal}");
    // The web's wasm product is read without a digest.
    let mut web = Declared::new();
    let wasm = dir.join("dice_gpu.wasm");
    assert_eq!(
        declare(&wasm, &[b"\0asm".as_slice(), &RECORD].concat(), &mut web),
        Ok("dice_gpu.wasm".into())
    );
    assert_eq!(
        web["dice"],
        (vec!["count".to_owned()], "dice_gpu.wasm".to_owned())
    );
    std::fs::write(&product, b"no declaration").unwrap();
    let mut none = Declared::new();
    gpu_product(&product, &mut none).unwrap();
    assert_eq!(surface_arguments(b.plan(), &none), Ok(()));
    std::fs::remove_dir_all(dir).unwrap();
}
