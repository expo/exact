use crate::source::{boxed, NoData, Source};
use crate::warnings::warned;

const ID: &str = "lower-border-unstyled";

#[test]
fn a_border_without_a_line_style_names_the_repair() {
    let src = "component App\n  view\n    column padding=16 border-width=1 border-color=\"#d4d4d8\" border-radius=12\n      text \"A card\"\n";
    assert_eq!(
        warned(src)
            .into_iter()
            .filter(|(id, _)| id == ID)
            .collect::<Vec<_>>(),
        [(ID.to_string(), 3)]
    );
}

fn flagged(src: &str) -> Vec<String> {
    let (plan, warnings) = contract::compile_warned(src);
    plan.unwrap_or_else(|e| panic!("{src}: {e}"));
    warnings
        .into_iter()
        .filter(|w| w.id == ID)
        .map(|w| w.message)
        .collect()
}

fn app(attrs: &str) -> String {
    format!("component App\n  state on = false\n  state line = \"none\"\n  view\n    column {attrs}\n      text \"Card\"\n")
}

#[test]
fn sides_shorthands_and_colours_name_only_the_affected_edges() {
    for (attrs, repair) in [
        ("border=\"2px\"", "border-style=\"solid\""),
        ("border-width=(on ? 1 : 0)", "border-style=\"solid\""),
        ("border-width=\"1px\"", "border-style=\"solid\""),
        ("border-width=\"-exact-hairline\"", "border-style=\"solid\""),
        (
            "border-top-width=1 border-top-color=\"-exact-separator\"",
            "border-top-style=\"solid\"",
        ),
        (
            "border-width=\"0 0 2px 0\"",
            "border-bottom-style=\"solid\"",
        ),
        ("border-left=\"2pt\"", "border-left-style=\"solid\""),
        (
            "border-width=\"1px 0 2px 0\" border-color=\"#ccc\"",
            "border-top-style=\"solid\"` and `border-bottom-style=\"solid\"",
        ),
        ("border=(on ? \"1px\" : \"none\")", "border-style=\"solid\""),
        (
            "border-top-style=\"solid\" border-top=\"1px\"",
            "border-top-style=\"solid\"",
        ),
    ] {
        let messages = flagged(&app(attrs));
        assert_eq!(messages.len(), 1, "{attrs}: {messages:?}");
        assert!(
            messages[0].ends_with(&format!("write `{repair}`")),
            "{attrs}: {messages:?}"
        );
    }
}

#[test]
fn zero_widths_visible_styles_and_unknown_styles_stay_quiet() {
    for attrs in [
        "border-width=0",
        "border-color=\"\"",
        "border-color=(on ? \"\" : \"unset\")",
        "border-width=\"0px\" border-color=\"-exact-separator\"",
        "border=\"0\"",
        "border-width=1 border-style=\"solid\"",
        "border-width=1 border-style=\"hidden\"",
        "border-width=1 border-style=(on ? \"none\" : \"solid\")",
        "border-width=1 border-style=line",
        "border-top-width=1 border-top-style=\"dashed\"",
        "border=\"2px solid\" border-width=0",
        "border-top-width=1 border=\"0\"",
        "border-radius=8",
        "border-color=\"-exact-separator\" border-width=\"0 0 0 0\"",
        "border=(on ? \"0px\" : \"1px none\")",
    ] {
        assert!(flagged(&app(attrs)).is_empty(), "{attrs}");
    }
    for tag in ["header", "nav", "section"] {
        let src = format!(
            "component App\n  view\n    column\n      {tag} border-width=1 width=20 height=20\n"
        );
        assert!(flagged(&src).is_empty(), "{tag}");
    }
}

#[test]
fn classes_and_state_choices_follow_the_cascade() {
    for (a, b, own, expected) in [
        ("border-width=1", "border-width=2", "", 1),
        ("border-width=0", "opacity=1", "", 0),
        ("border-width=1", "opacity=1", "", 1),
        ("border-width=1", "border=\"2px solid\"", "", 1),
        (
            "border-style=\"solid\"",
            "border-style=\"none\"",
            "border-width=1",
            0,
        ),
        (
            "border-top=\"1px solid\"",
            "border-top-style=\"solid\"",
            "border-top-width=2",
            0,
        ),
        (
            "border-style=\"solid\"",
            "border-style=\"solid\"",
            "border=\"2px\"",
            1,
        ),
        (
            "border-width=1",
            "border-width=2",
            "border-style=(on ? \"none\" : \"solid\")",
            0,
        ),
        ("border-color=\"-exact-separator\"", "opacity=1", "", 0),
    ] {
        let src = format!("style A\n  {a}\nstyle B\n  {b}\ncomponent App\n  state on = false\n  view\n    column class=(on ? A : B) {own}\n");
        assert_eq!(flagged(&src).len(), expected, "{src}");
    }
    for (style, own, expected) in [
        ("border-width=1", "", 1),
        ("border-width=1 border-style=\"solid\"", "", 0),
        ("border=\"1px solid\"", "border-width=2", 0),
        ("border=\"1px solid\"", "border=\"2px\"", 1),
        ("border-color=\"-exact-separator\"", "border-width=0", 0),
    ] {
        let src =
            format!("style Card\n  {style}\ncomponent App\n  view\n    column class=Card {own}\n");
        assert_eq!(flagged(&src).len(), expected, "{src}");
    }
}

#[test]
fn the_warning_keeps_css_border_widths_and_the_repair_draws_only_its_side() {
    use exact_kernel::Kernel;
    use exact_runner::{DataError, DataSource, Runner, Value};
    struct NoData;
    impl DataSource for NoData {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
    }
    for (attrs, widths) in [
        ("border-width=1 border-color=\"-exact-separator\"", [0.0; 4]),
        ("border-width=1 border-style=\"solid\"", [1.0; 4]),
        ("border-top-width=1", [0.0; 4]),
        ("border-bottom-width=1 border-color=\"#ccc\"", [0.0; 4]),
        (
            "border-bottom-width=1 border-color=\"#ccc\" border-bottom-style=\"solid\"",
            [0.0, 0.0, 1.0, 0.0],
        ),
        ("border-right-width=\"unset\"", [0.0; 4]),
        (
            "border-right-width=\"unset\" border-right-style=\"solid\"",
            [0.0, 3.0, 0.0, 0.0],
        ),
        (
            "border-top-width=1 border-top-style=\"solid\"",
            [1.0, 0.0, 0.0, 0.0],
        ),
    ] {
        let (plan, warnings) = contract::compile_warned(&app(attrs));
        assert_eq!(
            warnings.iter().filter(|w| w.id == ID).count(),
            usize::from(widths == [0.0; 4]),
            "{attrs}: {warnings:?}"
        );
        let plan = plan.unwrap();
        let r = Runner::boot(
            plan,
            boxed(NoData),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let k = r.kernel();
        assert_eq!(
            k.node(k.roots()[0]).unwrap().style.border_widths(),
            widths,
            "{attrs}"
        );
    }
}

#[test]
fn the_warning_is_not_crowded_out_by_naming_advice() {
    let images = "      image \"symbol:sf/plus\"\n".repeat(21);
    let src = format!("component App\n  view\n    column\n{images}      column border-width=1\n");
    let all = warned(&src);
    assert_eq!(all.len(), contract_lower::MAX_WARNINGS);
    assert_eq!(all[0].0, ID);
}

#[test]
fn colour_longhands_do_not_supply_an_authored_width() {
    for attrs in [
        "border-color=\"-exact-separator\"",
        "border-top-color=\"#ccc\"",
    ] {
        assert!(flagged(&app(attrs)).is_empty(), "{attrs}");
    }
}

#[test]
fn a_colour_on_every_side_does_not_widen_the_repair() {
    let messages = flagged(&app("border-bottom-width=1 border-color=\"#ccc\""));
    assert_eq!(messages.len(), 1);
    assert!(
        messages[0].ends_with("write `border-bottom-style=\"solid\"`"),
        "{messages:?}"
    );
}

#[test]
fn style_only_shorthands_are_deliberate() {
    for attrs in [
        "border=\"none\"",
        "border-top=\"none\"",
        "border=\"hidden\"",
    ] {
        assert!(flagged(&app(attrs)).is_empty(), "{attrs}");
    }
}

#[test]
fn explicit_none_is_never_repaired() {
    for attrs in [
        "border-width=1 border-style=\"solid solid none solid\"",
        "border-width=1 border-style=\"none\"",
        "border-width=1 border-style=(on ? \"none\" : \"none\")",
        "border-top=\"1px solid\" border-top-style=\"none\"",
        "border=\"2px none -exact-separator\" width=\"100%\"",
        "border-top-width=1 border-top=\"2px none\"",
    ] {
        assert!(flagged(&app(attrs)).is_empty(), "{attrs}");
    }
}

#[test]
fn positive_relative_widths_name_their_side() {
    for width in ["-exact-list-symbol-gap", "-exact-hairline"] {
        let attrs = format!("border-left-width=\"{width}\"");
        let messages = flagged(&app(&attrs));
        assert_eq!(messages.len(), 1, "{attrs}");
        assert!(messages[0].ends_with("write `border-left-style=\"solid\"`"));
        assert!(flagged(&app(&format!("{attrs} border-left-style=\"solid\""))).is_empty());
    }
    for unit in ["rem", "em", "vw", "vh", "vmin", "vmax"] {
        let attrs = format!("border-left-width=\"0.1{unit}\"");
        let messages = flagged(&app(&attrs));
        assert_eq!(messages.len(), 1, "{attrs}");
        assert!(
            messages[0].ends_with("write `border-left-style=\"solid\"`"),
            "{messages:?}"
        );
        assert!(flagged(&app(&format!("border-left-width=\"0{unit}\""))).is_empty());
        assert!(flagged(&app(&format!("{attrs} border-left-style=\"solid\""))).is_empty());
    }
}

#[test]
fn explicit_width_resets_use_the_initial_width() {
    for reset in ["unset", ""] {
        let attrs = format!("border-right-width=\"{reset}\"");
        let messages = flagged(&app(&attrs));
        assert_eq!(messages.len(), 1, "{attrs}");
        assert!(
            messages[0].ends_with("write `border-right-style=\"solid\"`"),
            "{messages:?}"
        );
    }
}

#[test]
fn class_width_and_style_are_paired_within_each_branch() {
    for (a, b, own, expected) in [
        ("border-width=1", "border=\"2px solid\"", "", 1),
        ("border-width=0", "opacity=1", "border-color=\"#ccc\"", 0),
        (
            "border-width=0",
            "border-width=\"unset\"",
            "border-color=\"#ccc\"",
            1,
        ),
        ("border-width=1 border-style=\"none\"", "opacity=1", "", 0),
        ("border-width=0", "border-style=\"solid\"", "", 0),
        ("border=\"2px\"", "border=\"none\"", "", 1),
        ("border=\"#ccc\"", "border-width=0", "", 1),
    ] {
        let src = format!("style A\n  {a}\nstyle B\n  {b}\ncomponent App\n  state on = false\n  view\n    column class=(on ? A : B) {own}\n");
        assert_eq!(flagged(&src).len(), expected, "{src}");
    }
}

#[test]
fn bare_controls_warn_and_native_controls_stay_quiet() {
    for tag in ["button", "input", "textarea"] {
        let child = if tag == "button" {
            "      text \"Button\"\n"
        } else {
            ""
        };
        for (attrs, expected) in [
            ("border-width=1 border-color=\"-exact-separator\"", 1),
            ("appearance=\"none\" border-width=1", 1),
            ("", 0),
            ("appearance=\"auto\"", 0),
            ("border-width=1 border-style=\"none\"", 0),
        ] {
            let src =
                format!("component App\n  view\n    {tag} {attrs} width=20 height=20\n{child}");
            assert_eq!(flagged(&src).len(), expected, "{src}");
        }
        let src = format!("style A\n  border-width=1\nstyle B\n  opacity=1\ncomponent App\n  state on = false\n  view\n    {tag} class=(on ? A : B) width=20 height=20\n{child}");
        assert_eq!(flagged(&src).len(), 1, "{src}");
    }
}

#[test]
fn colour_shorthands_reset_width_to_medium() {
    let src = format!(
        "style Card\n  border=\"1px solid\"\n{}",
        app("class=Card border=\"#ccc\"")
    );
    assert_eq!(flagged(&src).len(), 1);
    for (attrs, repair, widths) in [
        ("border=\"#ccc\"", "border-style=\"solid\"", [3.0; 4]),
        (
            "border-top=\"#ccc\"",
            "border-top-style=\"solid\"",
            [3.0, 0.0, 0.0, 0.0],
        ),
        (
            "border-width=1 border=\"#ccc\"",
            "border-style=\"solid\"",
            [3.0; 4],
        ),
        (
            "border-width=1 border-top=\"#ccc\"",
            "border-style=\"solid\"",
            [3.0, 1.0, 1.0, 1.0],
        ),
    ] {
        let src = app(attrs);
        let messages = flagged(&src);
        assert_eq!(messages.len(), 1, "{src}: {messages:?}");
        assert!(
            messages[0].ends_with(&format!("write `{repair}`")),
            "{messages:?}"
        );
        let fixed = app(&format!("{attrs} {repair} testId=\"border\""));
        assert!(flagged(&fixed).is_empty());
        assert_eq!(border_widths(&boot(&fixed)), widths);
    }
}

use exact_kernel::Kernel;
use exact_runner::{Event, Runner};
fn boot(src: &str) -> Runner<Source> {
    Runner::boot(
        contract::compile(src).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}
fn border_widths(r: &Runner<Source>) -> [f32; 4] {
    let k = r.kernel();
    k.node_by_key(k.find_by_test_id("border")[0])
        .unwrap()
        .style
        .border_widths()
}
fn flip(r: &mut Runner<Source>) {
    let k = r.kernel();
    let id = k.node_by_key(k.find_by_test_id("flip")[0]).unwrap().id;
    r.dispatch(id, Event::Press).unwrap();
}
fn class_app(a: &str, b: &str, own: &str) -> String {
    format!("style A\n  {a}\nstyle B\n  {b}\ncomponent App\n  state on = false\n  action flip\n    on = not on\n  view\n    column\n      button testId=\"flip\" press=flip\n        text \"Flip\"\n      column testId=\"border\" class=(on ? A : B) {own}\n")
}

#[test]
fn conditional_class_repairs_preserve_both_branches() {
    for (b, expected_no) in [
        ("border-top=\"2px none\"", [0.0; 4]),
        ("border-bottom-width=2", [0.0, 0.0, 2.0, 0.0]),
    ] {
        let mut src = class_app("border-top-width=1", b, "");
        let mut r = boot(&src);
        assert_eq!(border_widths(&r), [0.0; 4]);
        flip(&mut r);
        assert_eq!(border_widths(&r), [0.0; 4]);
        let messages = flagged(&src);
        assert_eq!(messages.len(), 1);
        // Apply each repair exactly where the diagnostic tells the author to.
        for repair in messages[0]
            .split_once("; write ")
            .unwrap()
            .1
            .split("; write ")
        {
            let (rows, target) = repair
                .split_once(" inside `style ")
                .expect("repair must name its class");
            let class = target.strip_suffix('`').unwrap();
            let rows = rows.replace('`', "").replace(" and ", " ");
            let declaration = format!("style {class}\n");
            let start = src.find(&declaration).unwrap() + declaration.len();
            let end = start + src[start..].find('\n').unwrap();
            src.insert_str(end, &format!(" {rows}"));
        }
        assert!(flagged(&src).is_empty(), "{src}");
        let mut r = boot(&src);
        assert_eq!(border_widths(&r), expected_no, "{src}");
        flip(&mut r);
        assert_eq!(border_widths(&r), [1.0, 0.0, 0.0, 0.0], "{src}");
        flip(&mut r);
        assert_eq!(border_widths(&r), expected_no, "{src}");
    }
}

#[test]
fn class_and_attribute_choices_share_their_condition() {
    for (a, b, own, expected) in [
        (
            "border-style=\"solid\"",
            "opacity=1",
            "border-width=(on ? 1 : 0)",
            0,
        ),
        (
            "opacity=1",
            "border-style=\"solid\"",
            "border-width=(on ? 0 : 1)",
            0,
        ),
        (
            "border-style=\"solid\"",
            "opacity=1",
            "border-width=(on ? 0 : 1)",
            1,
        ),
        (
            "border=\"2px solid\"",
            "opacity=1",
            "border-width=(on ? 1 : 0) border-color=(on ? \"red\" : \"blue\")",
            0,
        ),
    ] {
        let src = class_app(a, b, own);
        assert_eq!(flagged(&src).len(), expected, "{src}");
    }
    let src = class_app(
        "border-style=\"solid\"",
        "opacity=1",
        "border-width=(on ? 1 : 0)",
    );
    let mut r = boot(&src);
    assert_eq!(border_widths(&r), [0.0; 4]);
    flip(&mut r);
    assert_eq!(border_widths(&r), [1.0; 4]);
    flip(&mut r);
    assert_eq!(border_widths(&r), [0.0; 4]);
}
