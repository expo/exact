use super::*;
use exact_plan::{asm::Asm, builder::PlanBuilder, RegionKind, TypeKind};

fn read(plan: &Plan, row: u32, frames: &[Frame]) -> Result<Value, Trap> {
    let mut code = Asm::default();
    code.load_derive(exact_plan::DerivesId(row));
    let strings = intern(plan);
    let env = Env {
        plan,
        strings: &strings,
        router: None,
        lists: None,
        format: None,
        geometry: None,
        slots: &[],
        derives: &[],
        resources: (&[]).into(),
        params: &[],
        frames,
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
    let code = code.finish();
    let result = eval(&code, &env, &[]).map(|out| out.value);
    let cached = Strings::with_constants(plan, std::sync::Arc::new(Constants::new(plan)));
    let cached_env = Env {
        strings: &cached,
        ..env
    };
    assert_eq!(result, eval(&code, &cached_env, &[]).map(|out| out.value));
    result
}

#[test]
fn an_at_read_body_cuts_frames_and_has_fresh_locals() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let ty = b.primitive(TypeKind::Number);
    let unit = b.constant(&Value::Unit);
    let (region, arms) = b.region(RegionKind::Each, None, None, 0, unit, unit, 1);
    let mut body = Asm::default();
    body.load_item(0);
    let code = b.code(body);
    let d = b.derive("item", ty, code);
    b.set_derive_unsettled(d, Some(arms[0]));
    let plan = b.finish().unwrap();
    let frames = [
        Frame {
            region: Some(region.0),
            item: Some(Value::Number(17.0)),
            ..Frame::default()
        },
        Frame {
            item: Some(Value::Number(99.0)),
            ..Frame::default()
        },
    ];
    assert_eq!(read(&plan, d.0, &frames).unwrap(), Value::Number(17.0));
    let mut call = Asm::default();
    call.load_derive(d);
    let call = call.finish();
    let (body, scope) = plan.read_code_in(&call, &frames, |f| f.region).unwrap();
    assert_eq!(body, plan.code(code));
    assert_eq!(scope, &frames[..1]);
    assert!(plan
        .read_code_in(&call, &[] as &[Frame], |f| f.region)
        .is_none());
    assert!(matches!(read(&plan, d.0, &[]), Err(Trap::BadScope { .. })));
}

#[test]
fn row_call_work_and_depth_are_bounded_even_without_list_steps() {
    for (length, twice, kind, bound) in [
        (
            30,
            true,
            "CallLimit",
            ": past the 65,536 derive calls one evaluation may take",
        ),
        (65, false, "CallDepth", ": past 64 derive call levels"),
    ] {
        let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        let ty = b.primitive(TypeKind::Number);
        let code = b.constant(&Value::Number(1.0));
        let mut d = b.derive("base", ty, code);
        b.set_derive_unsettled(d, None);
        for i in 1..length {
            let mut body = Asm::default();
            body.load_derive(d);
            if twice {
                body.load_derive(d).simple(Opcode::Add);
            }
            let code = b.code(body);
            d = b.derive(&format!("d{i}"), ty, code);
            b.set_derive_unsettled(d, None);
        }
        let plan = b.finish().unwrap();
        let trap = read(&plan, d.0, &[]).unwrap_err();
        assert_eq!(
            format!("{trap:?}{}", trap.bound()),
            format!("{kind} {{ pc: 0 }}{bound}")
        );
    }
}

fn accounted_row_sum(len: usize, caller_step: bool) -> (Result<Outcome, Trap>, Accounting) {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let ty = b.primitive(TypeKind::Number);
    let unit = b.constant(&Value::Unit);
    let (region, arms) = b.region(RegionKind::Each, None, None, 0, unit, unit, 1);
    let mut body = Asm::default();
    body.load_item(0).call(exact_plan::Stdlib::Sum);
    let code = b.code(body);
    let d = b.derive("sum", ty, code);
    b.set_derive_unsettled(d, Some(arms[0]));
    let plan = b.finish().unwrap();
    let strings = intern(&plan);
    let values = Value::list(vec![Value::Number(1.0); len]);
    let frames = [Frame {
        region: Some(region.0),
        item: Some(values),
        ..Frame::default()
    }];
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
        frames: &frames,
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
    let mut caller = Asm::default();
    caller.load_derive(d);
    if caller_step {
        let end = caller.label();
        caller
            .simple(Opcode::Pop)
            .number(1.0)
            .list(1)
            .each_item(Opcode::Map, end)
            .load_local(0)
            .place(end);
    }
    let caller = caller.finish();
    eval_accounted(&caller, &env, &[])
}

#[test]
fn row_call_roster_steps_transfer_on_return_and_trap() {
    let (outcome, accounting) = accounted_row_sum(65_535, true);
    assert!(outcome.is_ok());
    assert_eq!(
        accounting,
        Accounting {
            calls: 1,
            list_steps: 65_536,
            call_depth: 0,
        }
    );

    let (outcome, accounting) = accounted_row_sum(65_536, true);
    assert!(matches!(outcome, Err(Trap::IterationLimit { .. })));
    assert_eq!(
        accounting,
        Accounting {
            calls: 1,
            list_steps: 65_537,
            call_depth: 0,
        }
    );

    let (outcome, accounting) = accounted_row_sum(65_537, false);
    assert!(matches!(outcome, Err(Trap::IterationLimit { .. })));
    assert_eq!(
        accounting,
        Accounting {
            calls: 1,
            list_steps: 65_537,
            call_depth: 0,
        }
    );
}
