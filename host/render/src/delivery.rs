use exact_runner::delivery::Delivery;
use std::path::Path;

pub(crate) fn beside(plan: &Path) -> std::io::Result<Delivery> {
    let bytes = match std::fs::read(plan.with_file_name("app.version.json")) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Delivery::default()),
        Err(e) => return Err(e),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let field = |name| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                std::io::Error::other(format!("app.version.json: {name} must be a string"))
            })
    };
    Ok(Delivery {
        version: field("version")?,
        build_number: field("buildNumber")?,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::Delivery;
    use crate::*;

    struct NoData;
    impl DataSource for NoData {
        fn query(
            &mut self,
            name: &str,
            _: &[exact_plan::Value],
        ) -> Result<exact_plan::Value, exact_runner::DataError> {
            Err(exact_runner::DataError::UnknownSource(name.into()))
        }
    }

    #[test]
    fn delivery_precedes_child_initializers() {
        let delivery = Delivery {
            version: "2.3.4".into(),
            build_number: "56".into(),
            ..Default::default()
        };
        let plan = contract::compile(
            r#"
routes nav
  home "/" render=build activate=never
shape Build
  version: string
component App
  resource build = exactDelivery() as shape Build
  view
    Version(start=build.version)
component Version
  props
    start: string
  state shown = start
  state first = performanceNow()
  view
    text `${shown} at ${first}`
"#,
        )
        .unwrap();
        let page = settle_at(
            &exact_runner::Program::from(&plan),
            &|| NoData,
            Default::default(),
            "/",
            DEADLINE,
            false,
            1700000123456.,
            &delivery,
            None,
        )
        .unwrap();
        let document = exact_web::document::project(&page.runner).unwrap();
        assert!(
            document.root.contains(">2.3.4 at 1700000123456<"),
            "{}",
            document.root
        );
        assert_eq!(page.runner.now_ms(), 1700000123456.);
        let rendered = render_with_at(
            &plan,
            &|| NoData,
            Default::default(),
            "/",
            &Site::default(),
            DEADLINE,
            Ids::Any,
            Projection::Auto,
            1700000123456.,
            &delivery,
        )
        .unwrap();
        assert!(
            rendered.document.root.contains(">2.3.4 at 1700000123456<"),
            "{}",
            rendered.document.root
        );
        assert_eq!(rendered.activate, exact_plan::ActivatePolicy::Never);
        retire(page, &|| NoData);
    }
}
