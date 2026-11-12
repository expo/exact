//! Source arguments have the same domain in either language and placement.
#![cfg(exact_js_engine)]

use exact_data::Placed;
use exact_js::Module;
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Placement, Runner, RunnerError};

struct Kind;

impl DataSource for Kind {
    fn query(&mut self, _: &str, args: &[Value]) -> Result<Value, DataError> {
        let n = args[0].as_number().unwrap();
        Ok(Value::str(if n.is_infinite() {
            "infinite"
        } else if n.is_nan() {
            "nan"
        } else {
            "finite"
        }))
    }
}

fn boot(source: impl DataSource, expr: &str) -> Result<Option<Value>, RunnerError> {
    let plan = contract::compile(&format!(
        r#"
component App
  state b = 0
  resource kind = spin({expr}) as shape string
  view
    text kind testId="kind"
"#
    ))
    .unwrap();
    let mut source = source;
    source.bind(&plan);
    source.activate().unwrap();
    let runner = Runner::boot(
        plan,
        source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )?;
    Ok(runner.resource("kind").cloned())
}

#[test]
fn non_finite_resource_arguments_are_refused_before_either_executor_runs() {
    for expr in ["1 / b", "-1 / b", "0 / b"] {
        for placement in [Placement::Main, Placement::Worker] {
            let module = Module::new(
                include_bytes!(concat!(env!("OUT_DIR"), "/timers.hbc")).to_vec(),
                "test.timers",
                "net.fetch https://fixture.exact.test\nsecret.keep shared\n",
            );
            for result in [
                boot(Placed::new(Kind, placement), expr),
                boot(module.placed(placement), expr),
            ] {
                assert!(
                    matches!(result, Err(RunnerError::Data { ref resource, error: DataError::BadArguments(ref why) })
                        if resource == "kind" && why == "argument 1 contains a non-finite number"),
                    "{expr} on {placement:?}: {result:?}"
                );
            }
        }
    }
}
