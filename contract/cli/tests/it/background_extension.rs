//! An image's background extension is `-exact-safe-area="extend"`; the retired
//! `-exact-background-extension` is refused, naming it.
use exact_kernel::{Kernel, PropId};
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Runner};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const IMAGE: &str = r#"image "https://example.com/a.png""#;

fn app(node: &str, attribute: &str) -> String {
    format!("component App\n  state wide = true\n  view\n    column\n      {node} {attribute} width=40 height=40 testId=\"it\"\n")
}

#[test]
fn an_image_extends_with_the_safe_area_attribute() {
    for value in ["extend", "extend top", "extend start end"] {
        let source = app(IMAGE, &format!("-exact-safe-area=\"{value}\""));
        let plan = contract::bake(contract::compile(&source).unwrap(), NoData).unwrap();
        let runner = Runner::boot(
            plan,
            NoData,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let kernel = runner.kernel();
        let node = kernel.node_by_key(kernel.find_by_test_id("it")[0]).unwrap();
        assert_eq!(node.props.str(PropId::SafeArea), Some(value));
    }
}

#[test]
fn the_retired_attribute_is_refused_naming_its_replacement() {
    for (node, value) in [
        (IMAGE, "\"auto\""),
        (IMAGE, "\"none\""),
        ("column", "\"auto\""),
        (IMAGE, r#"(wide ? "auto" : "none")"#),
    ] {
        let error = contract::compile(&app(node, &format!("-exact-background-extension={value}")))
            .unwrap_err();
        assert_eq!(error.id, "lower-attr-value", "{node} {value}: {error}");
        assert!(
            error.message.contains("-exact-safe-area=\"extend\""),
            "{node} {value}: {error}"
        );
    }
}
