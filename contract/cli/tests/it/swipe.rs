//! `swipe` (LLP 1057.001 §8): the platform's discrete swipe in the one
//! direction `swipe-direction` names, host kind 66 with no payload.

use crate::source::{boxed, Source};
use exact_kernel::{Kernel, PropId};
use exact_plan::{EventKind, Plan};
use exact_runner::{Event, Runner};

const APP: &str = r#"component App
  state seen = 0
  state way = "down"
  action swiped
    seen = seen + 1
  view
    column
      text `${seen}` testId="seen"
      column swipe=swiped swipe-direction="down" testId="bar"
        textarea testId="field"
      box swipe=swiped swipe-direction=way testId="computed"
        text "a"
      column testId="plain"
"#;

fn boot() -> Runner<Source> {
    let plan = contract::compile(APP).unwrap_or_else(|e| panic!("{e}"));
    let plan = Plan::decode(&plan.encode()).unwrap();
    Runner::boot(
        plan,
        boxed(()),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn view(r: &Runner<Source>, id: &str) -> exact_kernel::ViewId {
    let key = r.kernel().find_by_test_id(id)[0];
    r.kernel().node_by_key(key).unwrap().id
}

fn prop<'a>(r: &'a Runner<Source>, id: &str, prop: PropId) -> Option<&'a str> {
    r.kernel().node(view(r, id)).unwrap().props.str(prop)
}

#[test]
fn a_swipe_runs_its_action_and_carries_its_direction_to_the_host() {
    let mut r = boot();
    let bar = view(&r, "bar");
    assert_eq!(r.handlers_of(bar), vec![EventKind::Swipe]);
    assert_eq!(prop(&r, "bar", PropId::SwipeDirection), Some("down"));
    assert_eq!(prop(&r, "computed", PropId::SwipeDirection), Some("down"));
    r.dispatch(bar, Event::of_host_kind(66, "").unwrap())
        .unwrap();
    assert_eq!(prop(&r, "seen", PropId::Text), Some("1"));
    let plain = view(&r, "plain");
    assert!(
        r.dispatch(plain, Event::Swipe).is_err(),
        "a node without `swipe` hears none"
    );
    assert_eq!(prop(&r, "seen", PropId::Text), Some("1"));
}

#[test]
fn a_swipe_needs_one_of_the_four_directions() {
    let refused = |source: &str| contract::compile(source).unwrap_err().to_string();
    let missing = APP.replace(
        r#" swipe-direction="down" testId="bar""#,
        r#" testId="bar""#,
    );
    assert!(
        refused(&missing).contains("lower-swipe-direction"),
        "{}",
        refused(&missing)
    );
    for word in ["sideways", "down up", "Down"] {
        let wrong = APP.replace(
            r#"swipe-direction="down""#,
            &format!(r#"swipe-direction="{word}""#),
        );
        assert!(
            refused(&wrong).contains("lower-swipe-direction"),
            "{word}: {}",
            refused(&wrong)
        );
    }
}

#[test]
fn a_run_inside_a_paragraph_takes_no_swipe() {
    let run = APP.replace(
        "      text `${seen}` testId=\"seen\"\n",
        "      text `${seen}` testId=\"seen\"\n        text \"more\" swipe=swiped swipe-direction=\"down\"\n",
    );
    let error = contract::compile(&run).unwrap_err().to_string();
    assert!(error.contains("a run inside a paragraph"), "{error}");
}

/// A test file's `tap "id" swipe <direction> [at x y]` (LLP 1057.001 §8.3).
#[test]
fn a_test_swipes_in_a_direction() {
    use contract_syntax::{Step, TapForm};
    let src = "test \"keys\"\n  tap \"composer-bar\" swipe down\n  tap \"composer-bar\" swipe left at 4 20\n";
    let tests = contract::tests(src).unwrap();
    assert!(matches!(
        &tests[0].steps[0],
        Step::Tap { form: TapForm::Swipe { direction, at: None }, .. } if direction == "down"
    ));
    assert!(matches!(
        &tests[0].steps[1],
        Step::Tap { form: TapForm::Swipe { direction, at: Some((x, y)) }, .. } if direction == "left" && *x == 4.0 && *y == 20.0
    ));
    // The steps' JSON, which `scripts/agent-test.mjs` reads as `form` and `direction`
    let json: serde_json::Value = serde_json::from_str(&contract::tests_json(&tests)).unwrap();
    let steps = json[0]["steps"].as_array().unwrap();
    assert_eq!(
        (steps[0]["form"].as_str(), steps[0]["direction"].as_str()),
        (Some("swipe"), Some("down"))
    );
    assert_eq!(steps[1]["at"], serde_json::json!([4, 20]));
    for wrong in ["  tap \"bar\" swipe\n", "  tap \"bar\" swipe sideways\n"] {
        let e = contract::tests(&format!("test \"keys\"\n{wrong}")).unwrap_err();
        assert_eq!(e.id, "syntax-expected-step", "{wrong}");
    }
}

/// A swipe anywhere inside a grouped list, which UIKit draws itself, is
/// named; one on the list or outside it is not (LLP 1057.001 §8).
#[test]
fn a_swipe_a_grouped_section_or_footer_would_hide_is_named() {
    let list = |row: &str, footer: &str| {
        format!("component App\n  state n = 0\n  action swiped\n    n = n + 1\n  view\n    list appearance=\"auto\" listStyle=\"inset-grouped\"\n      section{row}\n        row\n          text \"Name\"\n        footer\n          text \"About the name\"{footer}\n")
    };
    let swipe = " swipe=swiped swipe-direction=\"left\"";
    let unseen = |src: &str| {
        super::warnings::warned(src)
            .into_iter()
            .filter(|(id, _)| id == "lower-swipe-unseen")
            .count()
    };
    assert_eq!(unseen(&list(swipe, "")), 1, "on the section");
    assert_eq!(unseen(&list("", swipe)), 1, "on the footer's text");
    assert_eq!(unseen(&list("", "")), 0);
    let classed = list("", swipe).replace(
        "    list appearance=\"auto\" listStyle=\"inset-grouped\"\n",
        "    list class=Grouped listStyle=\"inset-grouped\"\n",
    );
    let classed = format!("style Grouped\n  appearance=\"auto\"\n\n{classed}");
    assert_eq!(unseen(&classed), 1, "in a list its class groups");
    let backticked =
        list("", swipe).replace("listStyle=\"inset-grouped\"", "listStyle=`inset-grouped`");
    assert_eq!(unseen(&backticked), 1, "with its style in backticks");
    let appearance = list("", swipe).replace("appearance=\"auto\"", "appearance=`auto`");
    assert_eq!(unseen(&appearance), 1, "with its appearance in backticks");
    let interpolated = list("", swipe).replace("appearance=\"auto\"", "appearance=`au${\"to\"}`");
    assert_eq!(
        unseen(&interpolated),
        1,
        "with its appearance a literal interpolation"
    );
    let component = format!(
        "component App\n  view\n    list appearance=\"auto\" listStyle=\"inset-grouped\"\n      section\n        NameRow()\n        NameRow()\n\ncomponent NameRow\n  state n = 0\n  action swiped\n    n = n + 1\n  view\n    row{swipe}\n      text \"Name\"\n"
    );
    assert_eq!(
        unseen(&component),
        1,
        "in a component its caller puts in the list, named once"
    );
    let on_row = list("", "").replace("        row\n", &format!("        row{swipe}\n"));
    assert_eq!(unseen(&on_row), 1, "on a row");
    let on_text = list("", "").replace(
        "          text \"Name\"\n",
        &format!("          text \"Name\"{swipe}\n"),
    );
    assert_eq!(unseen(&on_text), 1, "inside a row");
    let on_list = list("", "").replace(
        "listStyle=\"inset-grouped\"\n",
        &format!("listStyle=\"inset-grouped\"{swipe}\n"),
    );
    assert_eq!(unseen(&on_list), 0, "on the list");
    let plain = list("", swipe).replace(" appearance=\"auto\" listStyle=\"inset-grouped\"", "");
    assert_eq!(unseen(&plain), 0, "in a list drawn as written");
}
