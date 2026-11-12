use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    Answer, DataError, DataSource, Message, Outcome, Request, Response, Runner, RunnerError, Store,
};
use std::{cell::Cell, rc::Rc};

struct Calls(Rc<Cell<usize>>, Rc<Cell<bool>>);

impl DataSource for Calls {
    fn ready(&self) -> bool {
        self.1.get()
    }

    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        assert!(self.ready(), "the source ran before activation");
        self.0.set(self.0.get() + 1);
        Ok(Value::str("answer"))
    }

    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        let value = self.query(source, args)?;
        Ok(if source == "saveLater" {
            Answer::Later(Request::get("https://example.test/save"))
        } else {
            Answer::Now(value)
        })
    }
}

fn boot(source: &str) -> (Runner<Calls>, Rc<Cell<usize>>) {
    boot_ready(source, Rc::new(Cell::new(true)))
}

fn boot_ready(source: &str, ready: Rc<Cell<bool>>) -> (Runner<Calls>, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    let runner = Runner::boot(
        contract::compile(source).unwrap(),
        Calls(calls.clone(), ready),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    (runner, calls)
}

fn refused(error: RunnerError, target: &str) {
    assert!(
        matches!(error, RunnerError::Data { ref resource, error: DataError::BadArguments(ref why) }
            if resource == target && why == "argument 2 contains a non-finite number"),
        "{error:?}"
    );
}

const EXPRESSIONS: &[&str] = &[
    "1 / b",
    "-1 / b",
    "0 / b",
    "[1 / b]",
    "some(1 / b)",
    "Payload(values=[some(1 / b)])",
];

#[test]
fn non_finite_resource_arguments_refuse_the_commit_without_calling_the_source() {
    for expr in EXPRESSIONS {
        let (mut runner, calls) = boot(&format!(
            r#"
shape Payload
  values: option<number>[]
component App
  state b = 1
  resource value = read("key") with {expr} as shape string
  action change(n: number)
    b = n
  view
    text value
"#
        ));
        assert_eq!(calls.get(), 1, "{expr}: the finite input is accepted");
        refused(
            runner.act("change", vec![Value::Number(0.)]).unwrap_err(),
            "value",
        );
        assert_eq!(calls.get(), 1, "{expr}: the source was not called");
        assert_eq!(runner.slot("b"), Some(&Value::Number(1.)));
        assert_eq!(runner.resource("value"), Some(&Value::str("answer")));
        assert!(!runner.has_pending());
        runner.act("change", vec![Value::Number(2.)]).unwrap();
        assert_eq!(runner.slot("b"), Some(&Value::Number(2.)));
    }
}

#[test]
fn non_finite_context_cannot_reuse_an_upload() {
    struct Upload {
        calls: Rc<Cell<usize>>,
        parsed: Rc<Cell<usize>>,
    }
    impl DataSource for Upload {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            unreachable!()
        }

        fn answer(
            &mut self,
            _: &mut Store,
            source: &str,
            _: &[Value],
        ) -> Result<Answer, DataError> {
            if source == "saveLater" {
                return Ok(Answer::Later(Request::get("https://example.test/save")));
            }
            self.calls.set(self.calls.get() + 1);
            Ok(Answer::stream(
                Request::post_json("https://example.test/upload", "{}").upload_progress(),
            ))
        }

        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            args: &[Value],
            _: Outcome,
        ) -> Result<Answer, DataError> {
            assert_eq!(args[1], Value::Number(1.));
            self.parsed.set(self.parsed.get() + 1);
            Ok(Answer::Now(Value::str("answer")))
        }
    }
    for completed in [false, true] {
        for reconciling in [false, true] {
            let calls = Rc::new(Cell::new(0));
            let parsed = Rc::new(Cell::new(0));
            let mut runner = Runner::boot(
                contract::compile(
                    r#"
component App
  state b = 1
  resource value = up("key") with (1 / b) as shape string
  mutation result as shape string refreshes value
  derive broken = failed(value)
  action write
    send result = saveLater()
  action change
    b = 0
  action cancel
    b = 0
    result = none
  view
    text value
"#,
                )
                .unwrap(),
                Upload {
                    calls: calls.clone(),
                    parsed: parsed.clone(),
                },
                Kernel::with_monospace(),
                Default::default(),
                "/",
            )
            .unwrap();
            let upload = runner.take_requests().pop().unwrap();
            runner
                .fulfill(upload.ticket, Outcome::Message(Message::upload_start()))
                .unwrap();
            let reply = || {
                Outcome::Response(Response {
                    status: 201,
                    headers: vec![],
                    body: vec![],
                })
            };
            if completed {
                runner.fulfill(upload.ticket, reply()).unwrap();
            }
            if reconciling {
                runner.act("write", vec![]).unwrap();
                runner.take_requests();
                runner.act("cancel", vec![]).unwrap();
                assert_eq!(runner.slot("b"), Some(&Value::Number(0.)));
                assert_eq!(runner.derive("broken"), Some(&Value::Bool(true)));
                assert!(!runner.has_pending());
            } else {
                refused(runner.act("change", vec![]).unwrap_err(), "value");
                assert_eq!(runner.slot("b"), Some(&Value::Number(1.)));
                assert_eq!(runner.derive("broken"), Some(&Value::Bool(false)));
                assert_eq!(runner.has_pending(), !completed);
            }
            assert_eq!(calls.get(), 1);
            assert!(runner.take_requests().is_empty());
            if !completed {
                runner.fulfill(upload.ticket, reply()).unwrap();
            }
            assert_eq!(parsed.get(), usize::from(completed || !reconciling));
            assert_eq!(runner.derive("broken"), Some(&Value::Bool(reconciling)));
        }
    }
}

#[test]
fn non_finite_send_arguments_refuse_the_commit_without_calling_the_source() {
    for expr in EXPRESSIONS {
        let (mut runner, calls) = boot(&format!(
            r#"
shape Payload
  values: option<number>[]
component App
  state changed = false
  mutation result as shape string
  action write(b: number)
    changed = true
    send result = save("key", {expr})
  view
    text "arguments"
"#
        ));
        refused(
            runner.act("write", vec![Value::Number(0.)]).unwrap_err(),
            "result",
        );
        assert_eq!(calls.get(), 0, "{expr}: the source was not called");
        assert_eq!(runner.slot("changed"), Some(&Value::Bool(false)));
        assert_eq!(runner.slot("result"), Some(&Value::NONE));
        assert!(!runner.has_pending());
        runner.act("write", vec![Value::Number(1.)]).unwrap();
        assert_eq!(calls.get(), 1, "{expr}: the finite input is accepted");
        assert_eq!(
            runner.slot("result"),
            Some(&Value::some(Value::str("answer")))
        );
    }
}

#[test]
fn non_finite_sends_refuse_before_activation_and_valid_sends_wait() {
    for expr in EXPRESSIONS {
        let ready = Rc::new(Cell::new(false));
        let (mut runner, calls) = boot_ready(
            &format!(
                r#"
shape Payload
  values: option<number>[]
component App
  state changed = false
  mutation result as shape string
  derive waiting = pending(result)
  action write(b: number)
    changed = true
    send result = save("key", {expr})
  view
    text "arguments"
"#
            ),
            ready.clone(),
        );
        let invalid = runner.act("write", vec![Value::Number(0.)]);
        ready.set(true);
        runner.data_ready().unwrap();
        assert_eq!(
            calls.get(),
            0,
            "{expr}: activation must not call the source"
        );
        refused(invalid.unwrap_err(), "result");
        assert_eq!(runner.slot("changed"), Some(&Value::Bool(false)));
        assert_eq!(runner.slot("result"), Some(&Value::NONE));
        assert_eq!(runner.mutation_requests(), 0);
        assert_eq!(runner.derive("waiting"), Some(&Value::Bool(false)));

        ready.set(false);
        runner.act("write", vec![Value::Number(1.)]).unwrap();
        assert_eq!(calls.get(), 0);
        assert_eq!(runner.derive("waiting"), Some(&Value::Bool(true)));
        ready.set(true);
        runner.data_ready().unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(runner.slot("changed"), Some(&Value::Bool(true)));
        assert_eq!(
            runner.slot("result"),
            Some(&Value::some(Value::str("answer")))
        );
        assert_eq!(runner.derive("waiting"), Some(&Value::Bool(false)));
    }
}

fn deferred_resource_arguments(baked: bool, seeded: bool) {
    for expr in EXPRESSIONS {
        let plan = contract::compile(&format!(
            r#"
shape Payload
  values: option<number>[]
component App
  state b = 1 persist
  resource value = read("key") with {expr} as shape string
  mutation result as shape string
  derive waiting = pending(result)
  action write
    send result = save("key", 1)
  action change(n: number)
    b = n
  view
    text value
"#
        ))
        .unwrap();
        let mut plan = if baked {
            contract::bake(plan, Calls(Rc::new(Cell::new(0)), Rc::new(Cell::new(true)))).unwrap()
        } else {
            plan
        };
        let snapshot = if seeded {
            plan.resources[0].reader = true;
            let mut entry = Value::list(vec![Value::str("key")]).to_bytes();
            entry.extend(Value::str("kept").to_bytes());
            vec![("exact.kept.value".into(), entry)]
        } else {
            vec![]
        };
        let ready = Rc::new(Cell::new(false));
        let calls = Rc::new(Cell::new(0));
        let mut invalid_snapshot = snapshot.clone();
        invalid_snapshot.push(("exact.state.b".into(), b"0".to_vec()));
        let invalid_boot = Runner::boot_stored(
            plan.clone(),
            Calls(calls.clone(), ready.clone()),
            Kernel::with_monospace(),
            invalid_snapshot,
            Default::default(),
            "/",
        );
        let mut runner = Runner::boot_stored(
            plan,
            Calls(calls.clone(), ready.clone()),
            Kernel::with_monospace(),
            snapshot,
            Default::default(),
            "/",
        )
        .unwrap();
        if seeded {
            assert_eq!(runner.resource("value"), Some(&Value::str("kept")));
        }
        runner.act("write", vec![]).unwrap();
        let invalid = runner.act("change", vec![Value::Number(0.)]);
        assert_eq!(calls.get(), 0, "{expr}: no call before activation");
        refused(invalid.unwrap_err(), "value");
        refused(invalid_boot.err().expect("invalid boot refused"), "value");
        assert_eq!(runner.slot("b"), Some(&Value::Number(1.)));
        assert_eq!(runner.derive("waiting"), Some(&Value::Bool(true)));
        assert!(runner.failed_resources().is_empty());
        ready.set(true);
        runner.data_ready().unwrap();
        assert_eq!(calls.get(), 2, "{expr}: the resource and waiting send run");
        assert_eq!(runner.resource("value"), Some(&Value::str("answer")));
        assert_eq!(
            runner.slot("result"),
            Some(&Value::some(Value::str("answer")))
        );
        assert_eq!(runner.derive("waiting"), Some(&Value::Bool(false)));
        assert!(runner.failed_resources().is_empty());
    }
}

#[test]
fn non_finite_resource_arguments_refuse_before_deferring_a_placeholder() {
    deferred_resource_arguments(false, false);
}

#[test]
fn non_finite_resource_arguments_refuse_before_deferring_a_baked_answer() {
    deferred_resource_arguments(true, false);
}

#[test]
fn non_finite_resource_context_refuses_before_reusing_a_kept_answer() {
    deferred_resource_arguments(false, true);
}

#[test]
fn non_finite_resource_arguments_fail_reconciliation_without_undoing_cancellation() {
    for expr in EXPRESSIONS.iter().copied().chain([
        "[0 / b]",
        "some(0 / b)",
        "Payload(values=[some(0 / b)])",
        "0 / b + count * 0",
        "[0 / b + count * 0]",
        "Payload(values=[some(0 / b + count * 0)])",
    ]) {
        cancelled_resource_arguments(expr);
    }
}

#[test]
fn signed_zero_keeps_a_reconciliation_failure_until_the_arguments_change() {
    struct Failing {
        fail: Rc<Cell<bool>>,
        calls: Rc<Cell<usize>>,
    }
    impl DataSource for Failing {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            self.calls.set(self.calls.get() + 1);
            if self.fail.get() {
                Err(DataError::Unavailable("reconciliation failed".into()))
            } else {
                Ok(Value::str("answer"))
            }
        }

        fn answer(
            &mut self,
            _: &mut Store,
            source: &str,
            args: &[Value],
        ) -> Result<Answer, DataError> {
            if source == "saveLater" {
                Ok(Answer::Later(Request::get("https://example.test/save")))
            } else {
                self.query(source, args).map(Answer::Now)
            }
        }
    }
    for expr in ["b", "[b]", "some(b)", "Payload(values=[some(b)])"] {
        let fail = Rc::new(Cell::new(false));
        let calls = Rc::new(Cell::new(0));
        let plan = contract::compile(&format!(
            r#"
shape Payload
  values: option<number>[]
component App
  state b = 0
  resource value = read({expr}) as shape string
  mutation result as shape string refreshes value
  derive broken = failed(value)
  action write
    send result = saveLater()
  action cancel
    result = none
  action flip
    b = -b
  action recover
    b = 1
  view
    text value
"#
        ))
        .unwrap();
        let mut runner = Runner::boot(
            plan,
            Failing {
                fail: fail.clone(),
                calls: calls.clone(),
            },
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        runner.act("write", vec![]).unwrap();
        fail.set(true);
        runner.act("cancel", vec![]).unwrap();
        assert_eq!(runner.derive("broken"), Some(&Value::Bool(true)));
        assert_eq!(calls.get(), 2);
        fail.set(false);
        runner.act("flip", vec![]).unwrap();
        assert_eq!(runner.derive("broken"), Some(&Value::Bool(true)), "{expr}");
        assert_eq!(calls.get(), 2, "signed zero does not retry: {expr}");
        runner.act("recover", vec![]).unwrap();
        assert_eq!(runner.derive("broken"), Some(&Value::Bool(false)));
        assert_eq!(calls.get(), 3);
    }
}

fn cancelled_resource_arguments(expr: &str) {
    for deferred in [false, true] {
        let ready = Rc::new(Cell::new(!deferred));
        let (mut runner, calls) = boot_ready(
            &format!(
                r#"
shape Payload
  values: option<number>[]
component App
  state b = 1
  state count = 0
  resource value = read("key", {expr}) as shape string
  mutation result as shape string refreshes value
  derive waiting = pending(result)
  derive broken = failed(value)
  action write
    send result = saveLater()
  action cancel
    b = 0
    result = none
  action recover
    b = 2
    refresh value
  action unrelated
    count = count + 1
  view
    text value
"#
            ),
            ready.clone(),
        );
        runner.act("write", vec![]).unwrap();
        assert_eq!(runner.derive("waiting"), Some(&Value::Bool(true)));
        let expected_calls = if deferred { 0 } else { 2 };
        assert_eq!(calls.get(), expected_calls);
        runner.act("cancel", vec![]).unwrap();
        assert_eq!(
            calls.get(),
            expected_calls,
            "reconciliation must not call the source"
        );
        assert_eq!(runner.slot("b"), Some(&Value::Number(0.)));
        assert_eq!(runner.slot("result"), Some(&Value::NONE));
        assert_eq!(runner.derive("waiting"), Some(&Value::Bool(false)));
        assert_eq!(runner.derive("broken"), Some(&Value::Bool(true)));
        assert_eq!(
            runner.resource("value"),
            Some(&Value::str(if deferred { "" } else { "answer" }))
        );
        assert!(runner
            .journal()
            .any(|line| line.contains("argument 2 contains a non-finite number")));
        runner.act("unrelated", vec![]).unwrap();
        assert_eq!(runner.slot("count"), Some(&Value::Number(1.)));
        assert_eq!(runner.slot("b"), Some(&Value::Number(0.)));
        assert_eq!(runner.slot("result"), Some(&Value::NONE));
        assert_eq!(runner.derive("waiting"), Some(&Value::Bool(false)));
        assert_eq!(runner.derive("broken"), Some(&Value::Bool(true)));
        assert_eq!(calls.get(), expected_calls);
        ready.set(true);
        runner.data_ready().unwrap();
        runner.act("unrelated", vec![]).unwrap();
        assert_eq!(runner.slot("count"), Some(&Value::Number(2.)));
        assert_eq!(runner.derive("broken"), Some(&Value::Bool(true)));
        assert_eq!(
            calls.get(),
            expected_calls,
            "the cancelled send must not activate"
        );
        runner.act("recover", vec![]).unwrap();
        assert_eq!(calls.get(), expected_calls + 1);
        assert_eq!(runner.derive("broken"), Some(&Value::Bool(false)));
    }
}
