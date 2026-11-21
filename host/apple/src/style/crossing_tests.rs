//! Which positioning rows cross to the presenter, which otherwise reads a
//! box's place from its frame: a dialog's insets, a placed popover's
//! margins, a sticky box's vertical insets. A width or height does not
//! cross, but that its author wrote one does (`"written"`): a presenter tells
//! an author's box from one a sheet gave (a grouped row's leading symbol,
//! `GroupedCustomRowsIOS`). So does an `animation`, which the engine runs: a
//! glass its author animates in does not also materialize.

use super::*;
use exact_runner::{DataError, DataSource, Runner};

#[test]
fn video_fit_distinguishes_an_absent_declaration_from_an_authored_clear() {
    for (declaration, expected) in [
        ("", "contain"),
        (r#"object-fit="unset""#, "fill"),
        (r#"object-fit="""#, "fill"),
        (r#"object-fit="fill""#, "fill"),
        (r#"object-fit="cover""#, "cover"),
    ] {
        let source = format!(
            "component App\n  view\n    video testId=\"video\" width=160 height=80 {declaration}\n"
        );
        let plan = contract::compile(&source).unwrap().encode();
        let (host, boot) = crate::Host::boot(
            &plan,
            NoData,
            Box::new(exact_kernel::MonospaceMeasurer::default()),
            400.,
            800.,
        )
        .unwrap();
        let kernel = host.runner().kernel();
        let node = kernel
            .node_by_key(kernel.find_by_test_id("video")[0])
            .unwrap();
        assert_eq!(
            node.style.written.has(StyleId::ObjectFit),
            !declaration.is_empty()
        );
        let batch: serde_json::Value = serde_json::from_str(&boot).unwrap();
        let style = batch["ops"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|op| op.get("style"))
            .next_back()
            .unwrap();
        assert_eq!(
            style["object_fit"].as_str().unwrap_or(""),
            expected,
            "{declaration}: {boot}"
        );
    }
}

#[test]
fn an_invalid_object_position_coordinate_resets_both_axes() {
    for (css, expected) in [
        (
            "var(--missing) 75%",
            serde_json::json!([{"pct": 50}, {"pct": 50}]),
        ),
        (
            "25% var(--missing)",
            serde_json::json!([{"pct": 50}, {"pct": 50}]),
        ),
        (
            "var(--missing, right) bottom",
            serde_json::json!([{"pct": 100}, {"pct": 100}]),
        ),
        ("var(--missing, 7px 11px)", serde_json::json!([7, 11])),
        (
            "var(--missing, var(--other, right bottom))",
            serde_json::json!([{"pct": 100}, {"pct": 100}]),
        ),
        (
            "var(--missing, 25%) 75%",
            serde_json::json!([{"pct": 25}, {"pct": 75}]),
        ),
    ] {
        let mut style = StyleProps::default();
        style
            .set_dynamic(StyleId::ObjectPosition, &StyleValue::Text(css.into()))
            .unwrap();
        let env = Env {
            properties: Some(Default::default()),
            ..Env::default()
        };
        let json: serde_json::Value = serde_json::from_str(&style_json(&style, &env).0).unwrap();
        assert_eq!(json["object_position"], expected, "{css}");
    }
}

#[test]
fn object_position_resolves_viewport_lengths_before_serializing() {
    let plan = contract::compile(r#"component App
  view
    image "symbol:star" width=160 height=80 object-fit="none" object-position="calc(10vw) calc(20vh)"
"#).unwrap().encode();
    let (mut host, boot) = crate::Host::boot(
        &plan,
        NoData,
        Box::new(exact_kernel::MonospaceMeasurer::default()),
        400.,
        800.,
    )
    .unwrap();
    let position = |batch: &str| {
        let json: serde_json::Value = serde_json::from_str(batch).unwrap();
        json["ops"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find_map(|op| op["style"].get("object_position").cloned())
    };
    let resized = host.resize(600., 300.);
    assert_eq!(
        position(&boot),
        Some(serde_json::json!([40, 160])),
        "{boot}"
    );
    assert_eq!(
        position(&resized),
        Some(serde_json::json!([60, 60])),
        "{resized}"
    );
}

#[test]
fn object_position_relative_units_follow_the_viewport_and_root_font() {
    for (css, initial, resized, scaled) in [
        ("10vw 20vh", [40, 160], [60, 60], [60, 60]),
        ("1rem 2rem", [16, 32], [16, 32], [20, 40]),
        ("1em 2em", [16, 32], [16, 32], [20, 40]),
    ] {
        let source = format!("component App\n  view\n    image \"symbol:star\" width=160 height=80 object-position=\"{css}\"\n");
        let plan = contract::compile(&source).unwrap().encode();
        let (mut host, boot) = crate::Host::boot(
            &plan,
            NoData,
            Box::new(exact_kernel::MonospaceMeasurer::default()),
            400.,
            800.,
        )
        .unwrap();
        let mut current = serde_json::Value::Null;
        for (batch, expected) in [
            (boot, initial),
            (host.resize(600., 300.), resized),
            (host.set_root_font_size(20.), scaled),
        ] {
            let json: serde_json::Value = serde_json::from_str(&batch).unwrap();
            for op in json["ops"].as_array().unwrap() {
                if let Some(position) = op["style"].get("object_position") {
                    current = position.clone();
                }
            }
            assert_eq!(current, serde_json::json!(expected), "{css}: {batch}");
        }
    }
}

#[test]
fn field_sizing_reaches_native_control_measurement() {
    for sizing in ["content", "fixed"] {
        let mut style = StyleProps::default();
        style
            .set_dynamic(StyleId::FieldSizing, &StyleValue::Text(sizing.into()))
            .unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&style_json(&style, &Env::default()).0).unwrap();
        assert_eq!(json["field_sizing"], sizing);
    }
}

#[test]
fn only_dialogs_keep_viewport_positioning_rows() {
    use exact_kernel::{Kernel, MonospaceMeasurer, Op, PropId};
    let mut kernel = Kernel::new(Box::new(MonospaceMeasurer::default()));
    let mut style = StyleProps::default();
    style
        .set_dynamic(StyleId::Left, &StyleValue::Number(12.0))
        .unwrap();
    style
        .set_dynamic(StyleId::Bottom, &StyleValue::Text("calc(10% + 8px)".into()))
        .unwrap();
    kernel
        .apply(
            0,
            1,
            &[
                Op::CreateView {
                    id: 1,
                    node_type: NodeType::View,
                },
                Op::SetStyle {
                    id: 1,
                    patch: Box::new(style),
                },
            ],
        )
        .unwrap();
    let ordinary: serde_json::Value =
        serde_json::from_str(&style_json_for(&kernel.node(1).unwrap(), &Env::default()).0).unwrap();
    assert!(ordinary.get("left").is_none());
    assert!(ordinary.get("bottom").is_none());
    kernel
        .apply(
            0,
            2,
            &[Op::SetProp {
                id: 1,
                prop: PropId::SemanticTag,
                value: "dialog".into(),
            }],
        )
        .unwrap();
    let dialog: serde_json::Value =
        serde_json::from_str(&style_json_for(&kernel.node(1).unwrap(), &Env::default()).0).unwrap();
    assert_eq!(dialog["left"], 12);
    assert_eq!(dialog["bottom"], serde_json::json!({ "pct": 10, "px": 8 }));
}

/// A sticky box carries its vertical insets, which say whether it
/// sticks as its scroller scrolls (an iOS header's search field), and
/// not its horizontal ones.
#[test]
fn a_sticky_box_carries_its_vertical_insets() {
    use exact_kernel::{Kernel, MonospaceMeasurer, Op};
    let mut kernel = Kernel::new(Box::new(MonospaceMeasurer::default()));
    let mut style = StyleProps::default();
    style
        .set_dynamic(StyleId::PositionType, &StyleValue::Text("sticky".into()))
        .unwrap();
    style
        .set_dynamic(StyleId::Left, &StyleValue::Number(4.0))
        .unwrap();
    let create = [
        Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 1,
            patch: Box::new(style),
        },
    ];
    kernel.apply(0, 1, &create).unwrap();
    let json = |kernel: &Kernel| -> serde_json::Value {
        serde_json::from_str(&style_json_for(&kernel.node(1).unwrap(), &Env::default()).0).unwrap()
    };
    let sideways = json(&kernel);
    assert!(sideways.get("left").is_none(), "{sideways}");
    assert!(sideways.get("top").is_none(), "{sideways}");
    let mut top = StyleProps::default();
    top.set_dynamic(StyleId::Top, &StyleValue::Number(0.0))
        .unwrap();
    kernel
        .apply(
            0,
            2,
            &[Op::SetStyle {
                id: 1,
                patch: Box::new(top),
            }],
        )
        .unwrap();
    assert_eq!(json(&kernel)["top"], 0);
}

/// LLP 1021 "Placement": a popover placed by `position-area` carries its
/// margins to the host, which aligns its margin box; without the row
/// they stay the kernel's, as every other box's.
#[test]
fn a_placed_popover_carries_its_margins() {
    use exact_kernel::{Kernel, MonospaceMeasurer, Op};
    let mut kernel = Kernel::new(Box::new(MonospaceMeasurer::default()));
    let mut style = StyleProps::default();
    style
        .set_dynamic(StyleId::MarginBottom, &StyleValue::Number(12.0))
        .unwrap();
    style
        .set_dynamic(StyleId::MarginLeft, &StyleValue::Auto)
        .unwrap();
    let create = [
        Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 1,
            patch: Box::new(style),
        },
    ];
    kernel.apply(0, 1, &create).unwrap();
    let json = |kernel: &Kernel| -> serde_json::Value {
        serde_json::from_str(&style_json_for(&kernel.node(1).unwrap(), &Env::default()).0).unwrap()
    };
    assert!(json(&kernel).get("margin_bottom").is_none());
    let mut area = StyleProps::default();
    area.set_dynamic(StyleId::PositionArea, &StyleValue::Text("top".into()))
        .unwrap();
    area.set_dynamic(
        StyleId::PositionTryFallbacks,
        &StyleValue::Text("flip-block, flip-inline, flip-block flip-inline".into()),
    )
    .unwrap();
    kernel
        .apply(
            0,
            2,
            &[Op::SetStyle {
                id: 1,
                patch: Box::new(area),
            }],
        )
        .unwrap();
    let placed = json(&kernel);
    assert_eq!(placed["position_area"], "top", "{placed}");
    assert_eq!(
        placed["position_try_fallbacks"],
        "flip-block, flip-inline, flip-block flip-inline"
    );
    assert_eq!(placed["margin_bottom"], 12, "{placed}");
    assert_eq!(placed["margin_left"], "auto", "{placed}");
}

struct NoData;
impl DataSource for NoData {
    fn query(
        &mut self,
        source: &str,
        _: &[exact_plan::Value],
    ) -> Result<exact_plan::Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

#[test]
fn an_authored_size_is_named_written_and_a_sheets_is_not() {
    let source = r#"component App
  view
    column testId="root" width=402 height=874
      list appearance="auto" testId="list" flex=1
        section
          row testId="sheet"
            image "symbol:star" testId="sheet-star"
            text "Favorites"
          row testId="sized"
            image "symbol:star" width=32 height=32 testId="sized-star"
            text "Favorites"
"#;
    let plan = contract::bake(contract::compile(source).unwrap(), NoData).unwrap();
    let r = Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::new(Box::new(exact_kernel::MonospaceMeasurer::default())),
        Default::default(),
        "/",
    )
    .unwrap();
    let written = |t: &str| -> serde_json::Value {
        let k = r.kernel();
        let node = k.node_by_key(k.find_by_test_id(t)[0]).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&style_json_for(&node, &Env::default()).0).unwrap();
        assert!(json.get("width").is_none(), "the size itself is layout's");
        json.get("written")
            .cloned()
            .unwrap_or(serde_json::json!([]))
    };
    assert_eq!(
        written("sized-star"),
        serde_json::json!(["width", "height"])
    );
    assert_eq!(written("sheet-star"), serde_json::json!([]));
}

#[test]
fn an_authored_animation_is_named_written_and_does_not_cross() {
    let source = r#"keyframes rise
  from opacity=0

component App
  view
    column testId="root"
      text "Badge" testId="moving" animation="rise 200ms ease-out"
      text "Still" testId="still"
"#;
    let plan = contract::bake(contract::compile(source).unwrap(), NoData).unwrap();
    let r = Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::new(Box::new(exact_kernel::MonospaceMeasurer::default())),
        Default::default(),
        "/",
    )
    .unwrap();
    let json = |t: &str| -> serde_json::Value {
        let k = r.kernel();
        let node = k.node_by_key(k.find_by_test_id(t)[0]).unwrap();
        serde_json::from_str(&style_json_for(&node, &Env::default()).0).unwrap()
    };
    let moving = json("moving");
    assert!(moving.get("animation").is_none(), "{moving}");
    assert_eq!(
        moving["written"],
        serde_json::json!(["animation"]),
        "{moving}"
    );
    assert!(json("still").get("written").is_none());
}

#[test]
fn basic_clips_cross_as_relative_geometry() {
    exact_kernel::style::link_all();
    let mut style = StyleProps::default();
    style
        .set_dynamic(
            StyleId::ClipPath,
            &StyleValue::Text("polygon(evenodd, 4px 25%, 100% 0, 0 100%)".into()),
        )
        .unwrap();
    let (text, skipped) = style_json(&style, &Env::default());
    assert!(skipped.is_empty());
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        json["clip_path"],
        serde_json::json!({
            "shape": "polygon", "values": [4,0,0,25,0,100,0,0,0,0,0,100], "rule":"evenodd"
        })
    );
}

#[test]
fn a_scrollers_d14_rows_cross_by_name_in_points_and_words() {
    // @ref LLP 1123 D14 — as `ScrollInsetsIOS.Rows(style:)` reads them: lengths in points, a
    // negative one kept, the policies by their words
    let source = "component App\n  view\n    column testId=\"list\" overflow-y=\"scroll\" -exact-scroll-content-inset=\"10 -20 30 40\" -exact-scroll-inset-adjustment=\"scrollable-axes\" -exact-scroll-indicator-adjustment=\"none\" -exact-scroll-vertical-indicator-insets=\"1 2 3 4\" -exact-scroll-horizontal-indicator-insets=\"5 6 7 8\"\n      text \"a\"\n";
    let plan = contract::compile(source).unwrap().encode();
    let (_host, boot) = crate::Host::boot(
        &plan,
        NoData,
        Box::new(exact_kernel::MonospaceMeasurer::default()),
        400.,
        800.,
    )
    .unwrap();
    let batch: serde_json::Value = serde_json::from_str(&boot).unwrap();
    let style = batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|op| op.get("style"))
        .find(|style| style.get("scroll_inset_adjustment").is_some())
        .unwrap();
    let insets = ["top", "right", "bottom", "left"]
        .map(|e| style[format!("scroll_content_inset_{e}")].as_f64());
    assert_eq!(
        insets,
        [Some(10.0), Some(-20.0), Some(30.0), Some(40.0)],
        "{style}"
    );
    let indicators = ["top", "right", "bottom", "left"]
        .map(|e| style[format!("scroll_vertical_indicator_insets_{e}")].as_f64());
    assert_eq!(
        indicators,
        [Some(1.0), Some(2.0), Some(3.0), Some(4.0)],
        "{style}"
    );
    let horizontal = ["top", "right", "bottom", "left"]
        .map(|e| style[format!("scroll_horizontal_indicator_insets_{e}")].as_f64());
    assert_eq!(
        horizontal,
        [Some(5.0), Some(6.0), Some(7.0), Some(8.0)],
        "{style}"
    );
    assert_eq!(
        style["scroll_inset_adjustment"].as_str(),
        Some("scrollable-axes")
    );
    assert_eq!(style["scroll_indicator_adjustment"].as_str(), Some("none"));
}
