//! A setting a launch may give (LLP 1153.002): the bake's check of the
//! module's `env.launch` grant lines against what app.json marks.

/// A host answers a launch's value only for a name the module's grants carry
/// (`env.launch`, LLP 1153.002 D2), so the module's grants, as it evaluated
/// them, carry exactly the names app.json marks, which the bake wrote:
/// a line that reached them another way is refused.
pub(crate) fn grants(grants: &str, settings: &serde_json::Value) -> Result<(), String> {
    let marked: std::collections::BTreeSet<&str> = (settings.as_object().into_iter().flatten())
        .filter(|(_, setting)| setting["launch"] == true)
        .map(|(name, _)| name.as_str())
        .collect();
    let carried: std::collections::BTreeSet<&str> =
        exact_runner::grants::launch_names(grants).collect();
    let stray: Vec<_> = carried
        .difference(&marked)
        .map(|n| format!("`env.launch {n}`"))
        .collect();
    let missing: Vec<_> = marked
        .difference(&carried)
        .map(|n| format!("`env.launch {n}`"))
        .collect();
    match (stray.is_empty(), missing.is_empty()) {
        (true, true) => Ok(()),
        (false, _) => Err(format!("app.ts: the module's grants carry {}, which app.json's \"env\" does not mark \"launch\": true; only the bake writes these lines", stray.join(", "))),
        (_, false) => Err(format!("app.ts: the module's grants lack {}, which the bake writes for each name app.json marks", missing.join(", "))),
    }
}

#[cfg(test)]
#[test]
fn the_module_grants_carry_exactly_the_names_app_json_marks() {
    let launch_grants = grants;
    let settings = serde_json::json!({"API": {"value": "x", "source": "default", "launch": true}, "PLAIN": {"value": "y", "source": "default"}});
    assert!(launch_grants("net.fetch https:\nenv.launch API", &settings).is_ok());
    let stray = launch_grants("env.launch API\nenv.launch HOME", &settings).unwrap_err();
    assert!(stray.contains("carry `env.launch HOME`"), "{stray}");
    let missing = launch_grants("net.fetch https:", &settings).unwrap_err();
    assert!(missing.contains("lack `env.launch API`"), "{missing}");
    assert!(launch_grants("env.launch HOME", &serde_json::Value::Null).is_err());
}
