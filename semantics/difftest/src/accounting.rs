//! Fixed boundary cases for executable-row resource accounting.

use crate::{leanrun, observe as observed, work_dir};
use exact_plan::{
    asm::Asm, builder::PlanBuilder, DerivesId, Opcode, Plan, RegionKind, Stdlib, TypeKind, Value,
};
use exact_runner::{
    failure::{Failure, FailureCode},
    held::Held,
    vm::{self, Accounting, Env, Frame, Outcome, Owned, OwnedRequest, Strings, Trap},
};

const CASES: usize = 69;

#[derive(Debug, PartialEq, Eq)]
struct Observation {
    outcome: &'static str,
    value: Option<String>,
    calls: u32,
    list_steps: u32,
    call_depth: u16,
}

impl Observation {
    fn line(&self, name: &str) -> String {
        let value = self
            .value
            .as_ref()
            .map_or(String::new(), |value| format!(" {value}"));
        format!(
            "{name} {}{value} {} {} {}",
            self.outcome, self.calls, self.list_steps, self.call_depth
        )
    }
}

fn observe(plan: &Plan, code: &[u8], frames: &[Frame]) -> (Result<Outcome, Trap>, Accounting) {
    let strings = Strings::new(plan);
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
    vm::eval_accounted(code, &env, &[])
}

fn classify(result: &Result<Outcome, Trap>) -> &'static str {
    match result {
        Ok(_) => "ok",
        Err(Trap::StackUnderflow { .. }) => "StackUnderflow",
        Err(Trap::TypeMismatch { .. }) => "TypeMismatch",
        Err(Trap::RangeCount { .. }) => "RangeCount",
        Err(Trap::PathParameter { .. }) => "PathParameter",
        Err(Trap::UnwrapNone { .. }) => "UnwrapNone",
        Err(Trap::BadField { .. }) => "BadField",
        Err(Trap::BadScope { .. }) => "BadScope",
        Err(Trap::BadParam { .. }) => "BadParam",
        Err(Trap::Arity { .. }) => "Arity",
        Err(Trap::WriteNotDeclared { .. }) => "WriteNotDeclared",
        Err(Trap::BadJump { .. }) => "BadJump",
        Err(Trap::Malformed { .. }) => "Malformed",
        Err(Trap::NoResult) => "NoResult",
        Err(Trap::StringTooLong { .. }) => "StringTooLong",
        Err(Trap::Pending { .. }) => "Pending",
        Err(Trap::ValueTooLarge { .. }) => "ValueTooLarge",
        Err(Trap::ValueTooDeep { .. }) => "ValueTooDeep",
        Err(Trap::IterationLimit { .. }) => "IterationLimit",
        Err(Trap::CallLimit { .. }) => "CallLimit",
        Err(Trap::CallDepth { .. }) => "CallDepth",
    }
}

fn observation(result: Result<Outcome, Trap>, accounting: Accounting) -> Observation {
    Observation {
        outcome: classify(&result),
        value: None,
        calls: accounting.calls,
        list_steps: accounting.list_steps,
        call_depth: accounting.call_depth,
    }
}

fn observation_with_value(result: Result<Outcome, Trap>, accounting: Accounting) -> Observation {
    let value = result
        .as_ref()
        .ok()
        .map(|outcome| observed::value(&outcome.value));
    Observation {
        outcome: classify(&result),
        value,
        calls: accounting.calls,
        list_steps: accounting.list_steps,
        call_depth: accounting.call_depth,
    }
}

fn doubled_list(body: &mut Asm, times: usize) {
    for _ in 0..times {
        body.bind_local()
            .load_local(0)
            .load_local(0)
            .list(2)
            .drop_local();
    }
}

fn doubled_string(body: &mut Asm, times: usize) {
    for _ in 0..times {
        body.bind_local()
            .load_local(0)
            .load_local(0)
            .simple(Opcode::Concat)
            .drop_local();
    }
}

fn doubled_value(times: usize) -> Value {
    let mut value = Value::Number(1.0);
    for _ in 0..times {
        value = Value::list(vec![value.clone(), value]);
    }
    value
}

fn nested_value(depth: usize) -> Value {
    let mut value = Value::Number(1.0);
    for _ in 0..depth {
        value = Value::some(value);
    }
    value
}

fn root_row(mut builder: PlanBuilder, body: Asm) -> (Plan, DerivesId) {
    let ty = builder.primitive(TypeKind::Unit);
    let code = builder.code(body);
    let row = builder.derive("row", ty, code);
    builder.set_derive_unsettled(row, None);
    (builder.finish().expect("accounting plan"), row)
}

fn owned_row(mut builder: PlanBuilder, body: Asm) -> (Plan, DerivesId, u32) {
    let ty = builder.primitive(TypeKind::Unit);
    let unit = builder.constant(&Value::Unit);
    let (region, arms) = builder.region(RegionKind::Each, None, None, 0, unit, unit, 1);
    let code = builder.code(body);
    let row = builder.derive("row", ty, code);
    builder.set_derive_unsettled(row, Some(arms[0]));
    (
        builder.finish().expect("owned accounting plan"),
        row,
        region.0,
    )
}

fn call_code(plan: &Plan, code: Asm, frames: &[Frame]) -> Observation {
    let (result, accounting) = observe(plan, &code.finish(), frames);
    observation(result, accounting)
}

fn call(plan: &Plan, row: DerivesId, frames: &[Frame]) -> Observation {
    let mut caller = Asm::new();
    caller.load_derive(row);
    call_code(plan, caller, frames)
}

fn call_owned(plan: &Plan, row: DerivesId, region: u32, item: Value) -> Observation {
    call(
        plan,
        row,
        &[Frame {
            region: Some(region),
            item: Some(item),
            ..Frame::default()
        }],
    )
}

fn supplied_call(builder: PlanBuilder, f: Stdlib, args: Vec<Value>) -> Observation {
    let mut body = Asm::new();
    for index in 0..args.len() {
        body.load_item(0).field(index as u16);
    }
    body.call(f);
    let (plan, row, region) = owned_row(builder, body);
    call_owned(&plan, row, region, Value::record(args))
}

fn list_steps(len: usize) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let ty = builder.primitive(TypeKind::Number);
    let unit = builder.constant(&Value::Unit);
    let (region, arms) = builder.region(RegionKind::Each, None, None, 0, unit, unit, 1);
    let mut body = Asm::new();
    body.load_item(0).call(Stdlib::Sum);
    let code = builder.code(body);
    let row = builder.derive("sum", ty, code);
    builder.set_derive_unsettled(row, Some(arms[0]));
    let plan = builder.finish().expect("list-step plan");
    let values = Value::list(vec![Value::Number(1.0); len]);
    let frames = [Frame {
        region: Some(region.0),
        item: Some(values),
        ..Frame::default()
    }];
    let mut caller = Asm::new();
    caller
        .load_derive(row)
        .simple(Opcode::Pop)
        .number(1.0)
        .list(1);
    let end = caller.label();
    caller.each_item(Opcode::Map, end).load_local(0).place(end);
    call_code(&plan, caller, &frames)
}

fn string_bytes(over: bool) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let x = builder.str("x");
    let mut body = Asm::new();
    body.str(x);
    doubled_string(&mut body, 26);
    if over {
        body.str(x).simple(Opcode::Concat);
    }
    let (plan, row) = root_row(builder, body);
    call(&plan, row, &[])
}

fn value_nodes(over: bool) -> Observation {
    let builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut body = Asm::new();
    body.number(1.0);
    doubled_list(&mut body, 23);
    body.bind_local().load_local(0);
    if over {
        body.number(1.0).list(2);
    } else {
        body.list(1);
    }
    body.drop_local();
    let (plan, row) = root_row(builder, body);
    call(&plan, row, &[])
}

fn value_bytes(over: bool) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let x = builder.str("x");
    let mut body = Asm::new();
    body.str(x);
    doubled_string(&mut body, 20);
    body.bind_local();
    for _ in 0..64 {
        body.load_local(0);
    }
    if over {
        body.str(x);
    }
    body.list(if over { 65 } else { 64 }).drop_local();
    let (plan, row) = root_row(builder, body);
    call(&plan, row, &[])
}

fn value_depth(depth: usize) -> Observation {
    let builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut body = Asm::new();
    body.number(1.0);
    for _ in 0..depth {
        body.simple(Opcode::Some);
    }
    let (plan, row) = root_row(builder, body);
    call(&plan, row, &[])
}

fn call_chain(length: usize) -> Observation {
    call_chain_after(length, 0)
}

fn call_chain_after(length: usize, spent: usize) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let ty = builder.primitive(TypeKind::Unit);
    let code = builder.constant(&Value::Unit);
    let leaf = builder.derive("row0", ty, code);
    builder.set_derive_unsettled(leaf, None);
    let mut row = leaf;
    for index in 1..length {
        let mut body = Asm::new();
        body.load_derive(row);
        let code = builder.code(body);
        row = builder.derive(&format!("row{index}"), ty, code);
        builder.set_derive_unsettled(row, None);
    }
    let plan = builder.finish().expect("call-chain plan");
    let mut caller = Asm::new();
    for _ in 0..spent {
        caller.load_derive(leaf).simple(Opcode::Pop);
    }
    caller.load_derive(row);
    call_code(&plan, caller, &[])
}

fn call_count(count: usize) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let ty = builder.primitive(TypeKind::Unit);
    let code = builder.constant(&Value::Unit);
    let row = builder.derive("row", ty, code);
    builder.set_derive_unsettled(row, None);
    let plan = builder.finish().expect("call-count plan");
    let mut caller = Asm::new();
    for _ in 0..count {
        caller.load_derive(row).simple(Opcode::Pop);
    }
    call_code(&plan, caller, &[])
}

fn join_separator(spent: usize) -> Observation {
    let builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut body = Asm::new();
    body.load_item(0)
        .field(0)
        .call(Stdlib::Sum)
        .simple(Opcode::Pop)
        .load_item(0)
        .field(1)
        .number(0.0)
        .call(Stdlib::Join);
    let (plan, row, region) = owned_row(builder, body);
    let item = Value::record(vec![
        Value::list(vec![Value::Number(1.0); spent]),
        Value::list(vec![Value::Number(1.0)]),
    ]);
    call_owned(&plan, row, region, item)
}

fn join_late_type(over: bool) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let x = builder.str("x");
    let mut body = Asm::new();
    body.str(x);
    doubled_string(&mut body, 25);
    body.bind_local()
        .load_item(0)
        .load_local(0)
        .call(Stdlib::Join);
    let (plan, row, region) = owned_row(builder, body);
    let third = if over {
        Value::str("x")
    } else {
        Value::str("")
    };
    let item = Value::list(vec![Value::str(""), Value::str(""), third, Value::Unit]);
    call_owned(&plan, row, region, item)
}

fn filter_type(spent: usize) -> Observation {
    let builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut body = Asm::new();
    body.load_item(0)
        .field(0)
        .call(Stdlib::Sum)
        .simple(Opcode::Pop)
        .load_item(0)
        .field(1);
    let end = body.label();
    body.each_item(Opcode::Filter, end).number(1.0).place(end);
    let (plan, row, region) = owned_row(builder, body);
    let item = Value::record(vec![
        Value::list(vec![Value::Number(1.0); spent]),
        Value::list(vec![Value::Number(1.0), Value::Number(2.0)]),
    ]);
    call_owned(&plan, row, region, item)
}

fn spend_from_item(body: &mut Asm) {
    body.load_item(0)
        .field(0)
        .call(Stdlib::Sum)
        .simple(Opcode::Pop);
}

fn map_extent_before_next() -> Observation {
    let builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut body = Asm::new();
    spend_from_item(&mut body);
    body.load_item(0).field(1);
    let end = body.label();
    body.each_item(Opcode::Map, end).load_local(0).place(end);
    let (plan, row, region) = owned_row(builder, body);
    let large = Value::list(vec![doubled_value(23), Value::Number(1.0)]);
    let item = Value::record(vec![
        Value::list(vec![Value::Number(1.0); 65_535]),
        Value::list(vec![large, Value::Unit]),
    ]);
    call_owned(&plan, row, region, item)
}

fn map_next_step() -> Observation {
    let builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut body = Asm::new();
    spend_from_item(&mut body);
    body.load_item(0).field(1);
    let end = body.label();
    body.each_item(Opcode::Map, end).load_local(0).place(end);
    let (plan, row, region) = owned_row(builder, body);
    let item = Value::record(vec![
        Value::list(vec![Value::Number(1.0); 65_535]),
        Value::list(vec![Value::Number(1.0), Value::Number(2.0)]),
    ]);
    call_owned(&plan, row, region, item)
}

fn filter_keep_drop() -> Observation {
    let builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut body = Asm::new();
    body.load_item(0);
    let end = body.label();
    body.each_item(Opcode::Filter, end)
        .load_local(0)
        .field(1)
        .place(end);
    let (plan, row, region) = owned_row(builder, body);
    let item = Value::list(vec![
        Value::record(vec![Value::Number(1.0), Value::Bool(true)]),
        Value::record(vec![nested_value(65), Value::Bool(false)]),
    ]);
    call_owned(&plan, row, region, item)
}

fn record_extent(over: bool) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let number = builder.primitive(TypeKind::Number);
    let fields = if over {
        vec![("large", number), ("extra", number)]
    } else {
        vec![("large", number)]
    };
    let record = builder.record("Extent", &fields);
    let mut body = Asm::new();
    body.number(1.0);
    doubled_list(&mut body, 23);
    if over {
        body.number(1.0);
    }
    body.record(record);
    let (plan, row) = root_row(builder, body);
    call(&plan, row, &[])
}

#[derive(Clone, Copy)]
enum RosterBoundary {
    Range,
    Split,
    Concat,
    Flat,
    Slice,
    Includes,
    IndexOf,
    SortBy,
}

fn roster_boundary(kind: RosterBoundary, past: bool) -> Observation {
    let cost = match kind {
        RosterBoundary::Flat => 4,
        _ => 2,
    };
    let spent = 65_536 - cost + usize::from(past);
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let a = builder.str("a,b");
    let comma = builder.str(",");
    let mut body = Asm::new();
    spend_from_item(&mut body);
    match kind {
        RosterBoundary::Range => {
            body.number(2.0).call(Stdlib::Range);
        }
        RosterBoundary::Split => {
            body.str(a).str(comma).call(Stdlib::Split);
        }
        RosterBoundary::Concat => {
            body.number(1.0)
                .list(1)
                .number(2.0)
                .list(1)
                .call(Stdlib::Concat);
        }
        RosterBoundary::Flat => {
            body.number(1.0)
                .list(1)
                .number(2.0)
                .list(1)
                .list(2)
                .call(Stdlib::Flat);
        }
        RosterBoundary::Slice => {
            body.number(1.0)
                .number(2.0)
                .list(2)
                .number(0.0)
                .number(2.0)
                .call(Stdlib::Slice);
        }
        RosterBoundary::Includes => {
            body.number(1.0)
                .number(2.0)
                .list(2)
                .number(2.0)
                .call(Stdlib::Includes);
        }
        RosterBoundary::IndexOf => {
            body.number(1.0)
                .number(2.0)
                .list(2)
                .number(2.0)
                .call(Stdlib::IndexOf);
        }
        RosterBoundary::SortBy => {
            body.number(2.0).number(1.0).list(2).bind_local();
            body.load_local(0);
            let end = body.label();
            body.each_item(Opcode::Map, end)
                .load_local(1)
                .place(end)
                .bind_local()
                .load_local(0)
                .load_local(1)
                .bool(false)
                .call(Stdlib::SortBy)
                .drop_local()
                .drop_local();
        }
    }
    let (plan, row, region) = owned_row(builder, body);
    let item = Value::record(vec![Value::list(vec![Value::Number(1.0); spent])]);
    call_owned(&plan, row, region, item)
}

fn sort_order(descending: bool, equal_keys: bool) -> Observation {
    let builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut body = Asm::new();
    body.load_item(0)
        .field(0)
        .bind_local()
        .load_item(0)
        .field(1)
        .bind_local()
        .load_local(0)
        .load_local(1)
        .bool(descending)
        .call(Stdlib::SortBy)
        .drop_local()
        .drop_local();
    let (plan, row, region) = owned_row(builder, body);
    let too_deep = nested_value(65);
    let keys = if equal_keys {
        vec![Value::Number(1.0), Value::Number(1.0)]
    } else if descending {
        vec![Value::Number(1.0), Value::Number(2.0)]
    } else {
        vec![Value::Number(2.0), Value::Number(1.0)]
    };
    let too_large = Value::str(&"x".repeat(vm::MAX_STRING + 1));
    let item = Value::record(vec![
        Value::list(vec![too_large, too_deep]),
        Value::list(keys),
    ]);
    call_owned(&plan, row, region, item)
}

fn localized_fill(past: bool) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = builder.primitive(TypeKind::String);
    let initial = builder.constant(&Value::str("en"));
    let locale = builder.slot("#locale", string, initial);
    builder.set_locale(locale);
    builder.locale("en", false, "", &[("key", "a{value}")]);
    let mut fill = "😀".repeat((1 << 24) - usize::from(!past));
    if !past {
        fill.push_str("xxx");
    }
    supplied_call(
        builder,
        Stdlib::T,
        vec![
            Value::str("en"),
            Value::str("key"),
            Value::list(vec![Value::str("value"), Value::str(&fill)]),
        ],
    )
}

fn encoded_input(past: bool) -> String {
    let mut input = String::from("a");
    input.push_str(&"😀".repeat(5_592_405));
    input.push(' ');
    if past {
        input.push('a');
    }
    input
}

fn encoded(f: Stdlib, past: bool) -> Observation {
    supplied_call(
        PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1),
        f,
        vec![Value::str(&encoded_input(past))],
    )
}

fn replace_all_amplified() -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let source = builder.str(&"x".repeat(1 << 16));
    let find = builder.str("x");
    let replacement = builder.str(&"x".repeat(1 << 16));
    let mut body = Asm::new();
    body.str(source)
        .str(find)
        .str(replacement)
        .call(Stdlib::ReplaceAll);
    let (plan, row) = root_row(builder, body);
    call(&plan, row, &[])
}

fn failure_read(resource: bool, over: bool, pending: bool) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let ty = builder.primitive(TypeKind::Unit);
    let mut body = Asm::new();
    if resource {
        let id = builder.resource("resource", "source", &[], ty, None);
        body.failure_resource(id);
    } else {
        let option = builder.option(ty);
        let init = builder.constant(&Value::Option(None));
        let slot = builder.slot("reply", option, init);
        let id = builder.mutation("mutation", slot, ty);
        body.failure_mutation(id);
    }
    let (plan, row) = root_row(builder, body);
    let reason = "x".repeat(1 << 25);
    let failure = Failure {
        code: FailureCode::Error,
        message: if over { "x".into() } else { String::new() },
        reason,
    };
    let resources = [(!pending).then(|| Held::new(Value::Unit))];
    let failed_resources = [Some(Vec::new())];
    let failed_why = [Some(failure.clone())];
    let failed_mutations = [Some(failure)];
    let strings = Strings::new(&plan);
    let env = Env {
        plan: &plan,
        strings: &strings,
        router: None,
        lists: None,
        format: None,
        geometry: None,
        slots: &[],
        derives: &[],
        resources: (&resources[..]).into(),
        params: &[],
        frames: &[],
        now_ms: 0.0,
        pending_resources: (&[false][..]).into(),
        current_resources: (&[true][..]).into(),
        failed_resources: (&failed_resources[..]).into(),
        failed_why: (&failed_why[..]).into(),
        pending_mutations: &[false],
        failed_mutations: &failed_mutations,
        store_dependent_derives: &[],
        store_dependent_resources: (&[false][..]).into(),
    };
    let mut caller = Asm::new();
    caller.load_derive(row);
    let (result, accounting) = vm::eval_accounted(&caller.finish(), &env, &[]);
    observation(result, accounting)
}

fn extent_order() -> Observation {
    let builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut body = Asm::new();
    body.load_item(0).simple(Opcode::Some);
    let (plan, row, region) = owned_row(builder, body);
    let text = "x".repeat(vm::MAX_STRING + 1);
    let mut item = Value::str(&text);
    for _ in 0..65 {
        item = Value::some(item);
    }
    call_owned(&plan, row, region, item)
}

fn owned_resource_after_cut(current: bool) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let number = builder.primitive(TypeKind::Number);
    let bool_ty = builder.primitive(TypeKind::Bool);
    let unit = builder.constant(&Value::Unit);
    let (outer, outer_arms) = builder.region(RegionKind::Each, None, None, 0, unit, unit, 1);
    let (inner, inner_arms) = builder.region(
        RegionKind::Each,
        None,
        Some(outer_arms[0]),
        1,
        unit,
        unit,
        1,
    );
    let resource = builder.resource("owned", "source", &[], number, None);
    builder.set_resource_owner(resource, inner_arms[0]);
    let mut body = Asm::new();
    if current {
        body.current_resource(resource);
    } else {
        body.load_resource(resource);
    }
    let body = builder.code(body);
    let row = builder.derive("row", if current { bool_ty } else { number }, body);
    builder.set_derive_unsettled(row, Some(outer_arms[0]));
    let plan = builder.finish().expect("owned-resource accounting plan");

    let outer_store = Owned::shared();
    let inner_store = Owned::shared();
    inner_store.borrow_mut().requests.push(OwnedRequest {
        resource: resource.0,
        row: None,
        value: Held::new(Value::Number(7.0)),
        pending: false,
        current: true,
        failed: None,
    });
    let frames = [
        Frame {
            region: Some(outer.0),
            row: Some(outer_store),
            ..Frame::default()
        },
        Frame {
            region: Some(inner.0),
            row: Some(inner_store),
            ..Frame::default()
        },
    ];
    let resources = [Some(Held::new(Value::Number(0.0)))];
    let strings = Strings::new(&plan);
    let env = Env {
        plan: &plan,
        strings: &strings,
        router: None,
        lists: None,
        format: None,
        geometry: None,
        slots: &[],
        derives: &[],
        resources: (&resources[..]).into(),
        params: &[],
        frames: &frames,
        now_ms: 0.0,
        pending_resources: (&[false][..]).into(),
        current_resources: (&[true][..]).into(),
        failed_resources: (&[None::<Vec<Value>>][..]).into(),
        failed_why: (&[None][..]).into(),
        pending_mutations: &[],
        failed_mutations: &[],
        store_dependent_derives: &[],
        store_dependent_resources: (&[false][..]).into(),
    };
    let mut caller = Asm::new();
    caller.load_derive(row);
    let (result, accounting) = vm::eval_accounted(&caller.finish(), &env, &[]);
    observation(result, accounting)
}

#[derive(Clone, Copy)]
enum OwnedRead {
    Value,
    Pending,
    Failed,
    Failure,
    Current,
}

fn owned_resource_wrong_arm(read: OwnedRead) -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let number = builder.primitive(TypeKind::Number);
    let unit = builder.constant(&Value::Unit);
    let (region, arms) = builder.region(RegionKind::When, None, None, 0, unit, unit, 2);
    let resource = builder.resource("owned", "source", &[], number, None);
    builder.set_resource_owner(resource, arms[1]);
    let mut body = Asm::new();
    match read {
        OwnedRead::Value => body.load_resource(resource),
        OwnedRead::Pending => body.pending_resource(resource),
        OwnedRead::Failed => body.failed_resource(resource),
        OwnedRead::Failure => body.failure_resource(resource),
        OwnedRead::Current => body.current_resource(resource),
    };
    let body = builder.code(body);
    let row = builder.derive("row", number, body);
    builder.set_derive_unsettled(row, Some(arms[0]));
    let plan = builder.finish().expect("wrong-arm accounting plan");

    let frames = [Frame {
        region: Some(region.0),
        row: Some(Owned::shared()),
        ..Frame::default()
    }];
    let resources = [Some(Held::new(Value::Number(7.0)))];
    let strings = Strings::new(&plan);
    let env = Env {
        plan: &plan,
        strings: &strings,
        router: None,
        lists: None,
        format: None,
        geometry: None,
        slots: &[],
        derives: &[],
        resources: (&resources[..]).into(),
        params: &[],
        frames: &frames,
        now_ms: 0.0,
        pending_resources: (&[false][..]).into(),
        current_resources: (&[true][..]).into(),
        failed_resources: (&[None::<Vec<Value>>][..]).into(),
        failed_why: (&[None][..]).into(),
        pending_mutations: &[],
        failed_mutations: &[],
        store_dependent_derives: &[],
        store_dependent_resources: (&[false][..]).into(),
    };
    let mut caller = Asm::new();
    caller.load_derive(row);
    let (result, accounting) = vm::eval_accounted(&caller.finish(), &env, &[]);
    observation(result, accounting)
}

fn row_call_restores_reader_frame() -> Observation {
    let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let unit_ty = builder.primitive(TypeKind::Unit);
    let unit = builder.constant(&Value::Unit);
    let (outer, outer_arms) = builder.region(RegionKind::Each, None, None, 0, unit, unit, 1);
    let (inner, _) = builder.region(
        RegionKind::Each,
        None,
        Some(outer_arms[0]),
        1,
        unit,
        unit,
        1,
    );
    let body = builder.constant(&Value::Unit);
    let row = builder.derive("row", unit_ty, body);
    builder.set_derive_unsettled(row, Some(outer_arms[0]));
    let plan = builder.finish().expect("frame-restoration accounting plan");
    let frames = [
        Frame {
            region: Some(outer.0),
            item: Some(Value::Number(1.0)),
            ..Frame::default()
        },
        Frame {
            region: Some(inner.0),
            item: Some(Value::Number(2.0)),
            ..Frame::default()
        },
    ];
    let mut caller = Asm::new();
    caller.load_derive(row).simple(Opcode::Pop).load_item(0);
    let (result, accounting) = observe(&plan, &caller.finish(), &frames);
    observation_with_value(result, accounting)
}

fn runner_lines() -> Vec<String> {
    [
        ("list-at", list_steps(65_535)),
        ("list-past", list_steps(65_536)),
        ("string-at", string_bytes(false)),
        ("string-past", string_bytes(true)),
        ("nodes-at", value_nodes(false)),
        ("nodes-past", value_nodes(true)),
        ("bytes-at", value_bytes(false)),
        ("bytes-past", value_bytes(true)),
        ("depth-at", value_depth(64)),
        ("depth-past", value_depth(65)),
        ("chain-48", call_chain(48)),
        ("chain-49", call_chain(49)),
        ("calls-at", call_count(65_536)),
        ("calls-past", call_count(65_537)),
        ("call-depth-at", call_chain(64)),
        ("call-depth-past", call_chain(65)),
        ("calls-depth-past", call_chain_after(65, 65_472)),
        ("join-separator-unused", join_separator(0)),
        ("join-separator-at", join_separator(65_535)),
        ("join-separator-past", join_separator(65_536)),
        ("join-string-at", join_late_type(false)),
        ("join-string-past", join_late_type(true)),
        ("filter-type-unused", filter_type(0)),
        ("filter-type-at", filter_type(65_535)),
        ("map-extent-before-next", map_extent_before_next()),
        ("map-next-step-past", map_next_step()),
        ("filter-keep-drop", filter_keep_drop()),
        ("range-at", roster_boundary(RosterBoundary::Range, false)),
        ("range-past", roster_boundary(RosterBoundary::Range, true)),
        ("record-at", record_extent(false)),
        ("record-past", record_extent(true)),
        ("split-at", roster_boundary(RosterBoundary::Split, false)),
        ("split-past", roster_boundary(RosterBoundary::Split, true)),
        ("concat-at", roster_boundary(RosterBoundary::Concat, false)),
        ("concat-past", roster_boundary(RosterBoundary::Concat, true)),
        ("flat-at", roster_boundary(RosterBoundary::Flat, false)),
        ("flat-past", roster_boundary(RosterBoundary::Flat, true)),
        ("slice-at", roster_boundary(RosterBoundary::Slice, false)),
        ("slice-past", roster_boundary(RosterBoundary::Slice, true)),
        (
            "includes-at",
            roster_boundary(RosterBoundary::Includes, false),
        ),
        (
            "includes-past",
            roster_boundary(RosterBoundary::Includes, true),
        ),
        (
            "indexOf-at",
            roster_boundary(RosterBoundary::IndexOf, false),
        ),
        (
            "indexOf-past",
            roster_boundary(RosterBoundary::IndexOf, true),
        ),
        ("sortBy-at", roster_boundary(RosterBoundary::SortBy, false)),
        ("sortBy-past", roster_boundary(RosterBoundary::SortBy, true)),
        ("sortBy-order-ascending", sort_order(false, false)),
        ("sortBy-order-descending", sort_order(true, false)),
        ("sortBy-order-equal", sort_order(false, true)),
        ("t-fill-at", localized_fill(false)),
        ("t-fill-past", localized_fill(true)),
        (
            "encodeURIComponent-at",
            encoded(Stdlib::EncodeURIComponent, false),
        ),
        (
            "encodeURIComponent-past",
            encoded(Stdlib::EncodeURIComponent, true),
        ),
        (
            "encodeRouteSegment-at",
            encoded(Stdlib::EncodeRouteSegment, false),
        ),
        (
            "encodeRouteSegment-past",
            encoded(Stdlib::EncodeRouteSegment, true),
        ),
        ("replaceAll-amplified", replace_all_amplified()),
        ("failure-resource-at", failure_read(true, false, false)),
        ("failure-resource-past", failure_read(true, true, false)),
        ("failure-resource-pending", failure_read(true, true, true)),
        ("failure-mutation-at", failure_read(false, false, false)),
        ("failure-mutation-past", failure_read(false, true, false)),
        ("extent-depth-first", extent_order()),
        ("owned-value-after-cut", owned_resource_after_cut(false)),
        ("owned-current-after-cut", owned_resource_after_cut(true)),
        (
            "owned-value-wrong-arm",
            owned_resource_wrong_arm(OwnedRead::Value),
        ),
        (
            "owned-pending-wrong-arm",
            owned_resource_wrong_arm(OwnedRead::Pending),
        ),
        (
            "owned-failed-wrong-arm",
            owned_resource_wrong_arm(OwnedRead::Failed),
        ),
        (
            "owned-failure-wrong-arm",
            owned_resource_wrong_arm(OwnedRead::Failure),
        ),
        (
            "owned-current-wrong-arm",
            owned_resource_wrong_arm(OwnedRead::Current),
        ),
        (
            "row-restores-reader-frame",
            row_call_restores_reader_frame(),
        ),
    ]
    .into_iter()
    .map(|(name, observation)| observation.line(name))
    .collect()
}

fn lean_module() -> &'static str {
    r#"import Contract.VmRows
import Contract.Observe
open Contract Contract.Vm Contract.VmRows

set_option maxRecDepth 100000
set_option maxHeartbeats 0

def repeatCode (n : Nat) (part : Code) : Code := (List.replicate n part).flatten

def stringCode (over : Bool) : Code :=
  [.str "x"] ++ repeatCode 26 [.bindLocal, .loadLocal 0, .loadLocal 0, .concat, .dropLocal] ++
    (if over then [.str "x", .concat] else []) ++ [.ret]

def nodeCode (over : Bool) : Code :=
  [.num 0x3ff0000000000000] ++
    repeatCode 23 [.bindLocal, .loadLocal 0, .loadLocal 0, .list 2, .dropLocal] ++
    [.bindLocal, .loadLocal 0] ++
    (if over then [.num 0x3ff0000000000000, .list 2] else [.list 1]) ++
    [.dropLocal, .ret]

def byteCode (over : Bool) : Code :=
  [.str "x"] ++ repeatCode 20 [.bindLocal, .loadLocal 0, .loadLocal 0, .concat, .dropLocal] ++
    [.bindLocal] ++ List.replicate 64 (.loadLocal 0) ++
    (if over then [.str "x", .list 65] else [.list 64]) ++ [.dropLocal, .ret]

def depthCode (n : Nat) : Code :=
  [.num 0x3ff0000000000000] ++ List.replicate n .some ++ [.ret]

def rootReport (body : Code) (env : Vm.Env := {}) : Report :=
  runReport [some { code := body }] [.loadDerive 0, .ret] env

def ownedReport (body : Code) (item : Value) : Report :=
  runReport [some { code := body, owner := some 0 }] [.loadDerive 0, .ret]
    { frames := [{ item := some item }] } [some 0]

def suppliedReport (f : String) (args : List Value) (env : Vm.Env := {}) : Report :=
  let body := (List.range args.length).flatMap fun i => [.loadItem 0, .field i]
  runReport [some { code := body ++ [.call f args.length, .ret], owner := some 0 }]
    [.loadDerive 0, .ret]
    { env with frames := [{ item := some (.record "Args" args) }] } [some 0]

def listReport (n : Nat) : Report :=
  runReport [some { code := [.loadItem 0, .call "sum" 1, .ret], owner := some 0 }]
    [.loadDerive 0, .pop, .num 0x3ff0000000000000, .list 1,
      .map 1, .loadLocal 0, .ret]
    { frames := [{ item := some (.list (List.replicate n (.num 0x3ff0000000000000))) }] }
    [some 0]

def chainRows : Nat → List (Option Row)
  | 0 => []
  | n + 1 => chainRows n ++ [some { code :=
      if n == 0 then [.unit, .ret] else [.loadDerive (n - 1), .ret] }]

def chainReport (n : Nat) : Report :=
  runReport (chainRows n) [.loadDerive (n - 1), .ret] {}

def countReport (n : Nat) : Report :=
  runReport (chainRows 1) (repeatCode n [.loadDerive 0, .pop] ++ [.ret]) {}

def callsDepthReport : Report :=
  runReport (chainRows 65)
    (repeatCode 65472 [.loadDerive 0, .pop] ++ [.loadDerive 64, .ret]) {}

def joinSeparatorReport (spent : Nat) : Report :=
  ownedReport
    [.loadItem 0, .field 0, .call "sum" 1, .pop,
     .loadItem 0, .field 1, .num 0, .call "join" 2, .ret]
    (.record "Case" [
      .list (List.replicate spent (.num 0x3ff0000000000000)),
      .list [.num 0x3ff0000000000000]])

def joinCode : Code :=
  [.str "x"] ++ repeatCode 25 [.bindLocal, .loadLocal 0, .loadLocal 0, .concat, .dropLocal] ++
    [.bindLocal, .loadItem 0, .loadLocal 0, .call "join" 2, .ret]

def joinLateTypeReport (over : Bool) : Report :=
  ownedReport joinCode (.list [.str "", .str "", if over then .str "x" else .str "", .unit])

def filterTypeReport (spent : Nat) : Report :=
  ownedReport
    [.loadItem 0, .field 0, .call "sum" 1, .pop,
     .loadItem 0, .field 1, .filter 1, .num 0x3ff0000000000000, .ret]
    (.record "Case" [
      .list (List.replicate spent (.num 0x3ff0000000000000)),
      .list [.num 0x3ff0000000000000, .num 0x4000000000000000]])

def nodeValue : Nat → Value
  | 0 => .num 0x3ff0000000000000
  | n + 1 => let v := nodeValue n; .list [v, v]

def deepValue : Nat → Value
  | 0 => .num 0x3ff0000000000000
  | n + 1 => .some (deepValue n)

def mapExtentBeforeNextReport : Report :=
  ownedReport
    [.loadItem 0, .field 0, .call "sum" 1, .pop,
     .loadItem 0, .field 1, .map 1, .loadLocal 0, .ret]
    (.record "Case" [
      .list (List.replicate 65535 (.num 0x3ff0000000000000)),
      .list [.list [nodeValue 23, .num 0x3ff0000000000000], .unit]])

def mapNextStepReport : Report :=
  ownedReport
    [.loadItem 0, .field 0, .call "sum" 1, .pop,
     .loadItem 0, .field 1, .map 1, .loadLocal 0, .ret]
    (.record "Case" [
      .list (List.replicate 65535 (.num 0x3ff0000000000000)),
      .list [.num 0x3ff0000000000000, .num 0x4000000000000000]])

def filterKeepDropReport : Report :=
  ownedReport
    [.loadItem 0, .filter 2, .loadLocal 0, .field 1, .ret]
    (.list [
      .record "Item" [.num 0x3ff0000000000000, .bool true],
      .record "Item" [deepValue 65, .bool false]])

def recordReport (over : Bool) : Report :=
  rootReport
    ([.num 0x3ff0000000000000] ++
      repeatCode 23 [.bindLocal, .loadLocal 0, .loadLocal 0, .list 2, .dropLocal] ++
      (if over then [.num 0x3ff0000000000000, .record "Extent" 2]
       else [.record "Extent" 1]) ++ [.ret])

def rosterTarget (kind : String) : Code :=
  match kind with
  | "range" => [.num 0x4000000000000000, .call "range" 1]
  | "split" => [.str "a,b", .str ",", .call "split" 2]
  | "concat" =>
    [.num 0x3ff0000000000000, .list 1, .num 0x4000000000000000, .list 1,
     .call "concat" 2]
  | "flat" =>
    [.num 0x3ff0000000000000, .list 1, .num 0x4000000000000000, .list 1,
     .list 2, .call "flat" 1]
  | "slice" =>
    [.num 0x3ff0000000000000, .num 0x4000000000000000, .list 2,
     .num 0, .num 0x4000000000000000, .call "slice" 3]
  | "includes" =>
    [.num 0x3ff0000000000000, .num 0x4000000000000000, .list 2,
     .num 0x4000000000000000, .call "includes" 2]
  | "indexOf" =>
    [.num 0x3ff0000000000000, .num 0x4000000000000000, .list 2,
     .num 0x4000000000000000, .call "indexOf" 2]
  | "sortBy" =>
    [.num 0x4000000000000000, .num 0x3ff0000000000000, .list 2, .bindLocal,
     .loadLocal 0, .map 1, .loadLocal 1, .bindLocal,
     .loadLocal 0, .loadLocal 1, .bool false, .call "sortBy" 3,
     .dropLocal, .dropLocal]
  | _ => [.unit]

def rosterCost (kind : String) : Nat := if kind == "flat" then 4 else 2

def rosterBoundaryReport (kind : String) (past : Bool) : Report :=
  let spent := 65536 - rosterCost kind + if past then 1 else 0
  ownedReport
    ([.loadItem 0, .field 0, .call "sum" 1, .pop] ++ rosterTarget kind ++ [.ret])
    (.record "Case" [.list (List.replicate spent (.num 0x3ff0000000000000))])

def powString (base : String) : Nat → String
  | 0 => base
  | n + 1 => let s := powString base n; s ++ s

def powText (n : Nat) : String := powString "x" n

def powEmoji (n : Nat) : String := powString "😀" n

def emojiPowers (powers : List Nat) : String :=
  powers.foldl (fun out power => out ++ powEmoji power) ""

def replaceAllAmplifiedReport : Report :=
  rootReport [.str (powText 16), .str "x", .str (powText 16), .call "replaceAll" 3, .ret]

def sortOrderReport (descending equalKeys : Bool) : Report :=
  let keys := if equalKeys then [.num 0x3ff0000000000000, .num 0x3ff0000000000000]
    else if descending then [.num 0x3ff0000000000000, .num 0x4000000000000000]
    else [.num 0x4000000000000000, .num 0x3ff0000000000000]
  ownedReport
    [.loadItem 0, .field 0, .bindLocal,
     .loadItem 0, .field 1, .bindLocal,
     .loadLocal 0, .loadLocal 1, .bool descending, .call "sortBy" 3,
     .dropLocal, .dropLocal, .ret]
    (.record "Case" [
      .list [.str (powText 26 ++ "x"), deepValue 65],
      .list keys])

def localizedFillReport (past : Bool) : Report :=
  let fill := if past then powEmoji 24 else emojiPowers (List.range 24) ++ "xxx"
  suppliedReport "t"
    [.str "en", .str "key", .list [.str "value", .str fill]]
    { strings := [("en", [("key", "a{value}")])] }

def encodedReport (f : String) (past : Bool) : Report :=
  let input := "a" ++ emojiPowers [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22] ++
    " " ++ if past then "a" else ""
  suppliedReport f [.str input]

def failureReport (resource over pending : Bool) (reason : String) : Report :=
  let message := if over then "x" else ""
  if resource then
    rootReport [.failureResource 0, .ret]
      { resources := [if pending then none else some .unit]
        failedResources := [true]
        failures := [some (reason, message, reason)] }
  else
    rootReport [.failureMutation 0, .ret]
      { mutationFailures := [some (reason, message, reason)] }

def nestSome : Nat → Value → Value
  | 0, v => v
  | n + 1, v => .some (nestSome n v)

def extentOrderReport (large : String) : Report :=
  ownedReport [.loadItem 0, .some, .ret] (nestSome 65 (.str large))

def ownedResourceAfterCutReport (current : Bool) : Report :=
  let body : Code :=
    if current then [.currentResource 0, .ret] else [.loadResource 0, .ret]
  runReport [some { code := body, owner := some 0 }]
    [.loadDerive 0, .ret]
    { resources := [some (.num (F64.ofBits 0x401c000000000000))],
      currentResources := [true], frames := [{}, {}] }
    [some 1, some 0] (resourceOwners := [some (1, 0)]) (arms := [some 0, some 0])

def ownedResourceWrongArmReport (read : Nat) : Report :=
  let body : Code := match read with
    | 0 => [.loadResource 0, .ret]
    | 1 => [.pendingResource 0, .ret]
    | 2 => [.failedResource 0, .ret]
    | 3 => [.failureResource 0, .ret]
    | _ => [.currentResource 0, .ret]
  runReport [some { code := body, owner := some 0 }]
    [.loadDerive 0, .ret]
    { resources := [some (.num (F64.ofBits 0x401c000000000000))], pendingResources := [false]
      failedResources := [false], currentResources := [true], frames := [{}] }
    [some 0] (resourceOwners := [some (0, 1)]) (arms := [some 0])

def rowRestoresReaderFrameReport : Report :=
  runReport [some { code := [.unit, .ret], owner := some 0 }]
    [.loadDerive 0, .pop, .loadItem 0, .ret]
    { frames := [{ item := some (.num (F64.ofBits 0x4000000000000000)) },
                 { item := some (.num (F64.ofBits 0x3ff0000000000000)) }] }
    [some 1, some 0]

def outcomeName : Except Trap (Value × Vm.Effects) → String
  | .ok _ => "ok"
  | .error .stackUnderflow => "StackUnderflow"
  | .error (.typeMismatch _) => "TypeMismatch"
  | .error .unwrapNone => "UnwrapNone"
  | .error .badField => "BadField"
  | .error .badScope => "BadScope"
  | .error .badParam => "BadParam"
  | .error .arity => "Arity"
  | .error .writeNotDeclared => "WriteNotDeclared"
  | .error .badJump => "BadJump"
  | .error .malformed => "Malformed"
  | .error .noResult => "NoResult"
  | .error .pending => "Pending"
  | .error (.call (.type _)) => "TypeMismatch"
  | .error (.call .pending) => "Pending"
  | .error (.call (.refused why)) =>
    if why == "`range` of a count that is not a whole number at least 0" then "RangeCount"
    else if why == "a path parameter cannot be empty, `.` or `..`" then "PathParameter"
    else "ModelCallRefused"
  | .error (.call (.unbound _)) => "ModelCallUnbound"
  | .error (.call (.unsupported _)) => "ModelCallUnsupported"
  | .error (.unsupported what) =>
    match what with
    | "StringTooLong: string bytes" => "StringTooLong"
    | "ValueTooLarge: value extent" => "ValueTooLarge"
    | "ValueTooDeep: value depth" => "ValueTooDeep"
    | "IterationLimit: list steps" => "IterationLimit"
    | "CallLimit: executable derive calls" => "CallLimit"
    | "CallDepth: executable derive calls" => "CallDepth"
    | _ => "ModelUnsupported"
  | .error .outOfFuel => "OutOfFuel"

def emit (name : String) (report : Report) : IO Unit :=
  IO.println s!"{name} {outcomeName report.outcome} {report.accounting.calls} {report.accounting.listSteps} {report.accounting.callDepth}"

def emitValue (name : String) (report : Report) : IO Unit :=
  let outcome := match report.outcome with
    | .ok (value, _) => outcomeName report.outcome ++ " " ++ Observe.value value
    | .error _ => outcomeName report.outcome
  IO.println s!"{name} {outcome} {report.accounting.calls} {report.accounting.listSteps} {report.accounting.callDepth}"

def main : IO Unit := do
  emit "list-at" (listReport 65535)
  emit "list-past" (listReport 65536)
  emit "string-at" (rootReport (stringCode false))
  emit "string-past" (rootReport (stringCode true))
  emit "nodes-at" (rootReport (nodeCode false))
  emit "nodes-past" (rootReport (nodeCode true))
  emit "bytes-at" (rootReport (byteCode false))
  emit "bytes-past" (rootReport (byteCode true))
  emit "depth-at" (rootReport (depthCode 64))
  emit "depth-past" (rootReport (depthCode 65))
  emit "chain-48" (chainReport 48)
  emit "chain-49" (chainReport 49)
  emit "calls-at" (countReport 65536)
  emit "calls-past" (countReport 65537)
  emit "call-depth-at" (chainReport 64)
  emit "call-depth-past" (chainReport 65)
  emit "calls-depth-past" callsDepthReport
  emit "join-separator-unused" (joinSeparatorReport 0)
  emit "join-separator-at" (joinSeparatorReport 65535)
  emit "join-separator-past" (joinSeparatorReport 65536)
  emit "join-string-at" (joinLateTypeReport false)
  emit "join-string-past" (joinLateTypeReport true)
  emit "filter-type-unused" (filterTypeReport 0)
  emit "filter-type-at" (filterTypeReport 65535)
  emit "map-extent-before-next" mapExtentBeforeNextReport
  emit "map-next-step-past" mapNextStepReport
  emit "filter-keep-drop" filterKeepDropReport
  emit "range-at" (rosterBoundaryReport "range" false)
  emit "range-past" (rosterBoundaryReport "range" true)
  emit "record-at" (recordReport false)
  emit "record-past" (recordReport true)
  emit "split-at" (rosterBoundaryReport "split" false)
  emit "split-past" (rosterBoundaryReport "split" true)
  emit "concat-at" (rosterBoundaryReport "concat" false)
  emit "concat-past" (rosterBoundaryReport "concat" true)
  emit "flat-at" (rosterBoundaryReport "flat" false)
  emit "flat-past" (rosterBoundaryReport "flat" true)
  emit "slice-at" (rosterBoundaryReport "slice" false)
  emit "slice-past" (rosterBoundaryReport "slice" true)
  emit "includes-at" (rosterBoundaryReport "includes" false)
  emit "includes-past" (rosterBoundaryReport "includes" true)
  emit "indexOf-at" (rosterBoundaryReport "indexOf" false)
  emit "indexOf-past" (rosterBoundaryReport "indexOf" true)
  emit "sortBy-at" (rosterBoundaryReport "sortBy" false)
  emit "sortBy-past" (rosterBoundaryReport "sortBy" true)
  emit "sortBy-order-ascending" (sortOrderReport false false)
  emit "sortBy-order-descending" (sortOrderReport true false)
  emit "sortBy-order-equal" (sortOrderReport false true)
  emit "t-fill-at" (localizedFillReport false)
  emit "t-fill-past" (localizedFillReport true)
  emit "encodeURIComponent-at" (encodedReport "encodeURIComponent" false)
  emit "encodeURIComponent-past" (encodedReport "encodeURIComponent" true)
  emit "encodeRouteSegment-at" (encodedReport "encodeRouteSegment" false)
  emit "encodeRouteSegment-past" (encodedReport "encodeRouteSegment" true)
  emit "replaceAll-amplified" replaceAllAmplifiedReport
  let reason := powText 25
  emit "failure-resource-at" (failureReport true false false reason)
  emit "failure-resource-past" (failureReport true true false reason)
  emit "failure-resource-pending" (failureReport true true true reason)
  emit "failure-mutation-at" (failureReport false false false reason)
  emit "failure-mutation-past" (failureReport false true false reason)
  emit "extent-depth-first" (extentOrderReport (powText 26 ++ "x"))
  emit "owned-value-after-cut" (ownedResourceAfterCutReport false)
  emit "owned-current-after-cut" (ownedResourceAfterCutReport true)
  emit "owned-value-wrong-arm" (ownedResourceWrongArmReport 0)
  emit "owned-pending-wrong-arm" (ownedResourceWrongArmReport 1)
  emit "owned-failed-wrong-arm" (ownedResourceWrongArmReport 2)
  emit "owned-failure-wrong-arm" (ownedResourceWrongArmReport 3)
  emit "owned-current-wrong-arm" (ownedResourceWrongArmReport 4)
  emitValue "row-restores-reader-frame" rowRestoresReaderFrameReport
"#
}

/// Compare all fixed accounting boundaries with the production runner.
pub fn run() -> Result<bool, String> {
    leanrun::build_targets(&["Contract.VmRows"])?;
    let runner = runner_lines();
    let lean = leanrun::run_module(lean_module(), &work_dir(), "accounting")?;
    if runner == lean {
        println!("difftest accounting: {CASES} cases agree");
        return Ok(true);
    }
    for index in 0..runner.len().max(lean.len()) {
        let rust = runner.get(index).map(String::as_str).unwrap_or("<missing>");
        let model = lean.get(index).map(String::as_str).unwrap_or("<missing>");
        if rust != model {
            println!("ACCOUNTING {index}: runner `{rust}`, Lean `{model}`");
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "async lane: needs the Lean toolchain and boundary-size values"]
    fn resource_boundaries_agree() {
        assert!(run().unwrap());
    }
}
