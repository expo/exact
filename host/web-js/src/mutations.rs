//! A mutation row's declaration: the root's `const m_<i>=mut(…)`, and a
//! child's, made with each instance of its arm (LLP 1035.005.004 D6).

use super::type_code;
use crate::code::{self, Scope, Uses};
use exact_plan::{MutationMode, Plan};

/// Row `i`'s `mut(…)` call: `name` and `slot` are JavaScript expressions,
/// and `rows`, a child's instance, holds the requests its refreshes name.
fn call(
    plan: &Plan,
    i: usize,
    name: &str,
    slot: &str,
    rows: Option<&str>,
    uses: &mut Uses,
) -> String {
    let m = &plan.mutations[i];
    let refreshes: Vec<String> = (m.refreshes.iter())
        .map(|x| {
            let r = plan.mutation_refreshes[x.0 as usize].resource.0;
            match rows {
                Some(rows) if plan.resources[r as usize].owner.is_some() => format!("{rows}.r{r}"),
                _ => format!("r_{r}"),
            }
        })
        .collect();
    let mt = uses.rt("mut");
    // A queue's sends wait their turn (LLP 1092 D6; schedule.js); a parallel
    // one's run together (LLP 1161.000 D6).
    format!(
        "{mt}({name},{slot},[{}],{}{})",
        refreshes.join(","),
        serde_json::to_string(&type_code(plan, m.ty)).unwrap(),
        match m.mode {
            MutationMode::Newest => "",
            MutationMode::Queue => ",1",
            MutationMode::Parallel => ",2",
        }
    )
}

/// Row `i`'s declaration in the root, `const m_<i>=…;`: a child's is a stub,
/// each instance making its own (`owned`).
pub(super) fn declaration(plan: &Plan, i: usize, uses: &mut Uses) -> String {
    let m = &plan.mutations[i];
    let name = serde_json::to_string(plan.str(m.name)).unwrap();
    if m.owner.is_some() {
        let stub = uses.rt("ownedMut");
        return format!("const m_{i}={stub}({name});");
    }
    format!(
        "const m_{i}={};",
        call(plan, i, &name, &format!("s_{}", m.slot.0), None, uses)
    )
}

/// The root row `i`'s `then`, set once the actions are declared: its
/// arguments (a child's captured props, for a use outside every region) are
/// read in the root's `scope` when it runs.
pub(super) fn root_then(
    plan: &Plan,
    i: usize,
    scope: &Scope,
    uses: &mut Uses,
) -> Result<String, String> {
    if plan.mutations[i].owner.is_some() {
        return Ok(String::new());
    }
    Ok(match then(plan, i, None, scope, uses)? {
        Some(f) => format!("m_{i}.then={f};"),
        None => String::new(),
    })
}

/// Row `i`, a child's mutation, made on the instance `rows` holds as
/// `rows.m<i>`, after its state and requests: its slot is the instance's,
/// its refreshes the instance's requests, and its `then` runs on the instance,
/// its arguments read in the instance's `scope`.
pub(super) fn owned(
    plan: &Plan,
    i: usize,
    rows: &str,
    scope: &Scope,
    uses: &mut Uses,
) -> Result<String, String> {
    let slot = format!("{rows}[{}]", plan.mutations[i].slot.0);
    let make = call(plan, i, "n", &slot, Some(rows), uses);
    let then = then(plan, i, Some(rows), scope, uses)?.unwrap_or_else(|| "0".into());
    let own = uses.rt("ownMut");
    Ok(format!("{rows}.m{i}={own}(m_{i},n=>{make},{then});"))
}

/// Row `i`'s `then` as a function the clock's commit calls, or `None` when it
/// has none: the action itself, or a call of it with the instance `rows` in
/// force and the plan's `then_args`.
fn then(
    plan: &Plan,
    i: usize,
    rows: Option<&str>,
    scope: &Scope,
    uses: &mut Uses,
) -> Result<Option<String>, String> {
    let m = &plan.mutations[i];
    let Some(action) = m.then else {
        return Ok(None);
    };
    let mut args = Vec::new();
    // An action that touches row slots takes the row in force first (emit.rs).
    if code::touches_rows(plan, plan.code(plan.actions[action.0 as usize].body)) {
        args.push(rows.unwrap_or("{}").to_string());
    }
    let declared = plan.str(m.name).split('#').next().unwrap_or_default();
    for a in m.then_args.iter() {
        let e = code::expression(
            plan,
            plan.code(plan.arg(a).expr),
            &code::evaluation(scope, format!("mutation {declared:?} then"), false),
            uses,
        )
        .map_err(|e| format!("mutation {declared} then: {e}"))?;
        args.push(e);
    }
    Ok(Some(if args.is_empty() {
        format!("a_{}", action.0)
    } else {
        format!("()=>a_{}({})", action.0, args.join(","))
    }))
}
