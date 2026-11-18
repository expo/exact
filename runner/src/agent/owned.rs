//! A child's resources in the agent's `state` (LLP 1035.005.004 D8): the
//! names its components declare them by, and each live instance's under its
//! path.

use crate::json::{quote, typed_json};
use crate::{DataSource, Runner};

/// The name a child's declaration was written with: lifting renames each
/// use's apart as `name#n`.
pub(crate) fn declared_name(lifted: &str) -> &str {
    lifted.split('#').next().unwrap_or(lifted)
}

/// `"owned"` and `"instances"`, when the plan declares a child's resource.
pub(super) fn state<D: DataSource>(runner: &Runner<D>, s: &mut String) {
    let plan = runner.plan();
    let declared = runner
        .declared_resources()
        .iter()
        .filter(|r| r.owner.is_some());
    let mut names: Vec<&str> = declared.map(|r| declared_name(plan.str(r.name))).collect();
    names.sort_unstable();
    names.dedup();
    if !names.is_empty() {
        s.push_str(",\"owned\":[");
        for (k, name) in names.iter().enumerate() {
            if k > 0 {
                s.push(',');
            }
            quote(name, s);
        }
        s.push(']');
    }
    let owned = runner.instance_resources();
    if !owned.is_empty() {
        let mut paths: Vec<&str> = owned.iter().map(|(p, ..)| *p).collect();
        paths.sort_unstable();
        paths.dedup();
        s.push_str(",\"instances\":{");
        for (k, path) in paths.iter().enumerate() {
            if k > 0 {
                s.push(',');
            }
            quote(path, s);
            s.push_str(":{");
            for (j, (_, site, value)) in owned.iter().filter(|(p, ..)| p == path).enumerate() {
                if j > 0 {
                    s.push(',');
                }
                let row = &plan.resources[*site];
                quote(declared_name(plan.str(row.name)), s);
                s.push(':');
                match value {
                    Some(v) => typed_json(plan, row.ty, v, s),
                    None => s.push_str("null"),
                }
            }
            s.push('}');
        }
        s.push('}');
    }
}
