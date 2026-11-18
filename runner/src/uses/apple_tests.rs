use super::{apple_uses, Capability};
use crate::uses;
use exact_plan::{asm::Asm, builder::PlanBuilder, TypeKind};

fn plan(view: &str) -> exact_plan::Plan {
    let source = format!(
        "component A\n  state wide = true\n  action go\n    wide = false\n  view\n    {view}\n"
    );
    contract::compile(&source).unwrap_or_else(|e| panic!("{source}: {e:?}"))
}

/// A plan with one text node, and what `add` puts in its tables.
fn built(add: impl FnOnce(&mut PlanBuilder)) -> exact_plan::Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    add(&mut b);
    b.node(
        exact_kernel::NodeType::Text as u8,
        None,
        None,
        0,
        &[],
        &[],
        None,
    );
    b.finish().unwrap()
}

/// D10: Apple's `grid` covers every plan whose layout reaches the grid
/// algorithm; elsewhere it stays the grammars a grid row needs.
#[test]
fn apple_links_grid_layout_for_every_plan_that_lays_out_a_grid() {
    let column = |attrs: &str| format!("column {attrs}\n      text \"a\"");
    let table = |attrs: &str| format!("table{attrs}\n      tr\n        td\n          text \"a\"");
    let grid_rows = column("display=\"grid\" grid-template-columns=\"1fr 1fr\"");
    let flex_rows = column("display=\"flex\" justify-items=\"center\"");
    for (view, apple, everywhere) in [
        ("text \"a\"".to_string(), false, false),
        (column("display=\"flex\""), false, false),
        (column("display=\"none\""), false, false),
        // A computed value counts as a use of whatever it might select.
        (column("display=(wide ? \"block\" : \"flex\")"), true, false),
        (column("display=\"grid\""), true, false),
        ("text \"a\" display=\"grid\"".to_string(), true, false),
        (grid_rows, true, true),
        // A grid row on a box that is no grid reaches no grid layout.
        (flex_rows, false, true),
        (table(""), true, false),
        // A table that is a block holds an anonymous table, a grid too.
        (table(" display=\"block\""), true, false),
    ] {
        let plan = plan(&view);
        assert_eq!(apple_uses(&plan).has(Capability::Grid), apple, "{view}");
        assert_eq!(uses(&plan).has(Capability::Grid), everywhere, "{view}");
    }
}

/// D11: the module seam serves a package's modules, its views and its
/// events; each table alone selects it, and no other host's set moves.
#[test]
fn modules_are_a_package_module_view_or_event() {
    let plain = plan("text \"a\"");
    assert!(!apple_uses(&plain).has(Capability::Modules));
    let module = built(|b| {
        let ty = b.primitive(TypeKind::Number);
        let source = b.source("battery", &[], ty);
        b.module(
            source,
            ("battery", "@fixture/battery"),
            "{}",
            false,
            false,
            "",
            "",
            "",
        );
    });
    let event = built(|b| {
        let body = b.code(Asm::new());
        let action = b.action("hear", &[], &[], body);
        b.module_event(("bell", "@fixture/bell"), "chime", action, "emit", 4, 0);
    });
    let mut view = plan("text \"a\"");
    view.add_view("swatch-box", ("swatch", "@fixture/swatch"), true, &[], "");
    for plan in [&module, &event, &view] {
        assert!(apple_uses(plan).has(Capability::Modules));
        assert!(!uses(plan).has(Capability::Modules));
    }
}

/// D11: the tool seam serves a plan with a `tool` line.
#[test]
fn tools_are_a_plan_with_a_tool() {
    assert!(!apple_uses(&plan("text \"a\"")).has(Capability::Tools));
    let source = "component A\n  state n = 0\n  tool bump writes \"Add one.\" = bump\n  action bump\n    n = n + 1\n  view\n    text \"a\"\n";
    let tooled = contract::compile(source).unwrap();
    assert!(apple_uses(&tooled).has(Capability::Tools));
    assert!(!uses(&tooled).has(Capability::Tools));
}

/// D10: each native roster entry is selected by the rows that reach its
/// family's code, a computed value by whatever it can be, and a plain text
/// and button plan by none; no other host's set moves.
#[test]
fn each_native_entry_is_selected_by_the_rows_that_reach_its_family() {
    use Capability::{Collections, Controls, Effects, Images, NativeViews, Svg, Traces};
    let plain = "column\n      text \"a\"\n      button \"b\" press=go";
    for (view, capability, selected) in [
        (plain, Svg, false),
        (plain, Images, false),
        (plain, NativeViews, false),
        (plain, Controls, false),
        (plain, Traces, false),
        (plain, Collections, false),
        (
            "svg viewBox=\"0 0 10 10\"\n      circle cx=5 cy=5 r=4",
            Svg,
            true,
        ),
        // A box's filter runs on box effects' engine, which SVG's uses.
        ("text \"a\" filter=\"blur(2px)\"", Svg, false),
        ("text \"a\" filter=\"blur(2px)\"", Effects, true),
        ("text \"a\" filter=(wide ? \"none\" : \"blur(2px)\")", Effects, true),
        // `clip-path` is the core's mask on Apple.
        ("text \"a\" clip-path=\"inset(1px)\"", Effects, false),
        (plain, Effects, false),
        // The host tells an SVG by its bytes: a computed source can be one,
        // and so can a download; a bundled file is the bake's to read.
        (
            "image (wide ? \"assets/a.png\" : \"assets/b.png\")",
            Svg,
            true,
        ),
        ("image \"https://example.test/a.png\"", Svg, true),
        ("image \"assets/a.png\"", Svg, false),
        ("image \"assets/a.png\"", Images, true),
        ("image \"symbol:star\"", Images, false),
        // The loader fails an empty source, which an `error` handler hears.
        ("image \"\"", Images, true),
        ("image \"symbol:star\"", Svg, false),
        ("view hatch=\"avatar\"", NativeViews, true),
        ("swatch-box", NativeViews, true),
        ("column\n      action-button press=go", NativeViews, true),
        ("button \"b\" press=go appearance=\"none\"", Controls, false),
        ("input type=\"checkbox\"", Controls, true),
        ("input type=\"checkbox\" role=\"switch\"", Controls, true),
        ("input type=\"range\"", Controls, true),
        ("progress value=0.5", Controls, true),
        ("input type=\"number\"", Controls, true),
        ("input type=\"text\"", Controls, false),
        (
            "row role=\"tablist\"\n      button \"a\" press=go",
            Controls,
            true,
        ),
        ("row role=(wide ? \"tablist\" : \"group\")", Controls, true),
        ("row role=\"group\"", Controls, false),
        (
            "list\n      row delete=go\n        text \"a\"",
            Collections,
            true,
        ),
        (
            "scroll swipeContent=\"r\" overflow-x=\"scroll\" height=60\n      row id=\"r\"\n        text \"a\"",
            Collections,
            true,
        ),
        // A list whose rows are cells; the author's own rows are not.
        ("list\n      row\n        text \"a\"", Collections, true),
        (
            "list appearance=\"none\"\n      row\n        text \"a\"",
            Collections,
            false,
        ),
    ] {
        let plan = plan(view);
        assert_eq!(
            apple_uses(&plan).has(capability),
            selected,
            "{view}: {capability:?}"
        );
        if !matches!(capability, Collections | Effects) {
            assert!(!uses(&plan).has(capability), "{view}: {capability:?}");
        }
    }
    let traced = contract::compile(
        "component A\n  action go\n    startTrace_EXPERIMENTAL()\n  view\n    button \"b\" press=go\n",
    )
    .unwrap();
    assert!(apple_uses(&traced).has(Traces));
}

/// A literal source's file ships with the app, so the bake reads it; a
/// download's and an app file's bytes come at run time.
#[test]
fn a_bundled_image_source_is_the_bakes_to_read() {
    use super::ImageSource::{Computed, Literal};
    for (source, bundled) in [
        (Literal("assets/a.png"), Some("assets/a.png")),
        (
            Literal("data:image/png;base64,AAAA"),
            Some("data:image/png;base64,AAAA"),
        ),
        (Literal("https://example.test/a.svg"), None),
        (Literal("app:/photos/1"), None),
        (Computed, None),
    ] {
        assert_eq!(source.bundled(), bundled);
        assert_eq!(source.can_be_svg(), bundled.is_none());
    }
}
