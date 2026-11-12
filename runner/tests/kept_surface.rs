use exact_kernel::{Kernel, NodeType};
use exact_plan::{asm::Asm, builder::PlanBuilder, Plan, TypeKind, Value};
use exact_runner::{DataError, DataSource, Runner};

#[derive(Default)]
struct Source {
    ready: bool,
    private: Option<&'static str>,
}
impl DataSource for Source {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("surface reached data: {source}")
    }
    fn ready(&self) -> bool {
        self.ready
    }
    fn grants(&self) -> &str {
        if self.private.is_some() {
            "device.health-read purpose.p"
        } else {
            ""
        }
    }
}
fn boot(plan: Plan, source: Source, snapshot: Vec<(String, Vec<u8>)>) -> Runner<Source> {
    Runner::boot_stored(
        plan,
        source,
        Kernel::with_monospace(),
        snapshot,
        Default::default(),
        "/",
    )
    .unwrap()
}

#[test]
fn kept_surface_publications_seed_until_live_and_disposal_does_not_replace_them() {
    let make = |keep: bool, field: &str, surface: &str| {
        let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        let number = b.primitive(TypeKind::Number);
        let hud = b.record("Hud", &[(field, number)]);
        let name = b.str(surface);
        let mut arg = Asm::new();
        arg.str(name);
        let arg = b.code(arg);
        let resource = b.resource("hud", "exactSurface", &[arg], hud, None);
        b.surface(surface, &[]);
        if keep {
            b.set_resource_keep(resource);
        }
        b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
        b.finish().unwrap()
    };
    let mut first = boot(make(true, "count", "world"), Source::default(), vec![]);
    first
        .set_surface_record("world", Some(r#"{"count":7}"#))
        .unwrap();
    let saved = first.carry().store;
    assert!(!saved.is_empty());
    first.set_surface_record("world", None).unwrap();
    assert_eq!(first.carry().store, saved);
    for ready in [true, false] {
        let mut next = boot(
            make(true, "count", "world"),
            Source {
                ready,
                ..Source::default()
            },
            saved.clone(),
        );
        assert_eq!(
            next.resource("hud"),
            Some(&Value::record(vec![Value::Number(7.)]))
        );
        assert!(next
            .set_surface_record("world", Some(r#"{"count":"bad"}"#))
            .is_err());
        assert_eq!(
            next.resource("hud"),
            Some(&Value::record(vec![Value::Number(7.)]))
        );
        next.set_surface_record("world", Some(r#"{"count":9}"#))
            .unwrap();
        assert_eq!(
            next.resource("hud"),
            Some(&Value::record(vec![Value::Number(9.)]))
        );
        next.set_surface_record("world", None).unwrap();
        assert_eq!(
            next.resource("hud"),
            Some(&Value::record(vec![Value::Number(0.)]))
        );
    }
    for (keep, field, surface, private) in [
        (false, "count", "world", None),
        (true, "renamed", "world", None),
        (true, "count", "world", Some("health-read")),
        (true, "count", "other", None),
    ] {
        let next = boot(
            make(keep, field, surface),
            Source {
                private,
                ..Source::default()
            },
            saved.clone(),
        );
        assert_eq!(
            next.resource("hud"),
            Some(&Value::record(vec![Value::Number(0.)]))
        );
    }
}

#[test]
fn kept_shape_metadata_preserves_a_deep_public_record() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut ty = b.primitive(TypeKind::Number);
    let mut value = Value::Number(7.);
    let mut json = "7".to_owned();
    for depth in 0..25 {
        ty = b.record(&format!("Layer{depth}"), &[("child", ty)]);
        value = Value::record(vec![value]);
        json = format!(r#"{{"child":{json}}}"#);
    }
    let name = b.str("world");
    let mut arg = Asm::new();
    arg.str(name);
    let arg = b.code(arg);
    let resource = b.resource("hud", "exactSurface", &[arg], ty, None);
    b.set_resource_keep(resource);
    b.surface("world", &[]);
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let plan = b.finish().unwrap();
    let mut first = boot(plan.clone(), Source::default(), vec![]);
    first.set_surface_record("world", Some(&json)).unwrap();
    let next = boot(plan, Source::default(), first.carry().store);
    assert_eq!(next.resource("hud"), Some(&value));
}
