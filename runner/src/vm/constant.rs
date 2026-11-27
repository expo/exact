use super::*;

/// Why an expression could not run in a constant environment.
#[derive(Debug)]
pub struct ConstantFailure {
    /// The runtime input execution reached, if any.
    pub read: Option<String>,
    /// The evaluator's refusal, including its work and value limits.
    pub trap: Trap,
}

/// Run the interpreter with literal parameters and no runtime inputs.
/// Only executed instructions can read an input or exhaust a limit.
pub fn eval_constant(
    code: &[u8],
    plan: &Plan,
    params: &[Value],
) -> (Result<Value, ConstantFailure>, Accounting) {
    let strings = intern(plan);
    let env = Env {
        plan,
        strings: &strings,
        router: None,
        lists: None,
        format: Some(crate::formatting),
        geometry: None,
        slots: &[],
        derives: &[],
        resources: (&[]).into(),
        params,
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
    let (result, (read, accounting)) =
        eval_machine_main::<true, _>(code, &env, &[], &mut Scratch::default(), None, |run| {
            (
                run.refused_read.clone(),
                Accounting {
                    calls: run.calls,
                    list_steps: run.steps,
                    call_depth: run.depth,
                },
            )
        });
    (
        result
            .map(|outcome| outcome.value)
            .map_err(|trap| ConstantFailure { read, trap }),
        accounting,
    )
}

pub(super) fn read(plan: &Plan, ins: &Instruction) -> Option<String> {
    let i = ins.args[0] as usize;
    Some(match ins.op {
        Opcode::LoadSlot | Opcode::StoreSlot => plan.str(plan.slots.get(i)?.name).into(),
        Opcode::LoadDerive => plan.str(plan.derives.get(i)?.name).into(),
        Opcode::LoadResource
        | Opcode::PendingResource
        | Opcode::CurrentResource
        | Opcode::FailedResource
        | Opcode::FailureResource
        | Opcode::Refresh => plan.str(plan.resources.get(i)?.name).into(),
        Opcode::PendingMutation
        | Opcode::FailedMutation
        | Opcode::FailureMutation
        | Opcode::Send => plan.str(plan.mutations.get(i)?.name).into(),
        Opcode::Command => format!("{}()", plan.str(exact_plan::StrId(i as u32))),
        Opcode::LoadItem | Opcode::LoadIndex | Opcode::LoadBound => "component prop".into(),
        Opcode::Call => {
            let f = Stdlib::from_wire(i as u8)?;
            if !f.localized()
                && !matches!(
                    f,
                    Stdlib::FormatShortcut
                        | Stdlib::PerformanceNow
                        | Stdlib::T
                        | Stdlib::Frame
                        | Stdlib::RangeFrame
                        | Stdlib::ScrollMetrics
                        | Stdlib::Measure
                        | Stdlib::ElementFromPoint
                        | Stdlib::ElementsFromPoint
                        | Stdlib::QuerySelectorAll
                        | Stdlib::Open
                        | Stdlib::Push
                        | Stdlib::Replace
                        | Stdlib::Back
                        | Stdlib::Select
                        | Stdlib::Go
                        | Stdlib::Stack
                        | Stdlib::Top
                        | Stdlib::Depth
                        | Stdlib::Params
                        | Stdlib::SearchParam
                        | Stdlib::Fragment
                        | Stdlib::SharedFiles
                )
            {
                return None;
            }
            format!("{}()", f.name())
        }
        _ => return None,
    })
}
