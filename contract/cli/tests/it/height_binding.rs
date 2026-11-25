//! The sheet handle is an authored IDREF and a typed two-number release.

use crate::source::boxed;
use exact_kernel::{Kernel, PropId};
use exact_plan::{EventKind, Plan, Value};
use exact_runner::{agent, DataError, DataSource, Runner};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("unexpected data query: {name}")
    }
}

fn source(params: &str, handler: &str, prop: &str) -> String {
    format!(
        "component App\n  state sheetPx = 360\n  action snap({params})\n    sheetPx = 180\n  view\n    column id=\"sheet\" height=sheetPx box-sizing=\"border-box\"\n      column testId=\"handle\" heightDragFor={prop} heightrelease={handler}\n"
    )
}

#[test]
fn height_binding_compiles_bakes_roundtrips_and_exports_the_handler() {
    let src = r#"component App
  state sheetPx = 360
  action snap(height: number, velocity: number)
    sheetPx = 180
  view
    column id="sheet" testId="target" height=sheetPx box-sizing="border-box"
      column testId="header" heightDragFor="sheet" heightrelease=snap
      scroll testId="scroll" height=40 heightDragFor="sheet" heightrelease=snap
        text "Content" height=100
      column
        list testId="list" height=40 heightDragFor="sheet" heightrelease=snap
          text "Row" height=100
"#;
    let (plan, warnings) = contract::compile_warned(src);
    assert!(warnings.iter().all(|w| w.id != ELIGIBILITY), "{warnings:?}");
    let plan = plan.unwrap();
    let plan = contract::bake(plan, boxed(NoData)).unwrap();
    let plan = Plan::decode(&plan.encode()).unwrap();
    let runner = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let kernel = runner.kernel();
    let target = kernel.find_by_test_id("target")[0];
    let prop = PropId::from_name("heightDragFor").expect("schema prop");
    assert_eq!(prop as u16, 75);
    let event = EventKind::from_name("heightrelease").expect("plan event");
    assert_eq!(event as u8, 14);
    let tree: serde_json::Value = serde_json::from_str(&agent::tree(&runner)).unwrap();
    for tag in ["header", "scroll", "list"] {
        let key = kernel.find_by_test_id(tag)[0];
        let handle = kernel.node_by_key(key).unwrap();
        assert_eq!(kernel.height_drag_target(key), Some(target));
        assert_eq!(handle.props.str(prop), Some("sheet"));
        assert_eq!(runner.handlers_of(handle.id), vec![event]);
        assert!(tree["nodes"].as_array().unwrap().iter().any(|node| {
            node["props"]["testId"] == tag
                && node["props"]["heightDragFor"] == "sheet"
                && node["handlers"] == serde_json::json!(["heightrelease"])
        }));
    }
}

#[test]
fn release_requires_two_numeric_trailing_parameters_and_string_idref() {
    for (params, handler, prop, error) in [
        ("height: number", "snap", "\"sheet\"", "handler-arity"),
        (
            "height: number, velocity: number, extra: number",
            "snap",
            "\"sheet\"",
            "handler-arity",
        ),
        (
            "height: string, velocity: number",
            "snap",
            "\"sheet\"",
            "type-handler-payload",
        ),
        (
            "height: number, velocity: bool",
            "snap",
            "\"sheet\"",
            "type-handler-payload",
        ),
        ("height: number, velocity: number", "snap", "3", "attr-type"),
        (
            "height: number, velocity: number",
            "snap",
            "false",
            "attr-type",
        ),
    ] {
        let err = contract::compile(&source(params, handler, prop))
            .unwrap_err()
            .to_string();
        assert!(err.contains(error), "{params}/{prop}: {err}");
    }
    contract::compile(&source(
        "origin: string, height: number, velocity: number",
        "snap(\"drag\")",
        "\"sheet\"",
    ))
    .unwrap();
}

#[test]
fn child_action_props_are_rechecked_after_inlining() {
    for ty in ["number", "string"] {
        let src = format!(
            "component App\n  state count = 0\n  action snap(height: {ty}, velocity: number)\n    count = count + 1\n  view\n    column\n      Handle(release=snap)\ncomponent Handle\n  props\n    release: action\n  view\n    column heightDragFor=\"sheet\" heightrelease=release\n"
        );
        let result = contract::compile(&src);
        if ty == "number" {
            result.unwrap();
        } else {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("type-handler-payload"));
        }
    }
}

const ELIGIBILITY: &str = "lower-height-drag-ineligible";

fn eligibility_source(target: &str) -> String {
    source("height: number, velocity: number", "snap", "\"sheet\"")
        .replace("height=sheetPx box-sizing=\"border-box\"", target)
}

fn eligibility_warnings(src: &str) -> Vec<contract::CompileError> {
    let (plan, warnings) = contract::compile_warned(src);
    plan.unwrap_or_else(|e| panic!("{e}\n{src}"));
    warnings
        .into_iter()
        .filter(|w| w.id == ELIGIBILITY)
        .collect()
}

#[test]
fn height_drag_targets_name_their_box_sizing_and_height_repairs() {
    for (target, repair) in [
        ("height=200", "box-sizing=\"border-box\""),
        (
            "height=200 box-sizing=\"content-box\"",
            "box-sizing=\"border-box\"",
        ),
        ("height=\"50%\" box-sizing=\"border-box\"", "height=200"),
        ("height=\"auto\" box-sizing=\"border-box\"", "height=200"),
        ("box-sizing=\"border-box\"", "height=200"),
    ] {
        let src = eligibility_source(target);
        let warnings = eligibility_warnings(&src);
        assert_eq!(warnings.len(), 1, "{src}: {warnings:?}");
        let warning = &warnings[0];
        assert!(warning.is_warning());
        assert_eq!(
            contract_lower::warning_rank::rank(&warning.id, &warning.message),
            0
        );
        assert_eq!(warning.span.line, 6);
        assert!(
            warning
                .message
                .contains("`heightDragFor=\"sheet\"` cannot bind: target `sheet`"),
            "{warning}"
        );
        assert!(warning.message.contains(repair), "{warning}");
        let runner = Runner::boot(
            contract::compile(&src).unwrap(),
            boxed(NoData),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let handle = runner.kernel().find_by_test_id("handle")[0];
        assert_eq!(runner.kernel().height_drag_target(handle), None);
    }
    let repaired = eligibility_source("height=200 box-sizing=\"border-box\"");
    assert!(eligibility_warnings(&repaired).is_empty());
    let runner = Runner::boot(
        contract::compile(&repaired).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let handle = runner.kernel().find_by_test_id("handle")[0];
    assert!(runner.kernel().height_drag_target(handle).is_some());
}

#[test]
fn height_drag_warnings_follow_classes_and_leave_unknown_values_to_runtime() {
    let base = eligibility_source("height=200 class=Sheet");
    let content = format!("style Sheet\n  box-sizing=\"content-box\"\n{base}");
    assert_eq!(eligibility_warnings(&content).len(), 1);
    let border = content.replace("box-sizing=\"content-box\"", "box-sizing=\"border-box\"");
    assert!(eligibility_warnings(&border).is_empty());
    let own = content.replace("class=Sheet", "class=Sheet box-sizing=\"border-box\"");
    assert!(eligibility_warnings(&own).is_empty());
    let percent = border.replace("height=200", "height=\"50%\"");
    assert_eq!(eligibility_warnings(&percent).len(), 1);
    let both = eligibility_source("height=\"50%\"");
    assert_eq!(eligibility_warnings(&both).len(), 2);
    for target in [
        "height=0 box-sizing=\"border-box\"",
        "height=\"200px\" box-sizing=\"border-box\"",
        "height=sheetPx box-sizing=\"border-box\"",
        "height=200 box-sizing=(sheetPx > 100 ? \"content-box\" : \"border-box\")",
    ] {
        let src = eligibility_source(target);
        assert!(eligibility_warnings(&src).is_empty(), "{src}");
    }
    let styles =
        "style Good\n  box-sizing=\"border-box\"\nstyle Bad\n  box-sizing=\"content-box\"\n";
    let chosen = format!(
        "{styles}{}",
        eligibility_source("height=200 class=(sheetPx > 100 ? Good : Bad)")
    );
    assert!(eligibility_warnings(&chosen).is_empty());
    let dynamic_id = eligibility_source("height=200")
        .replace("id=\"sheet\"", "id=(sheetPx > 100 ? \"sheet\" : \"other\")");
    assert!(eligibility_warnings(&dynamic_id).is_empty());
    let duplicate = "component App\n  view\n    column id=\"sheet\"\n      column id=\"sheet\" height=200\n        box heightDragFor=\"sheet\"\n";
    assert!(eligibility_warnings(duplicate).is_empty());
    let inline = "component App\n  view\n    column id=\"sheet\" height=200\n      Handle()\ncomponent Handle\n  view\n    box heightDragFor=\"sheet\"\n";
    assert_eq!(eligibility_warnings(inline).len(), 1);
    let repeated = "component App\n  state show = true\n  view\n    column id=\"sheet\" height=200\n      if show\n        for n in [1, 2] key=n\n          box heightDragFor=\"sheet\"\n";
    assert_eq!(eligibility_warnings(repeated).len(), 1);
}

#[test]
fn height_drag_sheet_defaults_apply_to_rows_but_not_their_descendants() {
    for (body, repair) in [
        (
            "column id=\"sheet\" height=200\n          box testId=\"handle\" heightDragFor=\"sheet\"",
            None,
        ),
        (
            "column\n          column id=\"sheet\" height=200\n            box testId=\"handle\" heightDragFor=\"sheet\"",
            Some("box-sizing=\"border-box\""),
        ),
        (
            "column\n          column\n            column id=\"sheet\" height=200\n              box testId=\"handle\" heightDragFor=\"sheet\"",
            Some("box-sizing=\"border-box\""),
        ),
        (
            "column id=\"sheet\" box-sizing=\"border-box\"\n          box testId=\"handle\" heightDragFor=\"sheet\"",
            Some("height=200"),
        ),
        (
            "column id=\"sheet\"\n          box testId=\"handle\" heightDragFor=\"sheet\"",
            Some("height=200"),
        ),
    ] {
        let src = format!(
            "component App\n  view\n    list appearance=\"auto\"\n      section\n        {body}\n"
        );
        assert_eligibility_matches_kernel(&src, repair);
    }
    for (target, repair) in [("height=200", None), ("", Some("height=200"))] {
        let src = format!(
            "component App\n  state count = 0\n  action del()\n    count = count + 1\n  view\n    list\n      column id=\"sheet\" {target} delete=del\n        box testId=\"handle\" heightDragFor=\"sheet\"\n"
        );
        assert_eligibility_matches_kernel(&src, repair);
    }
    for content in [
        "column id=\"sheet\" TARGET\n            box testId=\"handle\" heightDragFor=\"sheet\"",
        "row\n            column id=\"sheet\" TARGET\n              box testId=\"handle\" heightDragFor=\"sheet\"",
        "if show\n            column id=\"sheet\" TARGET\n              box testId=\"handle\" heightDragFor=\"sheet\"\n          else\n            text \"Empty\"",
        "match item\n            case some(n)\n              column id=\"sheet\" TARGET\n                box testId=\"handle\" heightDragFor=\"sheet\"\n            case none\n              text \"Empty\"",
    ] {
        for (target, repair) in [
            ("height=200", None),
            ("", Some("height=200")),
            ("height=200 box-sizing=\"content-box\"", Some("box-sizing=\"border-box\"")),
        ] {
            let content = content.replace("TARGET", target);
            let src = format!(
                "component App\n  state show = true\n  state item: option<number> = some(1)\n  view\n    list appearance=\"auto\"\n      section\n        scroll swipeContent=\"sheet\" swipeTrailing=\"act\" height=220 overflow-x=\"scroll\" overflow-y=\"hidden\" scroll-snap-type=\"x mandatory\"\n          {content}\n          button id=\"act\"\n            text \"Delete\"\n"
            );
            assert_eligibility_matches_kernel(&src, repair);
        }
    }
}

fn assert_eligibility_matches_kernel(src: &str, repair: Option<&str>) {
    let warnings = eligibility_warnings(src);
    assert_eq!(
        warnings.len(),
        usize::from(repair.is_some()),
        "{src}: {warnings:?}"
    );
    if let Some(repair) = repair {
        assert!(warnings[0].message.contains(repair), "{warnings:?}");
    }
    let runner = Runner::boot(
        contract::compile(src).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let handle = runner.kernel().find_by_test_id("handle")[0];
    assert_eq!(
        runner.kernel().height_drag_target(handle).is_some(),
        repair.is_none(),
        "{src}"
    );
}

#[test]
fn height_drag_reuses_the_path_visibility_and_enabled_checks() {
    let base = "component App\n  view\n    column\n      column id=\"sheet\" height=200 box-sizing=\"border-box\"\n        column\n          box testId=\"handle\" heightDragFor=\"sheet\"\n";
    for element in [
        "    column",
        "      column id=\"sheet\"",
        "        column",
        "          box testId=\"handle\"",
    ] {
        for attr in ["display=\"none\"", "inert=true", "disabled=true"] {
            let src = base.replacen(&format!("\n{element}"), &format!("\n{element} {attr}"), 1);
            assert_eligibility_matches_kernel(&src, Some(attr));
        }
    }
    let warnings = eligibility_warnings(&base.replacen("    column", "    column inert=true", 1));
    assert!(warnings[0]
        .message
        .contains("`column` on the handle's path"));
    assert!(!warnings[0].message.contains("clip"));
}

#[test]
fn height_drag_delete_disabled_belongs_to_the_action() {
    let indent = |text: &str, spaces: usize| {
        text.lines()
            .map(|line| format!("{}{line}\n", " ".repeat(spaces)))
            .collect::<String>()
    };
    for grouped in [false, true] {
        for (body, handle_row) in [
            ("column id=\"sheet\" height=200 delete=del disabled=true\n  box testId=\"handle\" heightDragFor=\"sheet\"", false),
            ("column delete=del disabled=true\n  column id=\"sheet\" height=200 box-sizing=\"border-box\"\n    box testId=\"handle\" heightDragFor=\"sheet\"", false),
            ("column testId=\"handle\" heightDragFor=\"sheet\" delete=del disabled=true", true),
        ] {
            let list = if grouped {
                format!("list appearance=\"auto\"\n  section\n{}", indent(body, 4))
            } else {
                format!("list\n{}", indent(body, 2))
            };
            let view = if handle_row {
                format!("column id=\"sheet\" height=200 box-sizing=\"border-box\"\n{}", indent(&list, 2))
            } else {
                list
            };
            let src = format!(
                "component App\n  state count = 0\n  action del()\n    count = count + 1\n  view\n{}", indent(&view, 4)
            );
            assert_eligibility_matches_kernel(&src, None);
            for attr in ["inert=true", "display=\"none\""] {
                let blocked = src.replace("disabled=true", attr);
                assert_eligibility_matches_kernel(&blocked, Some(attr));
            }
        }
    }
    // A stray delete is dropped without moving disabled off its element.
    let stray =
        eligibility_source("height=200 box-sizing=\"border-box\" delete=snap(0, 0) disabled=true");
    assert_eligibility_matches_kernel(&stray, Some("disabled=true"));
    let grouped = "component App\n  view\n    list appearance=\"auto\"\n      section\n        column id=\"sheet\" height=200\n          box testId=\"handle\" heightDragFor=\"sheet\"\n";
    for ancestor in ["list appearance=\"auto\"", "section"] {
        let blocked = grouped.replace(ancestor, &format!("{ancestor} disabled=true"));
        assert_eligibility_matches_kernel(&blocked, Some("disabled=true"));
    }
}
