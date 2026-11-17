use exact_kernel::{Kernel, PropId};
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Runner};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("unexpected {name}")
    }
}

const SOURCE: &str = include_str!("../fixtures/svg-lengths.contract");

const ATTRS: &[(&str, &[PropId])] = &[
    ("line", &[PropId::X1, PropId::Y1, PropId::X2, PropId::Y2]),
    ("linear", &[PropId::X1, PropId::Y1, PropId::X2, PropId::Y2]),
    ("radial", &[PropId::Fx, PropId::Fy, PropId::Fr]),
    (
        "marker",
        &[
            PropId::MarkerWidth,
            PropId::MarkerHeight,
            PropId::RefX,
            PropId::RefY,
        ],
    ),
    (
        "text",
        &[PropId::TextX, PropId::TextY, PropId::TextDx, PropId::TextDy],
    ),
    (
        "span",
        &[PropId::TextX, PropId::TextY, PropId::TextDx, PropId::TextDy],
    ),
];

#[test]
fn dynamic_svg_lengths_are_unset_and_diagnosed_then_recover() {
    let mut r = Runner::boot(
        contract::compile(SOURCE).unwrap(),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for (id, props) in [
        (
            "invalid-literal-line",
            &[PropId::X1, PropId::Y1, PropId::X2][..],
        ),
        ("invalid-literal-text", &[PropId::TextX][..]),
    ] {
        let node = r
            .kernel()
            .node_by_key(r.kernel().find_first_by_test_id(id).unwrap())
            .unwrap();
        for &prop in props {
            assert_eq!(node.props.str(prop), None);
        }
    }
    for refused in [
        "invalid x1 value \"\"; unset",
        "invalid y1 value \"10px 20px\"; unset",
        "invalid x2 value \"1e9999\"; unset",
        "invalid textX value \"\"; unset",
    ] {
        assert!(r
            .journal()
            .any(|line| line.contains("runtime-svg-length:") && line.ends_with(refused)));
    }
    for value in [
        "2px",
        "2em",
        "3CM",
        "calc(1px + 2px)",
        "50%",
        "env(safe-area-inset-top)",
        "1e2pt",
        "var(--x)",
        "10px, 2em",
        "0",
    ] {
        r.act("set", vec![Value::str(value)]).unwrap();
        let valid = matches!(value, "2px" | "3CM" | "50%" | "1e2pt" | "0");
        for &(id, props) in ATTRS {
            let node = r
                .kernel()
                .node_by_key(r.kernel().find_first_by_test_id(id).unwrap())
                .unwrap();
            for &prop in props {
                assert_eq!(
                    node.props.str(prop),
                    valid.then_some(value),
                    "{id} {prop:?}: {value}"
                );
            }
        }
        assert!(!r.is_poisoned());
        for (id, prop) in [
            ("marker", PropId::Orient),
            ("offset", PropId::Offset),
            ("primitive", PropId::FeDx),
            ("primitive", PropId::FeDy),
        ] {
            let node = r
                .kernel()
                .node_by_key(r.kernel().find_first_by_test_id(id).unwrap())
                .unwrap();
            assert_eq!(node.props.str(prop), Some(value));
        }
        if !valid {
            assert!(
                r.journal()
                    .any(|line| line.contains("runtime-svg-length") && line.contains(value)),
                "{value}"
            );
        }
    }
    for value in [
        "1in",
        "1cm",
        "1mm",
        "1pt",
        "1pc",
        "-1.5e-2PX",
        " 4px ",
        "1 2px, 3%",
        "center",
        "left",
        "right",
        "top",
        "bottom",
    ] {
        r.act("set", vec![Value::str(value)]).unwrap();
        for &(id, props) in ATTRS {
            let node = r
                .kernel()
                .node_by_key(r.kernel().find_first_by_test_id(id).unwrap())
                .unwrap();
            for &prop in props {
                let valid = match value {
                    "1 2px, 3%" => matches!(id, "text" | "span"),
                    "center" => matches!(prop, PropId::RefX | PropId::RefY),
                    "left" | "right" => prop == PropId::RefX,
                    "top" | "bottom" => prop == PropId::RefY,
                    _ => true,
                };
                assert_eq!(
                    node.props.str(prop),
                    valid.then_some(value),
                    "{id} {prop:?}: {value}"
                );
            }
        }
    }
    for unit in ["em", "px", "vh", "%"] {
        r.act("assemble", vec![Value::str(unit)]).unwrap();
        let node = r
            .kernel()
            .node_by_key(r.kernel().find_first_by_test_id("assembled").unwrap())
            .unwrap();
        let expected = match unit {
            "px" => Some("2px"),
            "%" => Some("2%"),
            _ => None,
        };
        assert_eq!(node.props.str(PropId::X1), expected);
        assert_eq!(node.props.str(PropId::Y1), expected);
    }
    let initial = SOURCE.replace("state value = \"2px\"", "state value = \"2em\"");
    let r = Runner::boot(
        contract::compile(&initial).unwrap(),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let node = r
        .kernel()
        .node_by_key(r.kernel().find_first_by_test_id("line").unwrap())
        .unwrap();
    assert_eq!(node.props.str(PropId::X1), None);
    assert!(r
        .journal()
        .any(|line| line.contains("runtime-svg-length") && line.contains("2em")));
}

#[test]
fn the_agent_tree_reports_svg_geometry() {
    let source = r#"component App
  state radius = 8
  action grow
    radius = 12
  view
    svg width=100 height=100 viewBox="0 0 100 100"
      circle testId="circle" cx=20 cy=30 r=radius
      circle testId="percent" cx="25%" cy="40%" r="10%"
      circle testId="relative" font-size=16 cx="2em" cy="10vh" r="2em"
      circle testId="viewport" cx="calc(10% + 2px)" cy="env(safe-area-inset-left)" r="10vh"
      ellipse testId="ellipse" cx=40 cy=50 rx=10 ry=5
      rect testId="rect" x=2 y=3 width=40 height=20 rx=4 ry=2
      line testId="line" x1=1 y1=2 x2=3 y2=4
"#;
    let mut runner = Runner::boot(
        contract::compile(source).unwrap(),
        NoData,
        Kernel::with_monospace(),
        exact_runner::Viewport::sized(200.0, 400.0),
        "/",
    )
    .unwrap();
    for radius in [8, 12] {
        for root in runner.roots() {
            runner
                .kernel_mut()
                .compute_layout(root, exact_kernel::Offer::definite(200.0, 400.0))
                .unwrap();
        }
        let tree: serde_json::Value =
            serde_json::from_str(&exact_runner::agent::tree(&runner)).unwrap();
        let props = |id: &str| {
            tree["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["props"]["testId"] == id)
                .unwrap()["props"]
                .clone()
        };
        for (id, expected) in [
            ("circle", serde_json::json!({"cx":20, "cy":30, "r":radius})),
            (
                "percent",
                serde_json::json!({"cx":"25%", "cy":"40%", "r":"10%"}),
            ),
            (
                "relative",
                serde_json::json!({"cx":32, "cy":"10vh", "r":32}),
            ),
            (
                "viewport",
                serde_json::json!({"cx":"calc(10% + 2px)", "cy":"env(safe-area-inset-left)", "r":"10vh"}),
            ),
            (
                "ellipse",
                serde_json::json!({"cx":40, "cy":50, "rx":10, "ry":5}),
            ),
            (
                "rect",
                serde_json::json!({"x":2, "y":3, "width":40, "height":20, "rx":4, "ry":2}),
            ),
            (
                "line",
                serde_json::json!({"x1":"1", "y1":"2", "x2":"3", "y2":"4"}),
            ),
        ] {
            for (key, value) in expected.as_object().unwrap() {
                assert_eq!(&props(id)[key], value, "{id}.{key}: {}", props(id));
            }
        }
        let circle = tree["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["props"]["testId"] == "circle")
            .unwrap();
        let detail: serde_json::Value = serde_json::from_str(&exact_runner::agent::node(
            &runner,
            circle["id"].as_u64().unwrap() as u32,
        ))
        .unwrap();
        assert_eq!(detail["props"], circle["props"]);
        runner.act("grow", vec![]).unwrap();
    }
}
