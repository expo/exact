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

#[test]
fn projected_gradients_and_clips_resolve_within_each_component() {
    let plan = contract::compile(
        r##"component App
  view
    row
      for i in [0, 1] key=i
        svg width=20 height=20
          defs
            linearGradient id="fade" testId=`gradient-${i}`
              stop offset=0 stop-color=(i == 0 ? "#ff0000" : "#0000ff")
            clipPath id="cut" testId=`clip-${i}`
              rect width=(i == 0 ? 8 : 16) height=20
          rect testId=`shape-${i}` width=20 height=20 fill="url(#fade)" clip-path="url(#cut)"
"##,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    exact_web::link(exact_web_capabilities::ALL);
    let (mut host, batch) = Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap();
    let batch: serde_json::Value = serde_json::from_str(&batch).unwrap();
    let document = host.document().unwrap();
    let tree = host.runner_mut().document_tree().unwrap();
    let (direct, _) =
        exact_web::document::project_tree(&tree, host.runner(), Default::default()).unwrap();
    assert_eq!(direct.root, document.root);
    let js = exact_web::document::project_for_js(host.runner()).unwrap();
    for i in 0..2 {
        let op = |name: &str| {
            batch["ops"]
                .as_array()
                .unwrap()
                .iter()
                .find(|op| {
                    op["op"] == "create" && op["props"]["data-testid"] == format!("{name}-{i}")
                })
                .unwrap()
        };
        let shape = op("shape");
        for (name, property) in [("gradient", "fill"), ("clip", "clip-path")] {
            let target = op(name)["id"].as_u64().unwrap();
            assert_eq!(op(name)["props"]["id"], format!("x{target}"));
            let expected = format!("{property}:url(#x{target});");
            assert!(
                shape["css"].as_str().unwrap().contains(&expected),
                "{shape}"
            );
            for root in [&document.root, &js.root] {
                let opening = root
                    .split(&format!("data-view=\"{}\"", shape["id"]))
                    .nth(1)
                    .unwrap()
                    .split('>')
                    .next()
                    .unwrap();
                assert!(opening.contains(&expected), "{opening}");
            }
        }
    }
}
