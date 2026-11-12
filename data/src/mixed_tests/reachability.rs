use super::*;

#[test]
fn a_rust_entry_does_not_load_an_unreachable_javascript_executor() {
    let mut javascript = Source::new("js");
    javascript.ready = false;
    javascript.failed = true;
    let calls = javascript.calls.clone();
    let mut mixed = Mixed::new(javascript, Source::new("rust"), &["js"], &["rust"]).unwrap();
    let plan = entry("rust");
    mixed.bind(&plan);
    assert!(mixed.ready());
    assert!(mixed.preload().unwrap());
    mixed.activate().unwrap();
    mixed.activate_for_validation().unwrap();
    assert_eq!(mixed.query("rust", &[]).unwrap(), Value::Number(1.0));
    assert_eq!(&*calls.borrow(), &["bind"]);

    let plan = entry("js");
    mixed.bind(&plan);
    assert!(!mixed.ready());
    assert!(mixed.preload().is_err());
}

fn entry(name: &str) -> Plan {
    let mut builder = exact_plan::builder::PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let number = builder.primitive(exact_plan::TypeKind::Number);
    builder.source(name, &[], number);
    builder.finish().unwrap()
}

#[test]
fn a_typescript_canvas_keeps_the_mixed_executor_reachable() {
    struct Canvas(Source);
    impl DataSource for Canvas {
        fn app_id(&self) -> &str {
            self.0.app_id()
        }
        fn canvas_surfaces(&self) -> Vec<(String, usize)> {
            vec![("drawing".into(), 0)]
        }
        fn ready(&self) -> bool {
            false
        }
        fn activate(&mut self) -> Result<(), DataError> {
            self.0.activate()
        }
        fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
            self.0.query(source, args)
        }
    }
    let javascript = Source::new("js");
    let calls = javascript.calls.clone();
    let mut mixed = Mixed::new(Canvas(javascript), Source::new("rust"), &[], &[]).unwrap();
    let mut builder = exact_plan::builder::PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    builder.surface("drawing", &[]);
    mixed.bind(&builder.finish().unwrap());
    assert!(!mixed.ready());
    mixed.activate().unwrap();
    assert_eq!(&*calls.borrow(), &["activate"]);
}
