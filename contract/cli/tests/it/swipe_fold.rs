//! One `swipe` with exactly one of `swipe-flick` or `swipe-follow` (LLP
//! 1057.001 §8): what an element writes, what every backend makes of it, the
//! retired names' errors, and a native module's tag, which keeps main's
//! reading of every name.

use crate::source::{boxed, NoData, Source};
use exact_kernel::{Kernel, PropId};
use exact_plan::EventKind;
use exact_runner::Runner;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

/// An app whose column holds `view`, with `more` declarations after it.
fn app(view: &str, more: &str) -> String {
    format!("component App\n  state n = 0\n  action a\n    n = n + 1\n  action b\n    n = n + 2\n  view\n    column\n{view}{more}")
}

/// Every refusal of `src`, by id and message.
fn errors(src: &str) -> Vec<(String, String)> {
    match contract::compile_path_source_all(Path::new("app.contract"), src, false) {
        Ok(_) => Vec::new(),
        Err(all) => all.into_iter().map(|e| (e.id, e.message)).collect(),
    }
}

/// Every refusal of `src`, by id, line and starting column.
fn spans(src: &str) -> Vec<(String, u32, u32)> {
    match contract::compile_path_source_all(Path::new("app.contract"), src, false) {
        Ok(_) => Vec::new(),
        Err(all) => all
            .into_iter()
            .map(|e| (e.id, e.span.line, e.span.col))
            .collect(),
    }
}

fn ids(src: &str) -> Vec<String> {
    errors(src).into_iter().map(|(id, _)| id).collect()
}

/// The handler kinds of the node `id`, and its flick direction.
fn kinds(src: &str, id: &str) -> (Vec<EventKind>, Option<String>) {
    let plan = contract::compile(src).unwrap_or_else(|e| panic!("{e}\n{src}"));
    let r: Runner<Source> = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let key = r.kernel().find_by_test_id(id)[0];
    let view = r.kernel().node_by_key(key).unwrap().id;
    let direction = r
        .kernel()
        .node(view)
        .unwrap()
        .props
        .str(PropId::SwipeDirection);
    (r.handlers_of(view), direction.map(String::from))
}

#[test]
fn each_form_lowers_to_the_kind_the_plan_has_always_had() {
    let row = |attrs: &str| {
        app(
            &format!("      box testId=\"row\" {attrs}\n        text \"r\"\n"),
            "",
        )
    };
    assert_eq!(
        kinds(&row(r#"swipe=a swipe-follow="right""#), "row"),
        (vec![EventKind::Swiperight], None)
    );
    assert_eq!(
        kinds(&row(r#"swipe=a swipe-follow="left""#), "row"),
        (vec![EventKind::Swipeleft], None)
    );
    for parenthesized in [
        r#"swipe=a swipe-follow=("right")"#,
        "swipe=a swipe-follow=(\n          \"right\"\n        )",
    ] {
        assert_eq!(
            kinds(&row(parenthesized), "row").0,
            vec![EventKind::Swiperight],
            "parentheses the parser unwraps: {parenthesized}"
        );
    }
    assert_eq!(
        kinds(&row(r#"swipe=a swipe-flick="down""#), "row"),
        (vec![EventKind::Swipe], Some("down".into()))
    );
}

/// The follow selector is the handler's name: a quoted literal written on
/// the element, judged in every component's written view, used or not.
#[test]
fn the_follow_selector_is_a_literal_written_on_the_element() {
    let refused = |src: &str| {
        let all = errors(src);
        assert!(
            all.iter()
                .any(|(id, m)| id == "lower-swipe-direction" && m.contains("two `if` branches")),
            "{all:?}\n{src}"
        );
    };
    for value in [
        "side",
        "`right`",
        "`${side}`",
        r#"(n > 0 ? "right" : "left")"#,
    ] {
        let body = format!("      Row(side=\"right\")\n");
        let comp = format!("\ncomponent Row\n  props\n    side: string\n  action a\n    let x = 1\n  view\n    box swipe=a swipe-follow={value}\n      text \"r\"\n");
        refused(&app(&body, &comp));
        let unused = comp.replace("component Row", "component Unused");
        refused(&app("      text \"plain\"\n", &unused));
    }
    for side in ["side: enum<\"left\", \"right\">"] {
        let comp = format!("\ncomponent Row\n  props\n    {side}\n  action a\n    let x = 1\n  view\n    box swipe=a swipe-follow=side\n      text \"r\"\n");
        refused(&app("      Row(side=\"right\")\n", &comp));
        refused(&app(
            "      text \"plain\"\n",
            &comp.replace("component Row", "component Unused"),
        ));
    }
}

#[test]
fn the_pair_and_the_flick_direction_are_judged_on_the_element() {
    let one = |attrs: &str, what: &str| {
        let all = errors(&app(
            &format!("      box {attrs}\n        text \"r\"\n"),
            "",
        ));
        let swipes: Vec<_> = all
            .iter()
            .filter(|(id, _)| id == "lower-swipe-direction")
            .collect();
        assert_eq!(swipes.len(), 1, "{what}: {all:?}");
    };
    one("swipe=a", "a missing choice");
    one(
        r#"swipe=a swipe-flick="down" swipe-follow="right""#,
        "a doubled choice",
    );
    one(r#"swipe-follow="right""#, "an orphan follow selector");
    one(r#"swipe-flick="down""#, "an orphan flick direction");
    one(
        r#"swipe=a swipe-flick="sideways""#,
        "an unknown flick direction",
    );
    let three = errors(&app("      box swipe=a swipe-flick=3\n", ""));
    assert!(
        three.iter().any(|(_, m)| m.contains("swipe-flick")),
        "{three:?}"
    );
    let head = errors(&app(
        "      head title=\"Inbox\" swipe=a swipe-follow=\"right\"\n",
        "",
    ));
    assert!(
        !head.is_empty(),
        "`head` takes no event, a swipe's included"
    );
    let run = |attrs: &str| {
        app(
            &format!("      text \"a\"\n        text \"b\" {attrs}\n"),
            "",
        )
    };
    assert!(
        errors(&run(r#"swipe=a swipe-follow="right""#)).is_empty(),
        "a following swipe on a run lowers"
    );
    assert!(
        errors(&run(r#"swipe=a swipe-flick="down""#))
            .iter()
            .any(|(_, m)| m.contains("a run inside a paragraph")),
        "a flick on a run has no view of its own"
    );
    let bare = errors(&run("swipe=a"));
    assert!(
        bare.iter()
            .all(|(_, m)| !m.contains("a run inside a paragraph")),
        "a run's swipe with no choice is told to choose, not to move: {bare:?}"
    );
}

/// Each retired name is refused where it is written: the three one-to-one
/// rewrites name their edit, and every other element states the rule. No
/// pair error joins either.
#[test]
fn a_retired_name_names_its_rewrite_or_the_rule() {
    let old = |attrs: &str, says: &str| {
        for unused in [false, true] {
            let element = format!("    box testId=\"row\" {attrs}\n      text \"r\"\n");
            let src = if unused {
                app("      text \"plain\"\n", &format!("\ncomponent Unused\n  action a\n    let x = 1\n  action b\n    let y = 1\n  view\n{element}"))
            } else {
                app(&format!("  {element}").replace("\n    ", "\n      "), "")
            };
            let all = errors(&src);
            assert!(
                all.iter()
                    .any(|(id, m)| id == "lower-unknown-attr" && m.contains(says)),
                "{attrs} (unused {unused}): {all:?}"
            );
            assert!(
                all.iter().all(|(id, _)| id != "lower-swipe-direction"),
                "{attrs}: no pair error joins: {all:?}"
            );
            // Each retired name once, at its own column
            let line = src.lines().position(|l| l.contains(attrs)).unwrap() as u32 + 1;
            let mut written: Vec<u32> = ["swiperight=", "swipeleft=", "swipe-direction="]
                .iter()
                .filter_map(|n| src.lines().nth(line as usize - 1).unwrap().find(n))
                .map(|c| c as u32 + 1) // a span's column counts from 1
                .collect();
            let mut reported: Vec<u32> = spans(&src)
                .into_iter()
                .filter(|(id, l, _)| id == "lower-unknown-attr" && *l == line)
                .map(|(_, _, c)| c)
                .collect();
            written.sort_unstable();
            reported.sort_unstable();
            assert_eq!(
                reported, written,
                "{attrs}: each retired name once, at itself: {all:?}"
            );
        }
    };
    old("swiperight=a", r#"swipe-follow="right""#);
    old("swipeleft=a", r#"swipe-follow="left""#);
    old(
        r#"swipe=a swipe-direction="down""#,
        "rename it `swipe-flick`",
    );
    for rest in [
        "swiperight=a swipeleft=b",
        r#"swipe=a swipe-direction="down" swiperight=b"#,
        r#"swipe-direction="down""#,
        r#"swiperight=a swipe-direction="down""#,
        r#"swiperight=a swipe-flick="down""#,
        r#"swiperight=a swipe-follow="right""#,
    ] {
        old(rest, "exactly one of `swipe-flick` or `swipe-follow`");
    }
    // Analysis still knows the name, and the fallback lint reports the rewrite in the same run
    let both = ids(&app("      box swiperight=\"x\"\n", ""));
    assert!(
        both.iter().any(|id| id == "analyze-handler-shape")
            && both.iter().any(|id| id == "lower-unknown-attr"),
        "{both:?}"
    );
}

#[test]
fn a_type_error_beside_a_follow_selector_reports_no_unknown_attribute() {
    let src = app(
        "      box swipe=a swipe-follow=\"right\"\n        text \"r\"\n",
        "",
    )
    .replace("    n = n + 1\n", "    n = \"text\"\n");
    let all = errors(&src);
    assert!(!all.is_empty());
    assert!(
        all.iter()
            .all(|(id, m)| !(id == "lower-unknown-attr" && m.contains("swipe-follow"))),
        "{all:?}"
    );
}

/// The plan backend and all three Lean forms read the pair as main read the
/// old names, and refuse alike what the written views refuse.
#[test]
fn every_backend_lowers_the_pair_as_main_lowered_the_old_names() {
    let comp = "\ncomponent Row\n  action a\n    let x = 1\n  view\n    box swipe=a swipe-follow=\"right\"\n      text \"r\"\n";
    let src = app(
        "      Row()\n      box swipe=a swipe-flick=\"down\"\n        text \"f\"\n",
        comp,
    );
    for (name, lean) in [
        ("lean", contract::lean::lean(&src, "fold").unwrap()),
        ("plain", contract::lean::lean_plain(&src, "fold").unwrap()),
        (
            "components",
            contract::lean::lean_components(&src, "fold").unwrap(),
        ),
    ] {
        assert!(lean.contains("\"swiperight\""), "{name}");
        assert!(
            lean.contains("(\"swipe\", \"a\""),
            "{name}: the flick's handler is `swipe`"
        );
        assert!(
            !lean.contains("swipe-follow") && !lean.contains("swipe-flick"),
            "{name}"
        );
        assert!(
            lean.contains("\"swipe-direction\""),
            "{name}: the flick's direction is main's prop"
        );
    }
    for unused in [
        "\ncomponent Unused\n  props\n    side: string\n  action a\n    let x = 1\n  view\n    box swipe=a swipe-follow=side\n",
        "\ncomponent Unused\n  action a\n    let x = 1\n  view\n    box swiperight=a\n",
    ] {
        let src = app("      text \"plain\"\n", unused);
        assert!(contract::compile(&src).is_err(), "{unused}");
        assert!(contract::lean::lean(&src, "fold").is_err(), "{unused}");
        assert!(contract::lean::lean_plain(&src, "fold").is_err(), "{unused}");
        assert!(contract::lean::lean_components(&src, "fold").is_err(), "{unused}");
    }
}

#[test]
fn the_table_keeps_main_rows_and_the_vocabulary_offers_only_the_pair() {
    use contract_lower::{tags, vocab};
    for name in ["swiperight", "swipeleft", "swipe", "swipe-direction"] {
        assert!(tags::attr(name).is_some(), "{name}: the plan's rows stay");
    }
    for name in ["swipe-flick", "swipe-follow"] {
        assert!(
            tags::attr(name).is_none() && vocab::compile_attr(name).is_some(),
            "{name}"
        );
    }
    let listed: Vec<_> = vocab::attrs().into_iter().map(|(n, _)| n).collect();
    let renamed: Vec<_> = vocab::renamed().into_iter().map(|(n, _)| n).collect();
    for name in ["swiperight", "swipeleft", "swipe-direction"] {
        assert!(!listed.contains(&name) && renamed.contains(&name), "{name}");
        for args in [
            &[name][..],
            &[name, "press"],
            &["--json", name],
            &["--json", name, "press"],
        ] {
            let out = Command::new(env!("CARGO_BIN_EXE_contract"))
                .arg("vocab")
                .args(args)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(1), "{args:?}");
            let said = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(said.contains("is retired"), "{args:?}: {said}");
            assert!(
                !said.contains("handler") || args.contains(&"press"),
                "{args:?}: no handler entry: {said}"
            );
        }
    }
    // The full listings name a retired word only in `renamed`
    let json = Command::new(env!("CARGO_BIN_EXE_contract"))
        .args(["vocab", "--json"])
        .output()
        .unwrap();
    let mut listing: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    listing.as_object_mut().unwrap().remove("renamed");
    let text = Command::new(env!("CARGO_BIN_EXE_contract"))
        .args(["vocab", "--all"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&text.stdout);
    for name in ["swiperight", "swipeleft", "swipe-direction"] {
        assert!(
            !listing.to_string().contains(&format!("\"{name}\"")),
            "{name} in the JSON listing"
        );
        assert!(
            !text
                .lines()
                .any(|l| l.trim_start().starts_with(&format!("{name} ")) && !l.contains(" -> ")),
            "{name}: an entry in the text listing"
        );
    }
    for name in ["swipe-flick", "swipe-follow"] {
        let out = Command::new(env!("CARGO_BIN_EXE_contract"))
            .args(["vocab", name])
            .output()
            .unwrap();
        assert!(
            out.status.success()
                && String::from_utf8_lossy(&out.stdout).contains("compile-time attribute"),
            "{name}"
        );
    }
    let near = errors(&app("      box swiperigt=a\n", ""));
    assert!(
        near.iter().all(|(_, m)| !m.contains("`swiperight`")),
        "no spelling offers a retired name: {near:?}"
    );
}

#[test]
fn a_flick_inside_a_grouped_list_warns_and_a_follow_does_not() {
    let list = |attrs: &str| {
        app(&format!("      list appearance=\"auto\" listStyle=\"inset-grouped\"\n        section\n          row {attrs}\n            text \"r\"\n"), "")
    };
    let unseen = |src: &str| {
        super::warnings::warned(src)
            .into_iter()
            .filter(|(id, _)| id == "lower-swipe-unseen")
            .count()
    };
    assert_eq!(unseen(&list(r#"swipe=a swipe-flick="left""#)), 1);
    assert_eq!(unseen(&list(r#"swipe=a swipe-follow="left""#)), 0);
}

/// A package whose views declare props and an event named after the swipe
/// words, written beside the app as an installed one.
struct Package(PathBuf);
impl Package {
    fn new(name: &str, app: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("exact-fold-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let at = path.join("node_modules/@fixture/pinboard");
        std::fs::create_dir_all(&at).unwrap();
        let interface = json!({
            "kind": "interface", "name": "@fixture/pinboard", "version": "1.0.0", "shapes": {}, "names": [],
            "views": {
                "pin-board": { "props": { "swiperight": "string", "swipe-direction": "string", "swipe-flick": "string" }, "events": [] },
                "tap-board": { "props": {}, "events": [{ "event": "swipe-flick", "args": [] }] }
            }
        });
        std::fs::write(at.join("package.json"), json!({ "name": "@fixture/pinboard", "version": "1.0.0", "exports": { ".": "./exact.json" } }).to_string()).unwrap();
        std::fs::write(at.join("exact.json"), interface.to_string()).unwrap();
        std::fs::write(
            path.join("package.json"),
            json!({ "name": "app", "dependencies": { "@fixture/pinboard": "1.0.0" } }).to_string(),
        )
        .unwrap();
        std::fs::write(path.join("app.contract"), app).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn ids(&self) -> Vec<String> {
        match contract::compile_path_all(&self.0.join("app.contract"), false) {
            Ok(_) => Vec::new(),
            Err(all) => all.into_iter().map(|e| e.id).collect(),
        }
    }
    fn boot(&self) -> Runner<Source> {
        let (plan, _) = contract::compile_path_all(&self.0.join("app.contract"), false)
            .unwrap_or_else(|e| panic!("{e:?}"));
        Runner::boot(
            plan,
            boxed(NoData),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap()
    }
}
impl Drop for Package {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// On a native module's tag every one of the six names reads as on main: the
/// retired three are its reserved words, and the pair are ordinary module
/// names, a declared prop or event or an app-local module's prop.
#[test]
fn a_module_tag_keeps_main_reading_of_every_name() {
    let view = |line: &str, more: &str| {
        let (top, actions) = more.split_once('|').unwrap_or(("", more));
        format!("{top}component App\n  state n = 0\n  action a\n    n = n + 1\n{actions}  view\n    column\n      {line}\n")
    };
    let ok = |line: &str, more: &str| {
        let p = Package::new("ok", &view(line, more));
        assert!(p.ids().is_empty(), "{line}: {:?}", p.ids());
    };
    // A function and an action of one name: main infers the action's `value` from the declared prop's use
    let fns = "fn cardText(label: string): string = label\n\n|  action cardText(value)\n    let ignored = value\n";
    ok("pin-board swiperight=cardText(\"manual\")", fns);
    ok("pin-board swipe-direction=\"down\"", "");
    ok("pin-board swipe-flick=\"manual\"", "");
    // A package's declared event of the name runs its handler
    let tap = Package::new("event", &view("tap-board swipe-flick=a testId=\"tap\"", ""));
    let mut r = tap.boot();
    let key = r.kernel().find_by_test_id("tap")[0];
    let id = r.kernel().node_by_key(key).unwrap().id;
    let sent = json!({ "exact:event": "swipe-flick", "args": [] });
    r.dispatch(id, exact_runner::Event::Message(sent.to_string()))
        .unwrap();
    assert_eq!(r.slot("n"), Some(&exact_runner::Value::Number(1.0)));
    let refused = |line: &str, id: &str| {
        let p = Package::new("refused", &view(line, ""));
        assert!(p.ids().iter().any(|i| i == id), "{line}: {:?}", p.ids());
    };
    refused("pin-board swipe-flick=42", "contract-view-prop");
    refused("tap-board swipe-follow=\"right\"", "lower-native-attr");
    refused("tap-board swipe-direction=\"down\"", "lower-native-attr");
    assert!(
        errors(&app(
            "      card-deck swipe-flick=\"manual\" swipe-follow=\"manual\"\n",
            ""
        ))
        .iter()
        .all(|(id, _)| !id.starts_with("lower-swipe") && id != "lower-unknown-attr"),
        "an app-local module takes the pair as its props"
    );
}

/// One error, and the one that fits: a run with both selectors is told it
/// has both, and an SVG shape's retired name gets the one error.
#[test]
fn a_run_with_both_selectors_and_an_svg_shape_get_the_error_that_fits() {
    let both = errors(&app(
        "      text \"a\"\n        text \"b\" swipe=a swipe-flick=\"down\" swipe-follow=\"right\"\n",
        "",
    ));
    assert!(
        both.iter()
            .all(|(_, m)| !m.contains("a run inside a paragraph")),
        "{both:?}"
    );
    assert!(
        both.iter()
            .any(|(id, m)| id == "lower-swipe-direction" && m.contains("not both")),
        "{both:?}"
    );
    let svg = errors(&app(
        "      svg width=10 height=10\n        rect width=10 height=10 swiperight=a\n",
        "",
    ));
    assert_eq!(svg.len(), 1, "one error at the retired name: {svg:?}");
    assert_eq!(svg[0].0, "lower-unknown-attr", "{svg:?}");
}

/// A literal flick direction is judged on the element as lowered, a prop's
/// included, and SVG content and a button's label get one error per
/// attribute.
#[test]
fn a_flick_literal_is_judged_where_lowered_and_svg_content_gets_one_error_per_attribute() {
    // A literal a prop passes is one of the four too, and an unused component's is not judged, as `swipe-direction`'s were
    let comp = "\ncomponent Row\n  props\n    side: string\n  action a\n    let x = 1\n  view\n    box swipe=a swipe-flick=side testId=\"row\"\n      text \"r\"\n";
    let passed = errors(&app("      Row(side=\"off\")\n", comp));
    assert!(
        passed.iter().any(|(id, _)| id == "lower-swipe-direction"),
        "{passed:?}"
    );
    let unused = comp
        .replace("swipe-flick=side", "swipe-flick=\"sideways\"")
        .replace("component Row", "component Unused");
    assert!(errors(&app("      text \"plain\"\n", &unused)).is_empty());
    let label = app("      button appearance=\"auto\" press=a\n        text \"Reply\" swipe-follow=\"up\" swipe=a\n", "");
    let at: Vec<_> = spans(&label).into_iter().map(|(_, l, c)| (l, c)).collect();
    let mut unique = at.clone();
    unique.dedup();
    assert_eq!(
        unique.len(),
        at.len(),
        "one error per attribute on a button's label: {:?}",
        errors(&label)
    );
    let text = errors(&app(
        "      svg width=10 height=10\n        text \"Q1\" swiperight=a\n",
        "",
    ));
    assert_eq!(text.len(), 1, "one error at the retired name: {text:?}");
    for shape in [
        r#"rect width=10 height=10 swipe-follow="up" swipe=a"#,
        "rect width=10 height=10 swipe=a",
    ] {
        let src = app(
            &format!("      svg width=10 height=10\n        {shape}\n"),
            "",
        );
        let at: Vec<_> = spans(&src).into_iter().map(|(_, l, c)| (l, c)).collect();
        let mut unique = at.clone();
        unique.dedup();
        if shape.contains("swipe-follow=\"up\"") {
            assert!(
                errors(&src)
                    .iter()
                    .any(|(id, _)| id == "lower-swipe-direction"),
                "the selector's own error: {:?}",
                errors(&src)
            );
        }
        assert!(
            !at.is_empty() && unique.len() == at.len(),
            "{shape}: {:?}",
            errors(&src)
        );
    }
}

/// A retired name's own error, which says what to write, is the one at its
/// attribute: a native button's check at the same place yields to it.
#[test]
fn a_retired_name_gets_one_error_on_a_native_button() {
    let src = app(
        "      button appearance=\"auto\" swiperight=a\n        text \"Reply\"\n",
        "",
    );
    let line = src.lines().position(|l| l.contains("swiperight")).unwrap() as u32 + 1;
    let col = src
        .lines()
        .nth(line as usize - 1)
        .unwrap()
        .find("swiperight")
        .unwrap() as u32
        + 1;
    let at: Vec<_> = spans(&src)
        .into_iter()
        .filter(|(_, l, c)| (*l, *c) == (line, col))
        .collect();
    assert_eq!(at.len(), 1, "{:?}", errors(&src));
    assert_eq!(at[0].0, "lower-unknown-attr");
}
