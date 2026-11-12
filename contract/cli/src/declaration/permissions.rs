//! A cycle among an interface's permissions, refused. A permission's state
//! reads those it `requires` on the target (LLP 1133.030 D12 item 1) and the
//! one whose grant includes it (`grantedBy`, LLP 1133.019 D5), so in a cycle
//! a state would rest on its own.

use serde_json::Value;

/// Refuses the first cycle among `permissions` through `grantedBy` and one
/// target's `requires`, naming each step. A permission another interface
/// declares is not followed: the runner stops at a name already read.
pub(super) fn acyclic(permissions: &[Value]) -> Result<(), String> {
    let mut targets: Vec<&str> = (permissions.iter())
        .filter_map(|p| p["requires"].as_object())
        .flat_map(|by| by.keys().map(String::as_str))
        .collect();
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() {
        // `grantedBy` alone, on every target.
        targets.push("");
    }
    for target in targets {
        // The permissions `name`'s state reads on `target`, each with its step.
        let reads = |name: &str| -> Vec<(String, String)> {
            let Some(p) = permissions.iter().find(|p| p["name"] == name) else {
                return Vec::new();
            };
            let by = (p["grantedBy"].as_str())
                .map(|b| (b.to_owned(), format!("`{name}` is granted by `{b}`")));
            let required = (p["requires"][target].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .map(|r| (r.to_owned(), format!("`{name}` requires `{r}` on {target}")));
            by.into_iter()
                .chain(required)
                .filter(|(n, _)| n != name)
                .collect()
        };
        for name in permissions.iter().filter_map(|p| p["name"].as_str()) {
            if let Some(steps) = cycle(name, &reads, &mut Vec::new(), &mut Vec::new()) {
                return Err(format!(
                    "the declared permissions' states rest on each other: {}",
                    steps.join(", and ")
                ));
            }
        }
    }
    Ok(())
}

/// The steps of a cycle reached from `name`, or none. `names` are the
/// permissions being read, `steps` the step from each to the next.
fn cycle(
    name: &str,
    reads: &dyn Fn(&str) -> Vec<(String, String)>,
    names: &mut Vec<String>,
    steps: &mut Vec<String>,
) -> Option<Vec<String>> {
    names.push(name.to_owned());
    for (next, step) in reads(name) {
        if let Some(at) = names.iter().position(|n| *n == next) {
            let mut found = steps[at..].to_vec();
            found.push(step);
            return Some(found);
        }
        steps.push(step);
        if let Some(found) = cycle(&next, reads, names, steps) {
            return Some(found);
        }
        steps.pop();
    }
    names.pop();
    None
}
