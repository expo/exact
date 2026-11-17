use super::*;

#[test]
fn a_built_worker_keeps_its_template_here_and_builds_its_instance_on_the_owner() {
    fn build(
        template: &Threaded,
        _: Arc<std::sync::atomic::AtomicBool>,
    ) -> crate::placed::Obtain<Threaded> {
        let calls = template.calls.clone();
        Box::new(move || {
            calls.lock().unwrap().push(format!(
                "built:{}",
                std::thread::current().name().unwrap_or("?")
            ));
            Ok(Threaded { calls })
        })
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut placed = Placed::built(
        Threaded {
            calls: calls.clone(),
        },
        Placement::Worker,
        build,
    );
    placed.activate().unwrap();
    let mut store = Store::new("secret.keep rust\nsecret.keep shared", []);
    let t = token(placed.answer(&mut store, "now", &[]).unwrap());
    let outcome = run(placed.dispatch(t, &store));
    let v = value_of(placed.parse(&mut store, "now", &[], outcome));
    assert_eq!(v.as_str(), Some("+"));
    assert_eq!(calls.lock().unwrap()[0], "built:exact-owner");
    // The template stays: replacement and validation go through it.
    assert!(placed.query("x", &[]).is_ok());
    placed.activate_for_validation().unwrap();
    assert_eq!(placed.placement(), Placement::Main);
    assert!(matches!(
        placed.answer(&mut store, "now", &[]).unwrap(),
        Answer::Now(_)
    ));
}

/// A data crate that names no app, as `exact new --rust` writes one.
struct Unnamed;

impl DataSource for Unnamed {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::Number(4.0))
    }
}

#[test]
fn an_unnamed_source_placed_under_the_modules_name_composes_beside_it() {
    let js = Source::new("js");
    let unnamed = Placed::new(Unnamed, Placement::Main);
    assert!(Mixed::new(js.clone(), unnamed, &["js"], &["rust"]).is_err());
    let named = Placed::new(Unnamed, Placement::Main).named("com.exact.test");
    let mut mixed = Mixed::new(js, named, &["js"], &["rust"]).unwrap();
    assert_eq!(mixed.app_id(), "com.exact.test");
    assert_eq!(mixed.query("rust", &[]).unwrap(), Value::Number(4.0));
    let calls = Default::default();
    let own = Placed::new(Threaded { calls }, Placement::Main).named("com.example.other");
    assert_eq!(own.app_id(), "com.exact.test");
}
