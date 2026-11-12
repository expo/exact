//! A setting a launch may give (LLP 1153.002): the bake's checks of the
//! launch module's name and of the module's `env.launch` grant lines.

/// The launch module's name, whatever its extension, or a directory, in the
/// app's captured files: the entry's and app.ts's `./__exact_launch` is the
/// producer's.
pub(crate) fn module_name(
    captured: &std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
) -> Result<(), String> {
    match captured.keys().find(|path| {
        (path.components().next()).is_some_and(|first| {
            let first = first.as_os_str().to_string_lossy();
            first == "__exact_launch" || first.starts_with("__exact_launch.")
        })
    }) {
        Some(path) => Err(format!(
            "{}: __exact_launch is the producer's launch module; rename it",
            path.display()
        )),
        None => Ok(()),
    }
}

/// The settings app.ts read (`import.meta.env`, LLP 1153 D4), each with the
/// value inlined and its source (`typescript.mjs` `configure`; none without
/// app.ts), once the module's grants carry exactly the names they mark.
pub(crate) fn settings(stage: &std::path::Path, grants: &str) -> Result<serde_json::Value, String> {
    let settings = match std::fs::read(stage.join("__exact_env.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
        Err(_) => serde_json::Value::Null,
    };
    self::grants(grants, &settings)?;
    Ok(settings)
}

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
