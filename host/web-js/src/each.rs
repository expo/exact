//! A resource over a list (LLP 1017.000 P4c, amended 2026-11-04): a
//! resource for each element, keyed (`each.js`), whose arguments read the
//! element as `p0`; each shows its type's zero until it answers.

use super::pages::zero;
use super::type_code;
use crate::code::{self, Scope, Uses};
use exact_plan::Plan;

/// Row `i`'s declaration when it is over a list, its arguments `args`.
pub(super) fn resource(
    plan: &Plan,
    i: usize,
    args: &[String],
    top: &Scope,
    uses: &mut Uses,
) -> Result<Option<String>, String> {
    let r = &plan.resources[i];
    let Some((list, key)) = plan.resource_each(r) else {
        return Ok(None);
    };
    let mut part = |c, role: &str| {
        code::expression(
            plan,
            plan.code(c),
            &code::evaluation(
                top,
                format!("resource {:?} each {role}", plan.str(r.name)),
                false,
            ),
            uses,
        )
        .map_err(|e| format!("resource {}: {e}", plan.str(r.name)))
    };
    let (list, key) = (part(list, "subject")?, part(key, "key")?);
    let elem = plan.types[r.ty.0 as usize]
        .elem
        .ok_or("a list resource's type")?;
    Ok(Some(format!(
        "const r_{i}={}({},{},()=>{list},p0=>{key},p0=>[{}],{},{},{});",
        uses.rt("resEach"),
        serde_json::to_string(plan.str(r.name)).unwrap(),
        serde_json::to_string(plan.str(r.source)).unwrap(),
        args.join(","),
        serde_json::to_string(&type_code(plan, r.ty)).unwrap(),
        serde_json::to_string(&type_code(plan, elem)).unwrap(),
        zero(plan, elem),
    )))
}

/// The function that builds row `r`, given `res`: a store reader goes
/// through store-reader.js's `reader`, told whether it declares an `else`
/// (LLP 1027 D4).
pub(super) fn constructor(r: &exact_plan::ResourcesRow, res: String) -> String {
    if !r.reader {
        return res;
    }
    let declared = r.placeholder.is_some() || r.placeholder_value.len > 0;
    format!("$reader({res},{})", u8::from(declared))
}

/// Whether the plan builds a store reader (`constructor`): only then does
/// its page import store-reader.js.
pub(super) fn readers(plan: &Plan) -> bool {
    plan.resources
        .iter()
        .any(|r| r.reader && plan.resource_each(r).is_none())
}

/// A resource's arguments as `res()` takes them: a function, with its `with`
/// context count and its `when` (LLP 1161.002) when it has them.
pub(super) fn arguments(
    plan: &Plan,
    r: &exact_plan::ResourcesRow,
    args: &[String],
    top: &Scope,
    uses: &mut Uses,
) -> Result<String, String> {
    let mut props = Vec::new();
    if r.context > 0 {
        props.push(format!("context:{}", r.context));
    }
    if let Some(gate) = r.gate.iter().next() {
        let g = code::expression(
            plan,
            plan.code(plan.arg(gate).expr),
            &code::evaluation(top, format!("resource {:?} gate", plan.str(r.name)), false),
            uses,
        )
        .map_err(|e| format!("resource {}: {e}", plan.str(r.name)))?;
        props.push(format!("gate:()=>{g}"));
    }
    Ok(match props.is_empty() {
        true => format!("()=>[{}]", args.join(",")),
        false => format!(
            "Object.assign(()=>[{}],{{{}}})",
            args.join(","),
            props.join(",")
        ),
    })
}
