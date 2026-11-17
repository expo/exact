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
