//! A resource row's declaration: the root's `const r_<i>=res(…)`, and a
//! child's, made with each instance that owns it (LLP 1035.005.004 D1).

use super::{each, pages, type_code, type_json, value_js, HOST_FACTS};
use crate::code::{self, Scope, Uses};
use exact_plan::{Plan, Value};
use std::fmt::Write as _;

/// Row `i`'s declaration in the root, `const r_<i>=…;`: its arguments read
/// `scope`.
pub(super) fn declaration(
    plan: &Plan,
    i: usize,
    scope: &Scope,
    dev_reload: bool,
    uses: &mut Uses,
) -> Result<String, String> {
    let r = &plan.resources[i];
    let args = arguments(plan, i, scope, uses)?;
    if let Some(each) = each::resource(plan, i, &args, scope, uses)? {
        return Ok(each);
    }
    let res = if dev_reload {
        "$devRes".to_string()
    } else {
        uses.rt("res")
    };
    let call = call(plan, i, &args, scope, res, dev_reload, None, uses)?;
    let mut out = format!("const r_{i}={call};");
    // The runner's own resource shows its facts, never a write (overlay.js)
    if exact_plan::RUNNER_OWNED_SOURCES.contains(&plan.str(r.source)) {
        let _ = write!(out, "r_{i}.r.owned=true;");
    }
    Ok(out)
}

/// Row `i`, a child's resource, made on the instance `rows` holds as
/// `rows.r<i>`, its arguments and gate read in the instance's `scope`; `path`
/// is the JavaScript expression of the instance's path (the agent's
/// `instances`, runner/src/instance/owners.rs).
pub(super) fn owned(
    plan: &Plan,
    i: usize,
    rows: &str,
    path: &str,
    scope: &Scope,
    uses: &mut Uses,
) -> Result<String, String> {
    let args = arguments(plan, i, scope, uses)?;
    let res = uses.rt("res");
    let call = call(plan, i, &args, scope, res, false, Some(i), uses)?;
    let own = uses.rt("own");
    Ok(format!("{rows}.r{i}={own}(r_{i},()=>{call},{path});"))
}

fn arguments(plan: &Plan, i: usize, scope: &Scope, uses: &mut Uses) -> Result<Vec<String>, String> {
    let r = &plan.resources[i];
    let mut args = Vec::new();
    for (k, a) in r.args.iter().enumerate() {
        let f = code::expression(
            plan,
            plan.code(plan.arg(a).expr),
            &code::evaluation(
                scope,
                format!("resource {:?} argument {}", plan.str(r.name), k + 1),
                false,
            ),
            uses,
        )
        .map_err(|e| format!("resource {}: {e}", plan.str(r.name)))?;
        args.push(f);
    }
    Ok(args)
}

/// The `res(…)` call for row `i` with arguments `args`, built by `res`. An
/// instance's (`owned`) takes the bake's answer only when the boot render
/// made it, and the page's answer by its arguments (each.js `keptMember`).
#[allow(clippy::too_many_arguments)]
fn call(
    plan: &Plan,
    i: usize,
    args: &[String],
    scope: &Scope,
    res: String,
    dev_reload: bool,
    owned: Option<usize>,
    uses: &mut Uses,
) -> Result<String, String> {
    let r = &plan.resources[i];
    // What it shows while it waits with nothing kept (LLP 1048.003 D6,
    // 1054.000.002): an `else source()` row's value, a declared
    // `else empty(…)`, or the type's zero; a placeholder row has none.
    let is_placeholder = plan
        .resources
        .iter()
        .any(|o| o.placeholder.map(|p| p.0 as usize) == Some(i));
    let placeholder = match (r.placeholder, plan.bytes(r.placeholder_value)) {
        (Some(p), _) => format!("()=>r_{}()", p.0),
        (None, b) if !b.is_empty() => value_js(&Value::from_bytes(b).map_err(|e| e.to_string())?),
        _ if is_placeholder => "void 0".into(),
        _ => pages::zero(plan, r.ty),
    };
    // Host facts and store readers are asked at launch (LLP 1027 D4).
    let fact = HOST_FACTS.contains(&plan.str(r.source));
    let initial = plan.bytes(r.initial);
    let mut initial = if initial.is_empty() || fact {
        "void 0".to_string()
    } else {
        value_js(&Value::from_bytes(initial).map_err(|e| e.to_string())?)
    };
    let initial_args = plan.bytes(r.initial_args);
    let mut initial_args = if initial_args.is_empty() || fact || r.reader {
        "void 0".to_string()
    } else {
        value_js(&Value::from_bytes(initial_args).map_err(|e| e.to_string())?)
    };
    // Give an instance the bake's answer only while the boot makes it (the
    // runner's settlement.rs `launch`), whether or not its arguments are
    // written, which a store reader's are not
    if owned.is_some() {
        for value in [&mut initial, &mut initial_args] {
            if value != "void 0" {
                *value = format!("({}()?{value}:void 0)", uses.rt("booting"));
            }
        }
    }
    let res = each::constructor(r, res);
    // An `else` row's compiled answer is the bake's for every launch,
    // never asked again (each was a worker turn; the runner's
    // `is_placeholder_row`, review B4): it is settled, not a bake.
    let kept = if is_placeholder { ",1" } else { "" };
    let carry = if dev_reload {
        format!(",{}{kept}", type_json(plan, r.ty))
    } else {
        kept.to_string()
    };
    // Its last `context` arguments are its `with` context (LLP 1027.005
    // D6), which identify no upload it asks (uploads.js, LLP 1016.000 D10).
    let args = each::arguments(plan, r, args, scope, uses)?;
    let (name, source) = (
        serde_json::to_string(plan.str(r.name)).unwrap(),
        serde_json::to_string(plan.str(r.source)).unwrap(),
    );
    let ty = serde_json::to_string(&type_code(plan, r.ty)).unwrap();
    // An instance's page answer, found by its arguments as a list element's
    // is (each.js).
    let tail = match owned {
        Some(_) => {
            let kept_member = uses.rt("keptOwned");
            let carried = if is_placeholder { "1" } else { "0" };
            format!(",{carried},{kept_member}({name},{source},{args},{ty})")
        }
        None => carry,
    };
    Ok(format!(
        "{res}({name},{source},{args},{initial},{initial_args},{ty},{placeholder}{tail})"
    ))
}
