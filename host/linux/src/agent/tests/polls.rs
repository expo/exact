//! Bound package calls made by a poll during an agent clock operation.
use super::*;
use crate::package::{json, Context, Entry, Json, Package, Reply};

struct Counter {
    context: Context,
    calls: usize,
}

impl Package for Counter {
    fn new(context: Context) -> Self {
        Self { context, calls: 0 }
    }

    fn later(&mut self, name: &str, _: &Json, _: Option<&Json>, reply: Reply) {
        assert_eq!(name, "polled");
        self.calls += 1;
        self.context
            .log(format!("call {} at {}", self.calls, self.context.now()));
        reply.send(json!(self.calls));
    }
}

#[test]
fn a_clock_jump_services_each_polled_package_call_before_replying() {
    let plan = contract::compile(
        "component App\n  resource value = polled() as shape number\n  view\n    text value.toString()\n",
    )
    .unwrap();
    let source = exact_plan::SourcesId(
        plan.sources
            .iter()
            .position(|s| plan.str(s.name) == "polled")
            .unwrap() as u32,
    );
    let mut builder = exact_plan::builder::PlanBuilder::from_plan(plan);
    builder.module(
        source,
        ("counter", "@fixture/counter"),
        "polled",
        false,
        true,
        "never",
        "",
        r#"{"default":0,"poll":{"option":"interval","default":100}}"#,
    );
    let (mut p, error) = Presenter::boot_with(
        &builder.finish().unwrap().encode(),
        (),
        (100., 40.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_packages(vec![Entry::of::<Counter>("counter")]);
    let bound = std::time::Duration::from_secs(2);
    let initial = handle(&mut p, r#"{"op":"clock","data":true}"#);
    assert!(initial.contains("\"settled\":true"), "{initial}");
    assert_eq!(
        p.host().runner().resource("value"),
        Some(&Value::Number(1.))
    );

    // A poll is outside the timer wheel: crossing several periods still asks once,
    // at the landing time, and seeking to that same time asks nothing more.
    for (to, calls) in [(100, 2), (200, 3), (1000, 4), (1000, 4)] {
        let reply = clock_within(&mut p, &format!(r#"{{"op":"clock","to":{to}}}"#), bound);
        assert_eq!(p.host().now(), f64::from(to), "{reply}");
        assert_eq!(
            p.host().runner().resource("value"),
            Some(&Value::Number(f64::from(calls))),
            "the poll's reply must commit before the clock answers: {reply}"
        );
        assert!(!p.host().runner().polls_asking(), "{reply}");
    }
    let calls: Vec<_> = p
        .host()
        .runner()
        .journal()
        .filter(|line| line.contains("counter: call "))
        .collect();
    assert_eq!(
        calls,
        [
            "t=0 counter: call 1 at 0",
            "t=100 counter: call 2 at 100",
            "t=200 counter: call 3 at 200",
            "t=1000 counter: call 4 at 1000"
        ]
    );
}
