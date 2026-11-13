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
