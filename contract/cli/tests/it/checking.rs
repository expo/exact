//! Baseline compiler diagnostics, including imported and related locations.
//! Each batch is captured before migration; the fixture records its baseline.
//! @ref LLP 1172.019 §2 item 4.

use crate::scratch::Scratch;
use serde_json::Value;
use std::process::Command;

#[test]
fn checking_preserves_baseline_acceptance_and_diagnostic_provenance() {
    let corpus: Value = serde_json::from_str(include_str!("../fixtures/checking.json")).unwrap();
    let mut failures = Vec::new();
    for case in corpus["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dir = Scratch::new(format!(
            "exact-checking-oracle-{}-{name}",
            std::process::id()
        ));
        let root = dir.canonicalize().unwrap();
        for (path, text) in case["files"].as_object().unwrap() {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text.as_str().unwrap()).unwrap();
        }
        let output = Command::new(env!("CARGO_BIN_EXE_contract"))
            .args([
                "build",
                root.join("app.contract").to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        // Only the temporary app directory varies. Keep every primary and
        // related file, range, message, severity and diagnostic order.
        let text = text.replace(root.to_str().unwrap(), "$APP");
        let diagnostics: Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}: {text}"));
        if output.status.code().map(i64::from) != case["status"].as_i64()
            || diagnostics != case["diagnostics"]
            || !output.stderr.is_empty()
        {
            failures.push(format!(
                "{name}: exit {:?}; {diagnostics:#}; stderr {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn accepted_lowering_warnings_free_places_in_text_and_leave_the_json_inventory() {
    let dir = Scratch::new(format!("exact-warning-selection-{}", std::process::id()));
    let path = dir.join("app.contract");
    let app =
        "use Icon from \"./icon.contract\"\ncomponent App\n  view\n    column\n      Icon()\n";
    let icon = "component Icon\n  view\n    column\n      text \"icon\"\n";
    let image = "      image \"symbol:sf/plus\"\n";
    let comment = "      // exact-accept lower-symbol-alias\n";
    // Both files report an alias on line seven. Only the import accepts it.
    std::fs::write(
        dir.join("icon.contract"),
        format!("{icon}\n{comment}{image}{image}"),
    )
    .unwrap();
    std::fs::write(
        &path,
        format!(
            "{app}\n{}      view align-items=\"center\"\n        text \"hello\"\n",
            format!("{image}{comment}{image}").repeat(24)
        ),
    )
    .unwrap();
    let build = |flags: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_contract"))
            .arg("build")
            .arg(&path)
            .args(flags)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        output
    };
    let json = |flags: &[&str]| {
        let output = build(flags);
        assert!(output.stderr.is_empty());
        serde_json::from_slice::<Vec<Value>>(&output.stdout).unwrap()
    };
    let all = json(&["--json"]);
    assert_eq!(all.len(), 26);
    assert_eq!(all[0]["id"], "lower-flex-on-block");
    let lines = |file: &str| {
        all.iter()
            .filter(|w| {
                w["id"] == "lower-symbol-alias" && w["file"].as_str().unwrap().ends_with(file)
            })
            .map(|w| w["line"].as_u64().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(lines("icon.contract"), [8]);
    assert!(lines("app.contract").contains(&7));
    assert!(json(&["--json", "--errors-only"]).is_empty());

    let text = build(&[]);
    let text = String::from_utf8(text.stderr).unwrap();
    assert_eq!(
        text.matches("warning [lower-symbol-alias]").count(),
        19,
        "{text}"
    );
    assert_eq!(text.matches("warning [lower-flex-on-block]").count(), 1);
}
