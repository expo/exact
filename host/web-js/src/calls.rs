//! The plan's tools (LLP 1111 D2) for calls.js, which runs a call of one as
//! the runner's `modules/calls.rs` does (LLP 1133.023 D7, §10).

use crate::emit::surface_type_json;
use exact_plan::{DerivesId, Plan};
use std::fmt::Write as _;

/// Each tool by name: `[action, its parameters as a record type,
/// consequence, [additive, idempotent, closed, sends], description, fails,
/// returns, available, face, the type returns has]`, a clause a thunk of its
/// derive (the actions and derives are declared after this call) or null,
/// the face its JSON or null, the type null without `returns`. Returns the
/// import; empty for a plan with no tools.
pub fn tools(plan: &Plan, body: &mut String) -> String {
    if plan.tools.is_empty() {
        return String::new();
    }
    let text = |s: &str| serde_json::to_string(s).unwrap();
    let thunk = |d: Option<DerivesId>| d.map_or("null".into(), |d| format!("()=>d_{}()", d.0));
    let rows: Vec<String> = (plan.tools.iter().enumerate())
        .map(|(i, t)| {
            let action = plan.action(t.action);
            let params: Vec<String> = (action.params.iter())
                .map(|p| {
                    let p = plan.param(p);
                    format!(
                        "{}:{}",
                        text(plan.str(p.name)),
                        surface_type_json(plan, p.ty)
                    )
                })
                .collect();
            let face = match plan.str(t.face) {
                "" => "null",
                face => face,
            };
            let returns = t.returns.map_or("null".into(), |d| {
                surface_type_json(plan, plan.derives[d.0 as usize].ty)
            });
            format!(
                "{}:[()=>a_{},{{{}}},{},[{},{},{},{}],{},{},{},{},{face},{returns}]",
                text(plan.str(t.name)),
                t.action.0,
                params.join(","),
                text(plan.str(t.consequence)),
                t.additive,
                t.idempotent,
                t.closed,
                exact_runner::tools::sends(plan, i),
                text(plan.str(t.description)),
                thunk(t.fails),
                thunk(t.returns),
                thunk(t.available),
            )
        })
        .collect();
    let _ = write!(body, "$tools({{{}}});", rows.join(","));
    "import{tools as $tools}from\"./calls.js\";".into()
}
