//! The JS target's row emitter (rows.rs): style writes, presence declarations and press timing.
#[test]
fn data_names_added_by_attribute_and_canvas_writers_are_reserved() {
    let plan = contract::compile(
            "component App\n  view\n    canvas bitmap-width=16 bitmap-height=16 color-space=\"display-p3\" color-type=\"float16\"\n",
        )
        .unwrap();
    let mut attrs = Vec::new();
    for node in plan.nodes.iter() {
        attrs.extend(crate::style::canvas_bitmap(&plan, &node).unwrap());
    }
    let mut props = exact_kernel::SortedMap::default();
    props.insert("src".into(), "app:/movie.mp4".into());
    props.insert("poster".into(), "app:/poster.png".into());
    props.insert("autofocus".into(), "false".into());
    for tag in ["img", "video", "audio", "iframe", "input"] {
        attrs.extend(super::attributes(tag, &props).0);
    }
    let names: std::collections::BTreeSet<_> = attrs
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| name.starts_with("data-"))
        .collect();
    let mut picture = exact_kernel::SortedMap::default();
    picture.insert("src".into(), "swatch:red".into());
    let picture_attrs = super::attributes("img", &picture).0;
    assert!(picture_attrs.contains(&("decoding".into(), "async".into())));
    assert!(
        !picture_attrs.iter().any(|(name, _)| name == "src"),
        "a package's scheme is never fetched"
    );
    let names: std::collections::BTreeSet<_> = names
        .into_iter()
        .chain(picture_attrs.iter().map(|(n, _)| n.as_str()))
        .filter(|name| name.starts_with("data-"))
        .collect();
    for name in [
        "data-app-poster",
        "data-color-space",
        "data-color-type",
        "data-exact-picture",
    ] {
        assert!(names.contains(name), "writer did not emit {name}");
    }
    let mut unreserved = Vec::new();
    for name in names {
        let source = format!("component App\n  view\n    column {name}=\"x\"\n");
        match contract::compile(&source) {
            Err(error) if error.id == "lower-data-word" => {}
            _ => unreserved.push(name),
        }
    }
    assert!(unreserved.is_empty(), "not reserved: {unreserved:?}");
}

#[test]
fn a_literal_data_frame_is_shown_by_its_sandbox() {
    // A literal opaque sandbox decides here; a bound one, in rt.js `P`,
    // which is handed the source (no attribute until then); a
    // same-origin one refuses it.
    let attrs = |sandbox: Option<&str>| {
        let mut props = exact_kernel::SortedMap::default();
        props.insert("src".to_string(), "data:text/html,hi".to_string());
        if let Some(s) = sandbox {
            props.insert("sandbox".to_string(), s.to_string());
        }
        let (attrs, _, _) = super::attributes("iframe", &props);
        attrs.into_iter().find(|(k, _)| k == "src").map(|a| a.1)
    };
    assert_eq!(attrs(Some("allow-scripts")).unwrap(), "data:text/html,hi");
    assert_eq!(
        attrs(Some("allow-scripts allow-same-origin")).unwrap(),
        "about:blank"
    );
    assert_eq!(attrs(None), None);
    let plan = contract::compile(
        r#"component App
  state sandbox = "allow-scripts"
  view
    column
      iframe "data:text/html,a" sandbox=sandbox
      iframe "data:text/html,b" sandbox="allow-scripts allow-same-origin"
      iframe "https://e.dev/c"
"#,
    )
    .unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(js.contains(",\"src\",()=>\"data:text/html,a\")"), "{js}");
    assert!(!js.contains("()=>\"data:text/html,b\""), "{js}");
    assert!(!js.contains("()=>\"https://e.dev/c\""), "{js}");
    // Each frame grants its guest fullscreen (LLP 1020.002 D5).
    assert_eq!(js.matches(r#""allow":"fullscreen *""#).count(), 3, "{js}");
}

#[test]
fn a_pressed_nodes_computed_transition_is_checked_as_an_authors() {
    exact_kernel::style::link_all();
    // LLP 1081 D5, as `style`'s map, then `scale` moved to the press's
    // `--exact-scale`.
    let map = crate::style::transition_map(
        &crate::style::transition_entries(&crate::style::transition_spring()),
        true,
        false,
    );
    assert_eq!(
        crate::style::tests::run(
            &map,
            &[
                "-exact-tint-color 1s",
                "tint-color 1s, opacity 1s",
                "/**/--exact-tint 1s",
                "opacity 1s -exact-spring(1, 2, 3), scale 1s",
                "press-scale 1s",
                "animation-trigger 1s, --exact-π 1s",
            ]
        ),
        serde_json::json!([
            "--exact-tint 1s",
            "opacity 1s",
            "none",
            "--exact-scale 1s,scale 0s,--exact-scale 1s",
            "none",
            "none"
        ])
    );
    crate::style::tests::assert_spring_transitions(&map, true);
}

use crate::style::{
    color_map,
    tests::{role, run},
};

/// A bound `filter` (LLP 1095 D1) is written through the colour map:
/// the emitted binding wraps its value in `color_map(plan)`, a module
/// constant every colour site shares (`$cm`), which turns
/// a role into its CSS, an admitted `-exact-platform-color()` literal into its
/// fallback, and refuses one the plan does not hold.
#[test]
fn a_bound_filter_goes_through_the_colour_map() {
    let literal = "drop-shadow(0px 2px 4px -exact-platform-color(ios webJsFilterColor, #010203))";
    let source = format!(
        r#"component App
  state on = false
  action flip
    on = !on
  view
    column
      button press=flip testId="flip"
        text "Flip"
      text "Shadow" filter=(on ? "{literal}" : "drop-shadow(0px 2px 4px -exact-system-orange)")
"#
    );
    let plan = contract::compile(&source).unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    let map = color_map(&plan);
    assert!(
        map.contains("webJsFilterColor"),
        "the plan admits the literal"
    );
    assert!(
        js.contains(&format!("const $cm={map};\n")) && js.contains(",\"filter\",\"\",()=>mapped(("),
        "the bound filter is not mapped:\n{js}"
    );
    // One constant, however many sites; none where no site maps a colour.
    assert_eq!(js.matches("const $cm=").count(), 1);
    let plain = contract::compile(
        "component App\n  state on = false\n  view\n    text \"Plain\" opacity=(on ? 0.5 : 1)\n",
    )
    .unwrap();
    let plain = crate::emit::emit(&plain, false, false).unwrap().js;
    assert!(
        plain.contains("\"opacity\"") && !plain.contains("$cm"),
        "{plain}"
    );
    assert_eq!(
        run(
            &map,
            &[
                "drop-shadow(0px 2px 4px -exact-system-orange)",
                literal,
                "drop-shadow(0px 2px 4px -exact-platform-color(ios webJsOtherColor, #010203))",
            ]
        ),
        serde_json::json!([
            format!("drop-shadow(0px 2px 4px {})", role("system-orange")),
            "drop-shadow(0px 2px 4px #010203ff)",
            "unset",
        ])
    );
}

/// A grouped row whose flag is bound chooses between two metrics as it
/// runs (LLP 1115.002 D6): the page, which keeps no kernel, reads each
/// name as its value in the web's column, as the kernel reads a literal
/// one. A number takes the row's `px`, a role goes on through the colour
/// map, and a metric with no row on the web unsets the row.
#[test]
fn a_bound_metric_is_its_value_in_the_webs_column() {
    let plan = contract::compile(
        r#"component App
  state gone = false
  action flip
    gone = !gone
  view
    list appearance="auto" flex=1
      section
        button press=flip destructive=gone testId="row"
          if gone
            image "symbol:sf/trash"
          text "Remove"
"#,
    )
    .unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(
        !js.contains("\"-exact-grouped-") || js.contains("Object.hasOwn(M,v)?M[v]:v"),
        "a metric's name is bound and nothing reads it:\n{js}"
    );
    let named = |row: &str| {
        let binding = plan
            .bindings
            .iter()
            .find(|b| {
                b.kind == exact_plan::BindingKind::Style
                    && exact_kernel::StyleId::from_bit(b.id as u32)
                        .is_some_and(|id| id.name() == row)
                    && crate::style::literal(&plan, plan.code(b.expr)).is_none()
                    && crate::style::metric_map(&plan, plan.code(b.expr)).is_some()
            })
            .unwrap_or_else(|| panic!("no bound {row} names a metric"));
        crate::style::metric_map(&plan, plan.code(binding.expr)).unwrap()
    };
    // The row's content starts at its inset, the row spanning its card
    // (LLP 1084 D7).
    assert_eq!(
        run(
            &named("padding_left"),
            &[
                "-exact-grouped-row-inset",
                "-exact-grouped-button-padding-start",
                "12px"
            ]
        ),
        serde_json::json!([16, 16, "12px"])
    );
    // Red when destructive, by its role; the label's colour is no metric.
    assert_eq!(
        run(
            &named("text_color"),
            &["-exact-grouped-destructive-color", "-exact-label"]
        ),
        serde_json::json!(["-exact-system-red", "-exact-label"])
    );
    // The Mac's push button's margin is no row on the web.
    assert_eq!(
        run(
            &named("margin_left"),
            &["-exact-grouped-button-margin-inline", "unset"]
        ),
        serde_json::json!([null, "unset"])
    );
}

/// A bound `column-count` of 0 is CSS `auto` (LLP 1093 §1), as css.rs
/// writes the row. The JS target must write that string: `setProperty`
/// rejects `"0"`, the inline declaration drops, a class rule stays, and
/// the journal records the refusal. Toggling 2 then 0 computes `auto`.
#[test]
fn a_bound_column_count_of_zero_computes_to_auto() {
    let plan = contract::compile(
        r#"component App
  state n = 2
  action auto
    n = 0
  view
    view column-count=n testId="flow"
      view
"#,
    )
    .unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    let marker = "\"column-count\",\"";
    let at = js
        .find(marker)
        .unwrap_or_else(|| panic!("no column-count write:\n{js}"));
    let rest = &js[at + marker.len()..];
    let (unit, after) = rest.split_once('"').expect("unit");
    let map = after.strip_prefix(",()=>mapped(").and_then(|mapped| {
        let (_, map) = mapped.split_once(")(),")?;
        map.split_once(",\"unset\")").map(|(m, _)| m.to_string())
    });
    let map_src = map.unwrap_or_else(|| "v=>v".into());
    let script = format!(
            "const map={map_src};const unit={unit:?};const write=v=>{{const m=map(v);return m==null?null:typeof m===\"number\"?m+unit:String(m)}};console.log(JSON.stringify([write(0),write(2),write(null)]))"
        );
    let bun = std::process::Command::new(std::env::var("BUN").unwrap_or_else(|_| "bun".into()))
        .args(["-e", &script])
        .output()
        .expect("bun");
    assert!(
        bun.status.success(),
        "{}",
        String::from_utf8_lossy(&bun.stderr)
    );
    let written: serde_json::Value = serde_json::from_slice(&bun.stdout).unwrap();
    assert_eq!(
        written,
        serde_json::json!(["auto", "2", null]),
        "column-count write unit={unit:?} map={map_src}\n{js}"
    );
    let dir = std::env::temp_dir().join(format!("exact-column-count-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("col.html");
    let cursor_writes =
        crate::style::style_writes(exact_kernel::StyleId::Cursor as u16, false, true).unwrap();
    let cursor_map = cursor_writes[0].map.unwrap();
    let html = format!(
        r#"<!doctype html><meta charset="utf-8"><style>#flow{{column-count:3;width:320px}}button{{cursor:default}}</style><div id="flow">column</div><div style="cursor:grab"><button id="cursor">Cursor</button></div><pre id="out"></pre><script>
const journal=[];globalThis.exact={{journal,now:()=>0}};
function css(e,prop,unit,v){{const t=v==null?null:typeof v==="number"?v+unit:String(v);if(t==null){{e.style.removeProperty(prop);return}}e.style.removeProperty(prop);e.style.setProperty(prop,t);if(!e.style.getPropertyValue(prop))journal.push(`unset ${{prop}}: ${{JSON.stringify(v)}} is not a value it takes`)}}
const map={map_src};const unit={unit:?};const flow=document.getElementById("flow");
const apply=v=>css(flow,"column-count",unit,map(v));
apply(2);const after2=getComputedStyle(flow).columnCount;const journal2=journal.slice();journal.length=0;
apply(0);const after0=getComputedStyle(flow).columnCount;
const journal0=journal.slice();journal.length=0;
const cursorMap={cursor_map},cursor=document.getElementById("cursor");
const cursors=["pointer","inherit","unset","not-a-cursor",null].map(v=>{{css(cursor,"cursor","",cursorMap(v));return getComputedStyle(cursor).cursor}});
document.getElementById("out").textContent=JSON.stringify({{after2,after0,journal2,journal0,inline:flow.style.getPropertyValue("column-count"),cursors,cursorJournal:journal}});
</script>"#
    );
    std::fs::write(&page, html).unwrap();
    // Use the same provisioned browser carrier as the app proofs. Chrome's
    // --dump-dom can stay alive indefinitely on Linux even after the script ran.
    let browser_script = format!(
        r#"
import {{ chromium }} from 'playwright-core';
const browser = await chromium.launch({{executablePath: process.env.CHROME, headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage']}});
try {{
  const page = await browser.newPage();
  await page.goto({});
  process.stdout.write(await page.locator('#out').textContent());
}} finally {{ await browser.close(); }}
"#,
        serde_json::to_string(&format!("file://{}", page.display())).unwrap()
    );
    let result = std::process::Command::new(std::env::var("BUN").unwrap_or_else(|_| "bun".into()))
        .args(["--eval", &browser_script])
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .expect("Bun runs the provisioned browser");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let computed: serde_json::Value =
        serde_json::from_slice(&result.stdout).expect("computed styles");
    assert_eq!(computed["after2"], "2", "{computed}");
    assert_eq!(computed["after0"], "auto", "{computed}");
    assert_eq!(computed["journal2"], serde_json::json!([]), "{computed}");
    assert_eq!(computed["journal0"], serde_json::json!([]), "{computed}");
    assert_eq!(computed["inline"], "auto", "{computed}");
    assert_eq!(
        computed["cursors"],
        serde_json::json!(["pointer", "grab", "grab", "grab", "default"]),
        "{computed}"
    );
    assert_eq!(
        computed["cursorJournal"].as_array().unwrap().len(),
        1,
        "{computed}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A computed corner given a list (`10px 20px`, `10px / 20px`) is
/// refused whole with a journal line, as the kernel refuses it; a calc
/// is one value.
#[test]
fn a_computed_corner_list_is_refused_as_the_kernel_refuses_it() {
    let plan = contract::compile(
            "component App\n  state r = \"10px 20px\"\n  view\n    view border-top-left-radius=r testId=\"c\"\n",
        )
        .unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(
        js.contains("unset border-top-left-radius: ") && js.contains(",say,"),
        "{js}"
    );
    let map = crate::style::corner_list(exact_kernel::StyleId::BorderRadiusTopLeft);
    let script = format!(
            "const journal=[];const say=l=>journal.push(l);const m={map};console.log(JSON.stringify({{out:[\"10px 20px\",\"10px/20px\",\"calc(10px + 2px)\",\"12px\",8,null].map(m),journal:journal.length}}))"
        );
    let bun = std::process::Command::new(std::env::var("BUN").unwrap_or_else(|_| "bun".into()))
        .args(["-e", &script])
        .output()
        .expect("bun");
    assert!(
        bun.status.success(),
        "{}",
        String::from_utf8_lossy(&bun.stderr)
    );
    let got: serde_json::Value = serde_json::from_slice(&bun.stdout).unwrap();
    assert_eq!(
        got,
        serde_json::json!({"out": ["unset", "unset", "calc(10px + 2px)", "12px", 8, null], "journal": 2})
    );
}
