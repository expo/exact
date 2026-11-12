use super::*;
use exact_plan::asm::Asm;
use exact_plan::builder::PlanBuilder;

pub(super) fn run(body: Asm) -> Result<Outcome, Trap> {
    run_reusing(body, &mut Scratch::default())
}

fn run_reusing(body: Asm, scratch: &mut Scratch) -> Result<Outcome, Trap> {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let code = b.code(body);
    let plan = b.finish().unwrap();
    let strings = intern(&plan);
    let env = Env {
        plan: &plan,
        strings: &strings,
        router: None,
        lists: None,
        format: None,
        geometry: None,
        slots: &[],
        derives: &[],
        resources: (&[]).into(),
        params: &[],
        frames: &[],
        now_ms: 0.0,
        pending_resources: (&[]).into(),
        current_resources: (&[]).into(),
        failed_resources: (&[]).into(),
        failed_why: (&[]).into(),
        pending_mutations: &[],
        failed_mutations: &[],
        store_dependent_derives: &[],
        store_dependent_resources: (&[]).into(),
    };
    eval_reusing(plan.code(code), &env, &[], scratch)
}

#[test]
fn reused_storage_does_not_carry_locals_stacks_or_failed_callbacks() {
    let mut scratch = Scratch::default();
    for _ in 0..100 {
        let mut local = Asm::new();
        local.number(2.0).bind_local().number(7.0);
        assert_eq!(
            run_reusing(local, &mut scratch).unwrap().value,
            Value::Number(7.0)
        );
        let mut read = Asm::new();
        read.load_local(0);
        assert!(matches!(
            run_reusing(read, &mut scratch),
            Err(Trap::BadScope { .. })
        ));
        let mut pop = Asm::new();
        pop.simple(Opcode::Pop);
        assert!(matches!(
            run_reusing(pop, &mut scratch),
            Err(Trap::StackUnderflow { .. })
        ));
        let mut callback = Asm::new();
        let end = callback.label();
        ones(&mut callback, 2)
            .each_item(Opcode::Map, end)
            .number(1.0)
            .simple(Opcode::Return)
            .place(end);
        assert!(matches!(
            run_reusing(callback, &mut scratch),
            Err(Trap::Malformed { .. })
        ));
        let mut valid = Asm::new();
        valid.number(41.0).number(1.0).simple(Opcode::Add);
        assert_eq!(
            run_reusing(valid, &mut scratch).unwrap().value,
            Value::Number(42.0)
        );
    }
}

/// `x` on the stack becomes `[x, x]`, `times` times, through a local.
fn doubled(body: &mut Asm, times: usize) -> &mut Asm {
    for _ in 0..times {
        body.bind_local()
            .load_local(0)
            .load_local(0)
            .list(2)
            .drop_local();
    }
    body
}

fn measured(f: impl FnOnce()) -> u64 {
    MEASURED.with(|m| m.set(0));
    f();
    MEASURED.with(|m| m.get())
}

/// Doubling through a local makes one instruction's value exponential in
/// instructions: 2^k leaves after k doublings, which equality, shape
/// checks and encoding then walk leaf by leaf. The VM refuses the
/// doubling that passes `MAX_VALUE_NODES`, and sizing a shared part
/// costs one lookup, not its expansion.
#[test]
fn a_doubled_list_is_refused_where_it_is_built() {
    // k doublings of a number hold 2^(k+1) - 1 values: 23 fit in 2^24.
    let mut body = Asm::new();
    body.number(1.0);
    doubled(&mut body, 23);
    let mut value = Value::Unit;
    let walked = measured(|| value = run(body).unwrap().value);
    assert!(walked < 1_000, "sizing 23 doublings walked {walked} values");
    let (mut leaves, mut v) = (1u64, &value);
    while let Value::List(items) = v {
        assert_eq!(items.len(), 2);
        leaves *= 2;
        v = &items[0];
    }
    assert_eq!(leaves, 1 << 23);

    let mut body = Asm::new();
    body.number(1.0);
    doubled(&mut body, 64);
    match run(body) {
        // The 24th doubling's `List`: `Number` is 9 bytes, a doubling
        // 13 (bind 1, load 3, load 3, list 5, drop 1).
        Err(Trap::ValueTooLarge { pc }) => assert_eq!(pc, 9 + 23 * 13 + 7),
        other => panic!("{other:?}"),
    }
}

/// A shared string counts its bytes wherever it appears: 64 copies of a
/// 1 MiB string fit `MAX_VALUE_BYTES`, 128 do not.
#[test]
fn a_shared_string_counts_its_bytes_in_every_place() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let x = b.str("x");
    let mut fits = Asm::new();
    fits.str(x);
    for _ in 0..20 {
        fits.bind_local()
            .load_local(0)
            .load_local(0)
            .op(exact_plan::Opcode::Concat, &[])
            .drop_local();
    }
    let mut refused = Asm::new();
    refused.str(x);
    for _ in 0..20 {
        refused
            .bind_local()
            .load_local(0)
            .load_local(0)
            .op(exact_plan::Opcode::Concat, &[])
            .drop_local();
    }
    doubled(&mut fits, 6);
    doubled(&mut refused, 7);
    let fits = b.code(fits);
    let refused = b.code(refused);
    let plan = b.finish().unwrap();
    let strings = intern(&plan);
    let env = Env {
        plan: &plan,
        strings: &strings,
        router: None,
        lists: None,
        format: None,
        geometry: None,
        slots: &[],
        derives: &[],
        resources: (&[]).into(),
        params: &[],
        frames: &[],
        now_ms: 0.0,
        pending_resources: (&[]).into(),
        current_resources: (&[]).into(),
        failed_resources: (&[]).into(),
        failed_why: (&[]).into(),
        pending_mutations: &[],
        failed_mutations: &[],
        store_dependent_derives: &[],
        store_dependent_resources: (&[]).into(),
    };
    assert!(eval(plan.code(fits), &env, &[]).is_ok());
    assert!(matches!(
        eval(plan.code(refused), &env, &[]),
        Err(Trap::ValueTooLarge { .. })
    ));
}

/// A `some` adds a level and no value (LLP 1090 D2): 23 doublings each
/// wrapped in one fit as the bare doublings do (they would hold
/// 3 · 2^23 values counted the old way), and the 24th still traps.
#[test]
fn a_some_measures_as_its_value() {
    let wrapped = |times: usize| {
        let mut body = Asm::new();
        body.number(1.0);
        for _ in 0..times {
            doubled(&mut body, 1).simple(exact_plan::Opcode::Some);
        }
        run(body)
    };
    assert!(wrapped(23).is_ok());
    assert!(matches!(wrapped(24), Err(Trap::ValueTooLarge { .. })));
}

/// The VM nests no deeper than `Value::decode` reads: 64 `some`s around a
/// number round-trip through the canonical bytes, a 65th is refused.
#[test]
fn nesting_stops_where_decoding_does() {
    let nested = |n: usize| {
        let mut body = Asm::new();
        body.number(1.0);
        for _ in 0..n {
            body.simple(exact_plan::Opcode::Some);
        }
        run(body)
    };
    let deepest = nested(64).unwrap().value;
    assert_eq!(Value::from_bytes(&deepest.to_bytes()).unwrap(), deepest);
    assert!(matches!(nested(65), Err(Trap::ValueTooDeep { .. })));
    let mut too_deep = deepest.clone();
    too_deep = Value::some(too_deep);
    assert!(Value::from_bytes(&too_deep.to_bytes()).is_err());
}

/// Push a list of `n` ones.
fn ones(body: &mut Asm, n: u32) -> &mut Asm {
    for _ in 0..n {
        body.number(1.0);
    }
    body.list(n)
}

/// `map(outer, x => map(inner, y => 1))`, then, when `extra`, one more
/// `map` over a one-item list; its value is the nested lists' length.
fn nested(outer: u32, inner: u32, extra: bool) -> Result<Outcome, Trap> {
    let mut body = Asm::new();
    ones(&mut body, inner).bind_local();
    ones(&mut body, outer);
    let (end, end_inner) = (body.label(), body.label());
    body.each_item(exact_plan::Opcode::Map, end)
        .load_local(0)
        .each_item(exact_plan::Opcode::Map, end_inner)
        .number(1.0)
        .place(end_inner)
        .place(end);
    if extra {
        let last = body.label();
        body.simple(exact_plan::Opcode::Pop);
        ones(&mut body, 1)
            .each_item(exact_plan::Opcode::Map, last)
            .number(1.0)
            .place(last);
    }
    body.drop_local();
    run(body)
}

/// LLP 1017.003 D3: 256 outer runs of 255 inner are 65,536 runs, which
/// fit; one more run anywhere in the evaluation is `IterationLimit`.
#[test]
fn callback_runs_are_counted_across_nesting_and_capped() {
    let Value::List(outer) = nested(256, 255, false).unwrap().value else {
        panic!("a list")
    };
    assert_eq!(outer.len(), 256);
    assert!(matches!(&outer[0], Value::List(inner) if inner.len() == 255));
    assert!(matches!(
        nested(256, 255, true),
        Err(Trap::IterationLimit { .. })
    ));
    assert!(nested(255, 255, true).is_ok());
}

#[test]
fn map_and_filter_bind_the_item_and_index_and_filter_keeps_by_bool() {
    // map([1, 1, 1], (x, i) => x + i) == [1, 2, 3]
    let mut body = Asm::new();
    let end = body.label();
    ones(&mut body, 3)
        .each_item(exact_plan::Opcode::Map, end)
        .load_local(0)
        .load_local(1)
        .simple(exact_plan::Opcode::Add)
        .place(end);
    let expected: Vec<Value> = [1.0, 2.0, 3.0].map(Value::Number).into();
    assert_eq!(run(body).unwrap().value, Value::list(expected));
    // filter(xs, (x, i) => i > 0) drops the first; `true` keeps the list itself.
    let mut body = Asm::new();
    let end = body.label();
    ones(&mut body, 3)
        .each_item(exact_plan::Opcode::Filter, end)
        .load_local(1)
        .number(0.0)
        .simple(exact_plan::Opcode::Gt)
        .place(end);
    assert!(matches!(run(body).unwrap().value, Value::List(l) if l.len() == 2));
    let mut body = Asm::new();
    let end = body.label();
    ones(&mut body, 2).bind_local().load_local(0);
    body.each_item(exact_plan::Opcode::Filter, end)
        .bool(true)
        .place(end)
        .load_local(0)
        .op(exact_plan::Opcode::Eq, &[])
        .drop_local();
    assert_eq!(run(body).unwrap().value, Value::Bool(true));
    // A filter body that is not a bool; an empty list runs no body.
    let mut body = Asm::new();
    let end = body.label();
    ones(&mut body, 1)
        .each_item(exact_plan::Opcode::Filter, end)
        .number(1.0)
        .place(end);
    assert!(matches!(
        run(body),
        Err(Trap::TypeMismatch {
            op: exact_plan::Opcode::Filter,
            ..
        })
    ));
    let mut body = Asm::new();
    let end = body.label();
    body.list(0)
        .each_item(exact_plan::Opcode::Map, end)
        .simple(exact_plan::Opcode::Return)
        .place(end);
    assert_eq!(run(body).unwrap().value, Value::list(vec![]));
    // A `Return` inside a body is malformed.
    let mut body = Asm::new();
    let end = body.label();
    ones(&mut body, 1)
        .each_item(exact_plan::Opcode::Map, end)
        .number(1.0)
        .simple(exact_plan::Opcode::Return)
        .place(end);
    assert!(matches!(run(body), Err(Trap::Malformed { .. })));
}

/// A body runs on a stack of its own and drops only the locals it bound.
#[test]
fn a_callback_body_cannot_touch_what_its_caller_holds() {
    let mut body = Asm::new();
    let end = body.label();
    body.number(7.0);
    ones(&mut body, 1)
        .each_item(exact_plan::Opcode::Map, end)
        .simple(exact_plan::Opcode::Pop)
        .simple(exact_plan::Opcode::Pop)
        .number(1.0)
        .place(end);
    assert!(matches!(run(body), Err(Trap::StackUnderflow { .. })));
    let mut body = Asm::new();
    let end = body.label();
    ones(&mut body, 1)
        .each_item(exact_plan::Opcode::Map, end)
        .drop_local()
        .number(1.0)
        .place(end);
    assert!(matches!(run(body), Err(Trap::Malformed { .. })));
    // Its caller's operands are there again after it.
    let mut body = Asm::new();
    let end = body.label();
    body.number(7.0);
    ones(&mut body, 2)
        .each_item(exact_plan::Opcode::Map, end)
        .number(1.0)
        .place(end)
        .simple(exact_plan::Opcode::Pop);
    assert_eq!(run(body).unwrap().value, Value::Number(7.0));
}

/// Each item `join` prints is a step against the same bound.
#[test]
fn join_takes_a_step_per_item() {
    let joined = |n: u32| {
        let mut body = Asm::new();
        let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        let sep = b.str("");
        ones(&mut body, n).str(sep).call(Stdlib::Join);
        let code = b.code(body);
        let plan = b.finish().unwrap();
        let strings = intern(&plan);
        let env = Env {
            plan: &plan,
            strings: &strings,
            router: None,
            lists: None,
            format: None,
            geometry: None,
            slots: &[],
            derives: &[],
            resources: (&[]).into(),
            params: &[],
            frames: &[],
            now_ms: 0.0,
            pending_resources: (&[]).into(),
            current_resources: (&[]).into(),
            failed_resources: (&[]).into(),
            failed_why: (&[]).into(),
            pending_mutations: &[],
            failed_mutations: &[],
            store_dependent_derives: &[],
            store_dependent_resources: (&[]).into(),
        };
        eval(plan.code(code), &env, &[]).map(|o| o.value)
    };
    assert!(
        matches!(joined(MAX_LIST_STEPS), Ok(s) if s.as_str().map(str::len) == Some(MAX_LIST_STEPS as usize))
    );
    assert!(matches!(
        joined(MAX_LIST_STEPS + 1),
        Err(Trap::IterationLimit { .. })
    ));
}

#[test]
fn join_prints_items_as_to_string_does() {
    let join = |items: Vec<Value>, sep: &str| {
        stdlib::join(&[Value::list(items), Value::str(sep)], MAX_STRING)
    };
    let n = Value::Number;
    assert_eq!(
        join(vec![n(1.5), n(-0.0), n(1e21), n(0.1 + 0.2)], ","),
        Ok(Value::str("1.5,0,1e+21,0.30000000000000004"))
    );
    assert_eq!(
        join(vec![Value::str("a"), Value::Bool(false)], " · "),
        Ok(Value::str("a · false"))
    );
    assert_eq!(join(vec![], ","), Ok(Value::str("")));
    assert_eq!(
        join(vec![Value::record(vec![])], ","),
        Err(stdlib::JoinError::Type)
    );
    assert_eq!(
        stdlib::join(
            &[
                Value::list(vec![Value::str("ab"), Value::str("cd")]),
                Value::str("-")
            ],
            4
        ),
        Err(stdlib::JoinError::TooLong)
    );
}
