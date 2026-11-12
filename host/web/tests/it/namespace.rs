//! An element's namespace comes from its node type (LLP 1055 D4): an SVG
//! node is created in SVG's, and every other node in HTML's, whatever its tag
//! begins with. A native module's custom element named `felt-…` is HTML, so
//! the glue attaches its factory to it (LLP 1024 D2).

use exact_runner::{DataError, DataSource, Value};
use exact_web::Host;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

#[test]
fn the_svg_namespace_follows_the_node_type_not_the_tag() {
    let plan = contract::compile(
        r#"component App
  view
    column
      felt-haptics testId="haptics" enabled=true
      svg testId="art" viewBox="0 0 10 10"
        defs
          filter id="soft" testId="filter"
            feGaussianBlur stdDeviation=2 testId="blur"
        rect width=10 height=10 filter="url(#soft)" testId="rect"
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    exact_web::link(exact_web_capabilities::ALL);
    let (_, batch) = Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap();
    let create_for = |test_id: &str| {
        let prop = batch
            .find(&format!("\"data-testid\":\"{test_id}\""))
            .unwrap_or_else(|| panic!("no {test_id} in {batch}"));
        let start = batch[..prop].rfind("{\"op\":\"create\"").unwrap();
        &batch[start..prop]
    };
    const SVG: &str = "\"ns\":\"http://www.w3.org/2000/svg\"";
    let haptics = create_for("haptics");
    assert!(haptics.contains("\"tag\":\"felt-haptics\""), "{haptics}");
    assert!(!haptics.contains("\"ns\""), "{haptics}");
    for (id, tag) in [
        ("art", "svg"),
        ("filter", "filter"),
        ("blur", "feGaussianBlur"),
        ("rect", "rect"),
    ] {
        let create = create_for(id);
        assert!(create.contains(&format!("\"tag\":\"{tag}\"")), "{create}");
        assert!(create.contains(SVG), "{create}");
    }
}
