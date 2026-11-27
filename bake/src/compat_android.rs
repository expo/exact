//! What an APK is composed from, as its compatibility inputs record it
//! (LLP 1030.006 D4a).

/// What an APK is composed from beyond the compatibility inputs' other
/// fields (LLP 1030.006 D4a): the capabilities it links, the plan's
/// (`uses.txt`, which decides what the reader links) and those every APK
/// ships (`Uses::ANDROID_CORE`), in the roster's order, so one set records
/// one way; the Android permissions its grant lines
/// and the source's use of module names derive (`permissions.txt`), and the
/// package extensions it binds a Kotlin implementation for. A generation
/// that needs more than this is a new APK; the hatch words are `hatches`.
pub(crate) fn android_linked(
    plan: &exact_plan::Plan,
    ceiling: Option<&str>,
    source: &str,
) -> serde_json::Value {
    let mut uses = android_uses(plan);
    let core = exact_runner::Uses::ANDROID_CORE.beyond(exact_runner::uses(plan));
    uses.extend(core.iter().map(|c| c.name()));
    uses.sort_by_key(|u| {
        exact_runner::Capability::ALL
            .iter()
            .position(|c| c.name() == *u)
    });
    serde_json::json!({
        "uses": uses,
        "permissions": contract::native::android_permissions(plan, ceiling, source),
        "extensions": plan_extensions(plan),
    })
}

/// What the APK links for the plan, by name, as its build reads it
/// (`contract::native::android_reach`).
pub(crate) use contract::native::android_uses;

/// The package extensions the plan's modules, views and events name, sorted.
pub(crate) fn plan_extensions(plan: &exact_plan::Plan) -> Vec<String> {
    plan.package_extensions()
        .into_keys()
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
#[path = "compat_android_tests.rs"]
mod tests;
