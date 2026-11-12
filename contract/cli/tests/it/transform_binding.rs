//! Fixed photo-transform authoring: IDREF, four geometry and six release numbers.
use exact_kernel::{Kernel, PropId};
use exact_plan::{EventKind, Plan, Value};
use exact_runner::{agent, DataError, DataSource, Runner};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("unexpected query: {name}")
    }
}

/// The handle with `event` bound to `handler`, and its partner (a transform
/// drag needs both, LLP 1057.003 C6) bound to a well-formed `partner`.
fn source(event: &str, params: &str, handler: &str, idref: &str) -> String {
    let (other, other_params) = partner(event);
    format!("component App\n  state value = 0\n  action receive({params})\n    value = 1\n  action partner({other_params})\n    value = 2\n  view\n    column overflow=\"hidden\"\n      column id=\"photo\" width=\"100%\" height=\"100%\" box-sizing=\"border-box\"\n        column testId=\"handle\" transformDragFor={idref} {event}={handler} {other}=partner\n")
}
fn partner(event: &str) -> (&'static str, &'static str) {
    if event == "transformgeometry" {
        ("transformrelease", RELEASE)
    } else {
        ("transformgeometry", GEOMETRY)
    }
}

const GEOMETRY: &str = "bw: number, bh: number, pw: number, ph: number";
const RELEASE: &str = "x: number, y: number, scale: number, vx: number, vy: number, vs: number";

#[test]
fn an_ineligible_transform_target_is_named_with_its_repair() {
    let src = source("transformgeometry", GEOMETRY, "receive", "\"photo\"")
        .replace(" box-sizing=\"border-box\"", "");
    let (plan, warnings) = contract::compile_warned(&src);
    let runner = Runner::boot(
        plan.unwrap(),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let handle = runner.kernel().find_by_test_id("handle")[0];
    assert_eq!(runner.kernel().transform_drag_binding(handle), None);
    let warning = warnings
        .iter()
        .find(|w| w.id == "lower-transform-drag-ineligible")
        .unwrap_or_else(|| panic!("missing transform eligibility warning: {warnings:?}"));
    assert!(warning.is_warning());
    assert_eq!(warning.span.line, 9);
    assert!(warning.message.contains("target `photo`"), "{warning}");
    assert!(
        warning.message.contains("box-sizing=\"border-box\""),
        "{warning}"
    );
}

fn eligibility_source(clip: &str, target: &str) -> String {
    source("transformgeometry", GEOMETRY, "receive", "\"photo\"")
        .replace(
            "column overflow=\"hidden\"",
            &format!("column id=\"viewport\" {clip}"),
        )
        .replace(
            "width=\"100%\" height=\"100%\" box-sizing=\"border-box\"",
            target,
        )
}

fn under(src: &str, parent: &str) -> String {
    let (component, view) = src.split_once("  view\n").unwrap();
    let indented: String = view.lines().map(|line| format!("  {line}\n")).collect();
    format!("{component}  view\n    {parent}\n{indented}")
}

const FULL: &str = "width=\"100%\" height=\"100%\" box-sizing=\"border-box\"";
const CLIP: &str = "overflow=\"hidden\"";
const ELIGIBILITY: &str = "lower-transform-drag-ineligible";

/// Compare the warning with the kernel's source proof, without needing a host gesture.
fn eligibility(src: &str, binds: bool) -> Vec<contract::CompileError> {
    let (plan, warnings) = contract::compile_warned(src);
    let runner = Runner::boot(
        plan.unwrap_or_else(|e| panic!("{e}\n{src}")),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let handle = runner.kernel().find_by_test_id("handle")[0];
    assert_eq!(
        runner.kernel().transform_drag_binding(handle).is_some(),
        binds,
        "{src}"
    );
    warnings
        .into_iter()
        .filter(|w| w.id == ELIGIBILITY)
        .collect()
}

#[test]
fn static_transform_geometry_failures_name_the_target_or_clip_and_repair() {
    for (clip, target, label, repair) in [
        (
            CLIP.into(),
            FULL.replace("width=\"100%\"", "width=200"),
            "target `photo`",
            "width=\"100%\"",
        ),
        (
            CLIP.into(),
            FULL.replace("height=\"100%\"", "height=\"90%\""),
            "target `photo`",
            "height=\"100%\"",
        ),
        (
            CLIP.into(),
            FULL.replace("width=\"100%\"", "width=\"calc(100% - 1px)\""),
            "target `photo`",
            "width=\"100%\"",
        ),
        (
            CLIP.into(),
            FULL.replace("height=\"100%\"", "height=\"100vh\""),
            "target `photo`",
            "height=\"100%\"",
        ),
        (
            CLIP.into(),
            FULL.replace("width=\"100%\" ", ""),
            "target `photo`",
            "width=\"100%\"",
        ),
        (
            CLIP.into(),
            FULL.replace("height=\"100%\" ", ""),
            "target `photo`",
            "height=\"100%\"",
        ),
        (
            CLIP.into(),
            FULL.replace("border-box", "content-box"),
            "target `photo`",
            "box-sizing=\"border-box\"",
        ),
        (
            "".into(),
            FULL.into(),
            "clip `viewport`",
            "overflow-x=\"hidden\"",
        ),
        (
            "overflow-x=\"hidden\"".into(),
            FULL.into(),
            "clip `viewport`",
            "overflow-y=\"hidden\"",
        ),
        (
            "overflow=\"auto\"".into(),
            FULL.into(),
            "clip `viewport`",
            "overflow-x=\"hidden\"",
        ),
        (
            "overflow=\"hidden visible\"".into(),
            FULL.into(),
            "clip `viewport`",
            "overflow-y=\"hidden\"",
        ),
        (
            CLIP.into(),
            format!("{FULL} padding-left=1"),
            "target `photo`",
            "padding=0",
        ),
        (
            format!("{CLIP} padding=\"0 1%\""),
            FULL.into(),
            "clip `viewport`",
            "padding=0",
        ),
        (
            CLIP.into(),
            format!("{FULL} border=\"1px solid\""),
            "target `photo`",
            "border-width=0",
        ),
        (
            format!("{CLIP} border-top=\"1px solid\""),
            FULL.into(),
            "clip `viewport`",
            "border-width=0",
        ),
        (
            CLIP.into(),
            format!("{FULL} margin=\"0 -1px\""),
            "target `photo`",
            "margin=0",
        ),
        (
            CLIP.into(),
            format!("{FULL} margin=\"auto\""),
            "target `photo`",
            "margin=0",
        ),
        (
            CLIP.into(),
            format!("{FULL} inset=1"),
            "target `photo`",
            "inset=0",
        ),
        (
            CLIP.into(),
            format!("{FULL} left=\"env(safe-area-inset-left)\""),
            "target `photo`",
            "inset=0",
        ),
        (
            format!("{CLIP} padding=\"calc(10% - 10px)\""),
            FULL.into(),
            "clip `viewport`",
            "padding=0",
        ),
        (
            CLIP.into(),
            format!("{FULL} transform-origin=\"0% 50%\""),
            "target `photo`",
            "transform-origin=\"50% 50%\"",
        ),
        (
            CLIP.into(),
            format!("{FULL} scale=0"),
            "target `photo`",
            "scale=1",
        ),
        (
            CLIP.into(),
            format!("{FULL} rotate=10"),
            "target `photo`",
            "rotate=0",
        ),
        (
            CLIP.into(),
            format!("{FULL} translate=\"10% 0px\""),
            "target `photo`",
            "pixel translation",
        ),
        (
            format!("{CLIP} scale=2"),
            FULL.into(),
            "clip `viewport`",
            "scale=1",
        ),
        (
            format!("{CLIP} translate=\"1px\""),
            FULL.into(),
            "clip `viewport`",
            "identity transform",
        ),
    ] {
        let src = eligibility_source(&clip, &target);
        let warnings = eligibility(&src, false);
        assert!(
            warnings.iter().any(|w| w.message.contains(label)
                && w.message.contains(repair)
                && w.span.line == if label.starts_with("clip") { 8 } else { 9 }),
            "expected {label}: {repair}; {warnings:?}\n{src}"
        );
        assert!(warnings.iter().all(|w| w.is_warning()));
    }
}

#[test]
fn eligible_transform_styles_follow_shorthand_overrides_and_effective_insets() {
    for (clip, target) in [
        (CLIP.into(), FULL.into()),
        (
            CLIP.into(),
            format!("{FULL} scale=2 translate=\"-12px 3px\""),
        ),
        (
            CLIP.into(),
            format!("{FULL} padding=\"0 0 0 1px\" padding-left=0"),
        ),
        (CLIP.into(), format!("{FULL} margin=\"0%\" inset=\"auto\"")),
        (
            CLIP.into(),
            format!("{FULL} border-width=3 border-style=\"hidden\""),
        ),
        (
            format!("{CLIP} border-width=3 border-style=\"none\""),
            FULL.into(),
        ),
        (
            CLIP.into(),
            format!("{FULL} border=\"1px solid\" border-width=0"),
        ),
        (
            "overflow=\"auto\" overflow-x=\"hidden\" overflow-y=\"hidden\"".into(),
            FULL.into(),
        ),
        ("overflow=\"hidden hidden\"".into(), FULL.into()),
        (
            format!("{CLIP} padding=0 margin=2"),
            format!("{FULL} padding=0"),
        ),
    ] {
        let src = eligibility_source(&clip, &target);
        assert!(eligibility(&src, true).is_empty(), "{src}");
    }
    for (own, binds) in [("", false), ("box-sizing=\"border-box\"", true)] {
        let src = format!(
            "style Photo\n  box-sizing=\"content-box\"\n{}",
            eligibility_source(
                CLIP,
                &format!("width=\"100%\" height=\"100%\" class=Photo {own}")
            )
        );
        let warnings = eligibility(&src, binds);
        assert_eq!(warnings.is_empty(), binds, "{warnings:?}");
    }
}

#[test]
fn transform_geometry_is_checked_independently_of_scroll_ancestors() {
    for (clip, target, repair, binds) in [
        (CLIP, FULL.to_string(), "", true),
        (
            CLIP,
            FULL.replace(" box-sizing=\"border-box\"", ""),
            "border box",
            false,
        ),
        (
            CLIP,
            FULL.replace("width=\"100%\"", "width=200"),
            "width=\"100%\"",
            false,
        ),
        ("", FULL.to_string(), "clip `viewport`", false),
        (CLIP, format!("{FULL} inset=1"), "inset=0", false),
        (
            "overflow=\"hidden\" padding=1",
            FULL.to_string(),
            "padding=0",
            false,
        ),
    ] {
        let src = eligibility_source(clip, &target);
        for src in [
            under(&src, "scroll height=300"),
            src.replace(
                "        column testId",
                "        scroll height=300\n          column testId",
            ),
        ] {
            let warnings = eligibility(&src, binds);
            if binds {
                assert!(warnings.is_empty(), "{warnings:?}\n{src}");
            } else {
                assert!(
                    warnings.iter().any(|w| w.message.contains(repair)),
                    "{warnings:?}\n{src}"
                );
            }
        }
    }
}

#[test]
fn transform_diagnostics_preserve_view_defaults_in_dialogs() {
    for (target, binds) in [
        (FULL.to_string(), true),
        (FULL.replace(" box-sizing=\"border-box\"", ""), false),
    ] {
        let src = eligibility_source(CLIP, &target);
        for src in [
            under(&src, "dialog"),
            src.replace("column id=\"viewport\"", "dialog id=\"viewport\""),
        ] {
            let warnings = eligibility(&src, binds);
            if binds {
                assert!(warnings.is_empty(), "{warnings:?}\n{src}");
            } else {
                assert!(
                    warnings.iter().any(|w| w.message.contains("target `photo`")
                        && w.message.contains("box-sizing=\"border-box\"")),
                    "{warnings:?}\n{src}"
                );
            }
        }
    }
}

#[test]
fn transform_diagnostics_check_dialog_and_control_path_rows() {
    for parent in ["dialog", "button appearance=\"none\"", "list height=300"] {
        for (row, repair, warns) in [
            ("display=\"none\"", "show it", true),
            ("rotate=10", "rotate=0", true),
            ("inert=true", "inert=false", true),
            ("disabled=true", "disabled=false", true),
            ("inert=(value == 0)", "", false),
            ("rotate=(value == 0 ? 10 : 0)", "", false),
        ] {
            let src = under(
                &eligibility_source(CLIP, FULL),
                &format!("{parent} id=\"outer\" {row}"),
            );
            let warnings = eligibility(&src, false);
            if warns {
                assert!(
                    warnings
                        .iter()
                        .any(|w| w.message.contains("path element `outer`")
                            && w.message.contains(repair)
                            && w.span.line == 8),
                    "{warnings:?}\n{src}"
                );
            } else {
                assert!(warnings.is_empty(), "{warnings:?}\n{src}");
            }
        }
    }
}

#[test]
fn transform_targets_and_clips_must_be_views() {
    for tag in ["scroll", "button appearance=\"none\""] {
        for (element, label) in [("viewport", "clip `viewport`"), ("photo", "target `photo`")] {
            let src = eligibility_source("overflow=\"hidden\" height=300", FULL).replace(
                &format!("column id=\"{element}\""),
                &format!("{tag} id=\"{element}\""),
            );
            let warnings = eligibility(&src, false);
            assert!(
                warnings.iter().any(|w| w.message.contains(label)
                    && w.message.contains("plain View")
                    && w.message.contains("`column` or `box`")
                    && w.span.line == if element == "viewport" { 8 } else { 9 }),
                "{warnings:?}\n{src}"
            );
            if element == "viewport" {
                assert!(
                    warnings
                        .iter()
                        .any(|w| w.message.contains("overflow=\"hidden\"")
                            && w.message.contains("padding=0")
                            && w.message.contains("border-width=0")),
                    "{warnings:?}\n{src}"
                );
            }
        }
    }
}

#[test]
fn a_root_transform_target_needs_a_clipping_parent() {
    let src = eligibility_source(CLIP, FULL);
    let (component, view) = src.split_once("  view\n").unwrap();
    let view: String = view
        .lines()
        .skip(1)
        .map(|line| format!("{}\n", line.strip_prefix("  ").unwrap()))
        .collect();
    let src = format!("{component}  view\n{view}");
    let warnings = eligibility(&src, false);
    assert!(
        warnings.iter().any(|w| w.message.contains("target `photo`")
            && w.message.contains("clipping parent")
            && w.message.contains("`column` or `box`")
            && w.message.contains("overflow=\"hidden\"")
            && w.span.line == 8),
        "{warnings:?}\n{src}"
    );
}

#[test]
fn transform_diagnostics_apply_class_overrides_before_rewriting_transforms() {
    for (class, own, binds, warns) in [
        ("rotate(10deg)", "\"none\"", true, false),
        ("none", "\"rotate(10deg)\"", false, true),
        (
            "rotate(10deg)",
            "(value == 0 ? \"none\" : \"rotate(10deg)\")",
            true,
            false,
        ),
        (
            "none",
            "(value == 0 ? \"rotate(10deg)\" : \"none\")",
            false,
            false,
        ),
    ] {
        for (element, label) in [
            ("id=\"photo\"", "target `photo`"),
            ("id=\"viewport\"", "clip `viewport`"),
            ("testId=\"handle\"", "`column` on the handle's path"),
        ] {
            let src = format!(
                "style Photo\n  transform=\"{class}\"\n{}",
                eligibility_source(CLIP, FULL)
                    .replace(element, &format!("{element} class=Photo transform={own}"))
            );
            let warnings = eligibility(&src, binds);
            if warns {
                assert!(
                    warnings
                        .iter()
                        .any(|w| w.message.contains(label) && w.message.contains("rotation")),
                    "{warnings:?}\n{src}"
                );
            } else {
                assert!(warnings.is_empty(), "{warnings:?}\n{src}");
            }
        }
    }
}

#[test]
fn transform_diagnostics_name_statically_inert_or_disabled_path_elements() {
    for prop in ["inert", "disabled"] {
        for (element, label) in [
            ("id=\"photo\"", "target `photo`"),
            ("id=\"viewport\"", "clip `viewport`"),
            ("testId=\"handle\"", "`column` on the handle's path"),
        ] {
            for (value, binds, warns) in [
                ("true", false, true),
                ("false", true, false),
                ("blocked", false, false),
            ] {
                let src = eligibility_source(CLIP, FULL)
                    .replace(
                        "  state value = 0\n",
                        "  state value = 0\n  state blocked = true\n",
                    )
                    .replace(element, &format!("{element} {prop}={value}"));
                let warnings = eligibility(&src, binds);
                if warns {
                    assert!(
                        warnings.iter().any(|w| w.message.contains(label)
                            && w.message.contains(&format!("{prop}=false"))),
                        "{warnings:?}\n{src}"
                    );
                } else {
                    assert!(warnings.is_empty(), "{warnings:?}\n{src}");
                }
            }
        }
    }
}

#[test]
fn transform_diagnostics_check_authored_rows_below_controls_and_collections() {
    for parent in [
        "button appearance=\"none\"",
        "button",
        "link",
        "list height=300",
    ] {
        for (target, binds, repair) in [
            (FULL.to_string(), true, None),
            (
                FULL.replace("width=\"100%\"", "width=200"),
                false,
                Some("width=\"100%\""),
            ),
            (
                FULL.replace(" box-sizing=\"border-box\"", ""),
                false,
                Some("box-sizing=\"border-box\""),
            ),
        ] {
            let src = under(&eligibility_source(CLIP, &target), parent);
            let warnings = eligibility(&src, binds);
            if let Some(repair) = repair {
                assert!(
                    warnings
                        .iter()
                        .any(|w| w.message.contains("target `photo`") && w.message.contains(repair)),
                    "{warnings:?}\n{src}"
                );
            } else {
                assert!(warnings.is_empty(), "{warnings:?}\n{src}");
            }
        }
    }
}

#[test]
fn transform_diagnostics_assume_no_defaults_below_a_grouped_list() {
    // The grouped sheet writes rows under the author's, so an unwritten row
    // there is not the CSS default.
    for target in [
        FULL.to_string(),
        FULL.replace(" box-sizing=\"border-box\"", ""),
    ] {
        let src = eligibility_source(CLIP, &target);
        let src = under(&under(&src, "section"), "list appearance=\"auto\"");
        let warnings = eligibility(&src, false);
        assert!(warnings.is_empty(), "{warnings:?}\n{src}");
    }
}

#[test]
fn dynamic_transform_values_are_not_rejected_from_their_initial_values() {
    let src = eligibility_source(
        "overflow=clipping padding=pad border-width=pad border-style=line",
        "width=size height=size box-sizing=sizing padding=pad border-width=pad border-style=line margin=offset inset=offset scale=factor translate=shift transform-origin=origin",
    ).replace("  state value = 0\n", "  state value = 0\n  state clipping = \"hidden\"\n  state size = \"100%\"\n  state sizing = \"border-box\"\n  state pad = 0\n  state line = \"none\"\n  state offset = 0\n  state factor = 1\n  state shift = \"0px\"\n  state origin = \"50% 50%\"\n");
    assert!(eligibility(&src, true).is_empty());
    let invalid = src.replace(
        "state sizing = \"border-box\"",
        "state sizing = \"content-box\"",
    );
    assert!(eligibility(&invalid, false).is_empty());
    // A length relative to a font size is resolved only when the app runs.
    let relative = eligibility_source(CLIP, &format!("{FULL} padding=\"1rem\""));
    let (result, warnings) = contract::compile_warned(&relative);
    result.unwrap();
    assert!(warnings.iter().all(|w| w.id != ELIGIBILITY), "{warnings:?}");
    let choice = format!(
        "style Good\n  box-sizing=\"border-box\"\nstyle Bad\n  box-sizing=\"content-box\"\n{}",
        eligibility_source(
            CLIP,
            "width=\"100%\" height=\"100%\" class=(value == 0 ? Good : Bad)"
        )
    );
    assert!(eligibility(&choice, true).is_empty());
}

#[test]
fn transform_handlers_compile_bake_roundtrip_and_export_old_and_new_ordinals() {
    for (event, params, ordinal) in [
        ("transformgeometry", GEOMETRY, 15),
        ("transformrelease", RELEASE, 16),
    ] {
        let plan = contract::compile(&source(event, params, "receive", "\"photo\"")).unwrap();
        let plan = contract::bake(plan, NoData).unwrap();
        let plan = Plan::decode(&plan.encode()).unwrap();
        let runner = Runner::boot(
            plan,
            NoData,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let key = runner.kernel().find_by_test_id("handle")[0];
        let handle = runner.kernel().node_by_key(key).unwrap();
        let prop = PropId::from_name("transformDragFor").unwrap();
        assert_eq!(prop as u16, 76);
        assert_eq!(handle.props.str(prop), Some("photo"));
        let event_kind = EventKind::from_name(event).unwrap();
        assert_eq!(event_kind as u8, ordinal);
        assert!(runner.handlers_of(handle.id).contains(&event_kind));
        let tree: serde_json::Value = serde_json::from_str(&agent::tree(&runner)).unwrap();
        assert!(tree["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["props"]["transformDragFor"] == "photo"
                && n["handlers"]
                    .as_array()
                    .is_some_and(|h| h.contains(&serde_json::json!(event)))));
    }
    assert_eq!(EventKind::Heightrelease as u8, 14);
    assert_eq!(EventKind::Navigate as u8, 13);
    assert_eq!(PropId::HeightDragFor as u16, 75);
}

#[test]
fn exact_numeric_arity_and_string_idref_are_required() {
    for (event, params) in [
        ("transformgeometry", GEOMETRY),
        ("transformrelease", RELEASE),
    ] {
        for bad in [
            params.rsplit_once(',').unwrap().0.to_string(),
            format!("{params}, extra: number"),
        ] {
            let e = contract::compile(&source(event, &bad, "receive", "\"photo\""))
                .unwrap_err()
                .to_string();
            assert!(e.contains("handler-arity"), "{e}");
        }
        for ty in ["string", "bool"] {
            let bad = params.replacen("number", ty, 1);
            let e = contract::compile(&source(event, &bad, "receive", "\"photo\""))
                .unwrap_err()
                .to_string();
            assert!(e.contains("type-handler-payload"), "{e}");
        }
        for bad in ["1", "false"] {
            let e = contract::compile(&source(event, params, "receive", bad))
                .unwrap_err()
                .to_string();
            assert!(e.contains("attr-type"), "{e}");
        }
        contract::compile(&source(
            event,
            &format!("origin: string, {params}"),
            "receive(\"photo\")",
            "\"photo\"",
        ))
        .unwrap();
    }
}

#[test]
fn one_transform_handler_without_the_other_is_refused() {
    for (event, params, missing) in [
        ("transformgeometry", GEOMETRY, "transformrelease"),
        ("transformrelease", RELEASE, "transformgeometry"),
    ] {
        let src = format!("component App\n  state value = 0\n  action receive({params})\n    value = 1\n  view\n    column\n      column id=\"photo\"\n        column transformDragFor=\"photo\" {event}=receive\n");
        let e = contract::compile(&src).unwrap_err().to_string();
        assert!(
            e.contains("lower-transform-drag-handlers") && e.contains(&format!("add `{missing}=`")),
            "{e}"
        );
    }
}

#[test]
fn numeric_types_are_rechecked_after_child_action_inlining() {
    for (event, params) in [
        ("transformgeometry", GEOMETRY),
        ("transformrelease", RELEASE),
    ] {
        for invalid in [false, true] {
            let params = if invalid {
                params.replacen("number", "string", 1)
            } else {
                params.into()
            };
            let (other, other_params) = partner(event);
            let src = format!("component App\n  state n = 0\n  action receive({params})\n    n = 1\n  action partner({other_params})\n    n = 2\n  view\n    column\n      Handle(callback=receive, other=partner)\ncomponent Handle\n  props\n    callback: action\n    other: action\n  view\n    column transformDragFor=\"photo\" {event}=callback {other}=other\n");
            let result = contract::compile(&src);
            if invalid {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("type-handler-payload"));
            } else {
                result.unwrap();
            }
        }
    }
}

#[test]
fn pixel_translate_literals_and_dynamic_template_reach_kernel_rows() {
    let src = "component App\n  state x = 0\n  state y = 0\n  action move(a: number, b: number)\n    x = a\n    y = b\n  view\n    column\n      column testId=\"literal\" translate=\"-12.5px 3px\"\n      column testId=\"dynamic\" translate=`${x}px ${y}px`\n";
    let plan = contract::bake(contract::compile(src).unwrap(), NoData).unwrap();
    let mut r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let literal = r.kernel().find_by_test_id("literal")[0];
    assert_eq!(
        r.kernel().node_by_key(literal).unwrap().style.translate,
        exact_kernel::Vec2 { x: -12.5, y: 3.0 }
    );
    let dynamic = r.kernel().find_by_test_id("dynamic")[0];
    r.act("move", vec![Value::Number(27.25), Value::Number(-40.0)])
        .unwrap();
    assert_eq!(
        r.kernel().node_by_key(dynamic).unwrap().style.translate,
        exact_kernel::Vec2 { x: 27.25, y: -40.0 }
    );
    // A value the row refuses is unset, as CSS does an invalid value at
    // computed-value time, and journaled; the runner goes on.
    r.act("move", vec![Value::Number(1e40), Value::Number(0.0)])
        .unwrap();
    assert!(!r.is_poisoned());
    assert_eq!(
        r.kernel().node_by_key(dynamic).unwrap().style.translate,
        exact_kernel::Vec2 { x: 0.0, y: 0.0 }
    );
    assert!(r.journal().any(|l| l.contains("invalid translate value")));
    r.act("move", vec![Value::Number(1.0), Value::Number(2.0)])
        .unwrap();
    assert_eq!(
        r.kernel().node_by_key(dynamic).unwrap().style.translate,
        exact_kernel::Vec2 { x: 1.0, y: 2.0 }
    );
    for bad in [
        "1 2",
        "20%% 0",
        "calc(1px + 2px) 0",
        "1px 2px 3px 4px",
        "NaNpx 0",
        "1e39px 0",
    ] {
        let src = format!("component App\n  view\n    column translate=\"{bad}\"\n");
        assert!(contract::compile(&src).is_err(), "{bad}");
    }
}

const OWNERS: &str = "lower-transform-drag-owners";

fn ownership_source(view: &str) -> String {
    let view: String = view.lines().map(|line| format!("      {line}\n")).collect();
    format!(
        "component App\n  state n = 0\n  state on = true\n  state choice = some(1)\n  state rows = [1, 2]\n  action geometry({GEOMETRY})\n    n = 1\n  action release({RELEASE})\n    n = 2\n  view\n    column\n{view}"
    )
}

fn photo(name: &str) -> String {
    format!(
        "column overflow=\"hidden\"\n  column id={name} width=\"100%\" height=\"100%\" box-sizing=\"border-box\"\n    column transformDragFor={name} transformgeometry=geometry transformrelease=release"
    )
}

fn indented(body: &str) -> String {
    body.lines().map(|line| format!("  {line}\n")).collect()
}

fn nested_transform_targets(body: &str) -> String {
    ownership_source(&format!(
        "column overflow=\"hidden\"\n  column id=\"outer\" width=\"100%\" height=\"100%\" box-sizing=\"border-box\" overflow=\"hidden\"\n    column id=\"inner\" width=\"100%\" height=\"100%\" box-sizing=\"border-box\"\n{}",
        indented(&indented(&indented(body)))
    ))
}

fn transform_choice(condition: &str) -> String {
    format!("if {condition}\n  column transformDragFor=\"outer\" transformgeometry=geometry transformrelease=release\nelse\n  column transformDragFor=\"inner\" transformgeometry=geometry transformrelease=release")
}

/// The lines of the ownership warnings `src` earns; a target that cannot bind
/// earns its own warning, which these cases do not count.
fn owner_warnings(src: &str) -> Vec<u32> {
    super::warnings::warned(src)
        .into_iter()
        .filter(|(id, _)| id == OWNERS)
        .map(|(_, line)| line)
        .collect()
}

#[test]
fn simultaneous_transform_targets_name_both_binding_sites() {
    // The id is scoped to strict ancestors: equal spellings can name different targets.
    for other in ["\"other\"", "\"photo\""] {
        let src = ownership_source(&format!("{}\n{}", photo("\"photo\""), photo(other)));
        assert_eq!(owner_warnings(&src), [14, 17]);
        let (_, warnings) = contract::compile_warned(&src);
        for w in warnings {
            assert!(w.message.contains("first valid target"), "{w}");
            assert!(w.message.contains("never stolen"), "{w}");
            assert!(
                w.message
                    .contains("Mount handles only for the active target"),
                "{w}"
            );
        }
    }
}

#[test]
fn constant_templates_and_strings_resolve_to_the_same_transform_target() {
    for (id, binding) in [
        ("\"first\"", "`first`"),
        ("`first`", "\"first\""),
        ("\"first\"", "`fir${\"st\"}`"),
    ] {
        let first = photo(id).replace(
            &format!("transformDragFor={id}"),
            &format!("transformDragFor={binding}"),
        );
        let src = ownership_source(&format!("{first}\n{}", photo("\"second\"")));
        assert_eq!(owner_warnings(&src), [14, 17], "{src}");
        assert!(owner_warnings(&ownership_source(&first)).is_empty());
    }
}

#[test]
fn a_transform_target_repeated_by_a_row_template_is_warned_once() {
    for name in ["\"photo\"", "`photo-${row}`"] {
        for list in ["rows", "[1, 2]"] {
            let src = ownership_source(&format!(
                "list id=\"photos\"\n  for row in {list} key=row\n{}",
                indented(&indented(&photo(name)))
            ));
            assert_eq!(owner_warnings(&src), [16]);
            let (_, warnings) = contract::compile_warned(&src);
            assert!(warnings[0].message.contains("`for` template"));
        }
    }
}

#[test]
fn one_transform_target_can_have_several_handles_even_in_a_loop() {
    for handles in [
        "column transformDragFor=\"photo\" transformgeometry=geometry transformrelease=release",
        "column transformDragFor=\"photo\" transformgeometry=geometry transformrelease=release\ncolumn transformDragFor=\"photo\" transformgeometry=geometry transformrelease=release",
        "for row in rows key=row\n  column transformDragFor=\"photo\" transformgeometry=geometry transformrelease=release",
    ] {
        let src = ownership_source(&format!(
            "column overflow=\"hidden\"\n  column id=\"photo\" width=\"100%\" height=\"100%\" box-sizing=\"border-box\"\n{}",
            indented(&indented(handles))
        ));
        assert!(owner_warnings(&src).is_empty(), "{src}");
    }
    let src = ownership_source(&format!(
        "for row in [1] key=row\n{}",
        indented(&photo("\"photo\""))
    ));
    assert!(owner_warnings(&src).is_empty(), "{src}");
}

#[test]
fn mutually_exclusive_transform_targets_do_not_compete() {
    let first = indented(&photo("\"first\""));
    let second = indented(&photo("\"second\""));
    for view in [
        format!("if on\n{first}else\n{second}"),
        format!(
            "match choice\n  case some(value)\n{}  case none\n{}",
            indented(&first),
            indented(&second)
        ),
    ] {
        assert!(
            owner_warnings(&ownership_source(&view)).is_empty(),
            "{view}"
        );
        // A sibling outside the choice can compete with either arm.
        let src = ownership_source(&format!("{view}{}", photo("\"third\"")));
        assert_eq!(owner_warnings(&src).len(), 3);
    }
}

#[test]
fn hidden_or_unresolved_transform_bindings_are_not_competitors() {
    for other in [
        photo("\"other\"").replacen("column ", "column display=\"none\" ", 1),
        photo("\"other\"").replacen("column ", "column inert=true ", 1),
        photo("\"other\"").replace("transformDragFor=\"other\"", "transformDragFor=\"missing\""),
        photo("\"other\"").replacen("column ", "column id=\"other\" ", 1),
        photo("\"other\"").replace("transformDragFor=\"other\"", "transformDragFor=\"\""),
    ] {
        let src = ownership_source(&format!("{}\n{other}", photo("\"photo\"")));
        assert!(owner_warnings(&src).is_empty(), "{src}");
    }
}

#[test]
fn static_class_visibility_respects_inline_overrides_for_transform_targets() {
    for (attrs, count) in [
        ("class=Hidden", 0),
        ("class=Hidden display=\"block\"", 2),
        ("class=Visible display=\"none\"", 0),
        ("class=Hidden display=(on ? \"block\" : \"none\")", 2),
    ] {
        for suffix in ["overflow", "id"] {
            let other = photo("\"other\"").replacen(
                &format!("column {suffix}"),
                &format!("column {attrs} {suffix}"),
                1,
            );
            let src = format!(
                "{}style Hidden\n  display=\"none\"\nstyle Visible\n  display=\"block\"\n",
                ownership_source(&format!("{}\n{other}", photo("\"photo\"")))
            );
            assert_eq!(owner_warnings(&src).len(), count, "{src}");
        }
    }
}

#[test]
fn shared_conditions_keep_transform_handles_exclusive_across_rows() {
    let handle = |name| {
        format!("column transformDragFor=\"{name}\" transformgeometry=geometry transformrelease=release")
    };
    for choice in [
        transform_choice("on"),
        transform_choice("selected"),
        transform_choice("not on or n > 0"),
        format!(
            "match choice\n  case some(value)\n{}  case none\n{}",
            indented(&indented(&handle("outer"))),
            indented(&indented(&handle("inner")))
        ),
    ] {
        let src =
            nested_transform_targets(&format!("for row in rows key=row\n{}", indented(&choice)))
                .replace("  view\n", "  derive selected = on\n  view\n");
        assert!(owner_warnings(&src).is_empty(), "{src}");
    }
    let src = nested_transform_targets(&format!(
        "for row in rows key=row\n  for item, position in [row] key=item\n{}",
        indented(&indented(&transform_choice("position == 0")))
    ));
    assert!(owner_warnings(&src).is_empty(), "{src}");
}

#[test]
fn opposite_arms_in_different_rows_can_bind_different_transform_targets() {
    for condition in ["row == 1", "position == 0"] {
        let src = nested_transform_targets(&format!(
            "for row, position in rows key=row\n{}",
            indented(&transform_choice(condition))
        ));
        assert_eq!(owner_warnings(&src), [17, 19]);
    }
}

#[test]
fn child_transform_choices_can_vary_with_row_state_and_props() {
    for (declaration, count) in [
        ("state active = true", 2),
        ("derive active = row == 1", 2),
        ("derive active = shared", 0),
    ] {
        let src = format!(
            "{}component Handle\n  props\n    row: number\n    shared: bool\n    geometry: action\n    release: action\n  {declaration}\n  view\n{}",
            nested_transform_targets("for row in rows key=row\n  Handle(row=row, shared=on, geometry=geometry, release=release)"),
            indented(&indented(&transform_choice("active")))
        );
        assert_eq!(owner_warnings(&src).len(), count, "{src}");
    }
}

#[test]
fn a_guard_can_limit_a_transform_target_to_one_literal_row() {
    for (list, condition) in [
        ("[1, 2]", "position == 0"),
        ("[1, 2]", "row == 1"),
        ("rows", "position == 0"),
    ] {
        for guard_handle in [false, true] {
            let photo = photo("\"photo\"");
            let body = if guard_handle {
                photo.replace(
                    "    column transformDragFor",
                    &format!("    if {condition}\n      column transformDragFor"),
                )
            } else {
                format!("if {condition}\n{}", indented(&photo))
            };
            let src = ownership_source(&format!(
                "for row, position in {list} key=row\n{}",
                indented(&body)
            ));
            assert!(owner_warnings(&src).is_empty(), "{src}");
        }
    }
    let src = ownership_source(&format!(
        "for row, position in [1, 2, 3] key=row\n  if position < 2\n{}",
        indented(&indented(&photo("\"photo\"")))
    ));
    assert_eq!(owner_warnings(&src).len(), 1, "{src}");
}

#[test]
fn separate_handles_in_different_rows_can_own_distinct_transform_targets() {
    let src = ownership_source(&format!(
        "for row, position in [1, 2] key=row\n{}",
        indented(&photo("\"photo\"").replace(
            "    column transformDragFor=\"photo\" transformgeometry=geometry transformrelease=release",
            "    if position == 0\n      column transformDragFor=\"photo\" transformgeometry=geometry transformrelease=release\n    else\n      column transformDragFor=\"photo\" transformgeometry=geometry transformrelease=release"
        ))
    ));
    assert_eq!(owner_warnings(&src).len(), 2, "{src}");
}

#[test]
fn shadowed_row_names_do_not_resolve_to_an_outer_transform_target() {
    let src = ownership_source(
        "for row in [\"a\", \"b\"] key=row\n  column overflow=\"hidden\"\n    column id=row width=\"100%\" height=\"100%\" box-sizing=\"border-box\"\n      for row in [\"c\"] key=row\n        column transformDragFor=row transformgeometry=geometry transformrelease=release",
    );
    assert!(owner_warnings(&src).is_empty(), "{src}");
}

#[test]
fn literal_number_and_boolean_template_parts_resolve_transform_targets() {
    for (id, binding) in [
        ("`photo-${1}`", "`photo-${1}`"),
        ("`photo-${1}`", "\"photo-1\""),
        ("\"photo-true\"", "`photo-${true}`"),
        ("`photo-${false}`", "\"photo-false\""),
        ("`photo-${-0}`", "\"photo-0\""),
        ("\"photo-1e+21\"", "`photo-${1e21}`"),
    ] {
        let first = photo(id).replace(
            &format!("transformDragFor={id}"),
            &format!("transformDragFor={binding}"),
        );
        let src = ownership_source(&format!("{first}\n{}", photo("\"second\"")));
        assert_eq!(owner_warnings(&src), [14, 17], "{src}");
    }
}

#[test]
fn statically_unreachable_match_arms_cannot_own_a_transform_drag() {
    for (subject, reachable, unreachable) in [
        ("some(1)", "\"outer\"", "\"inner\""),
        ("none", "\"inner\"", "\"outer\""),
    ] {
        let body = format!(
            "match {subject}\n  case some(value)\n{}  case none\n{}",
            indented(&indented(&photo("\"outer\""))),
            indented(&indented(&photo("\"inner\"")))
        );
        let src = ownership_source(&format!("{body}{}", photo("\"side\"")));
        let (_, warnings) = contract::compile_warned(&src);
        let lines = owner_warnings(&src);
        assert_eq!(lines.len(), 2, "{src}: {warnings:?}");
        let handle = |id| format!("transformDragFor={id}");
        assert!(lines.iter().any(|line| src
            .lines()
            .nth(*line as usize - 1)
            .unwrap()
            .contains(&handle(reachable))));
        assert!(lines.iter().all(|line| !src
            .lines()
            .nth(*line as usize - 1)
            .unwrap()
            .contains(&handle(unreachable))));
    }
}

#[test]
fn a_template_target_cannot_equal_a_shorter_literal_ancestor_id() {
    let src = ownership_source(&format!(
        "column id=\"photo\" overflow=\"hidden\"\n  for row in [1, 2] key=row\n{}",
        indented(&indented(&photo("`photo-${row}`")))
    ));
    assert_eq!(owner_warnings(&src), [16]);
}

#[test]
fn complementary_sibling_guards_keep_transform_targets_exclusive() {
    let first = indented(&photo("\"first\""));
    let second = indented(&photo("\"second\""));
    for (a, b) in [("on", "not on"), ("not on", "on"), ("n == 1", "n != 1")] {
        let src = ownership_source(&format!("if {a}\n{first}if {b}\n{second}"));
        assert!(owner_warnings(&src).is_empty(), "{src}");
    }
    let src = ownership_source(&format!("if on\n{first}if n > 0\n{second}"));
    assert_eq!(owner_warnings(&src).len(), 2, "{src}");
    // A row-dependent guard can take both arms in different rows.
    let handle = |name| {
        format!("column transformDragFor=\"{name}\" transformgeometry=geometry transformrelease=release")
    };
    let src = nested_transform_targets(&format!(
        "for row in rows key=row\n  if row == 1\n{}  if row != 1\n{}",
        indented(&indented(&handle("outer"))),
        indented(&indented(&handle("inner")))
    ));
    assert_eq!(owner_warnings(&src).len(), 2, "{src}");
}

#[test]
fn a_match_arm_limits_a_transform_target_to_the_rows_that_take_it() {
    let arms = |subject: &str| {
        format!(
            "  match {subject}\n    case some(value)\n{}    case none\n      text \"x\"\n",
            indented(&indented(&indented(&photo("\"photo\""))))
        )
    };
    for (list, subject, count) in [
        ("[some(1), none]", "row", 0),
        ("[1, 2]", "(position == 0 ? some(1) : none)", 0),
        ("[some(1), some(2)]", "row", 1),
    ] {
        let src = ownership_source(&format!(
            "for row, position in {list} key=position\n{}",
            arms(subject)
        ));
        // A position key draws its own advice.
        let owners = super::warnings::warned(&src)
            .into_iter()
            .filter(|(id, _)| *id == OWNERS)
            .count();
        assert_eq!(owners, count, "{src}");
    }
}
