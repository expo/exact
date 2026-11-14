use super::{passkey_rp_ids, payment_lines, sign_in};

/// Every line `exact add` advises for each package's grants (`grantLines`
/// in scripts/grant-advice.mjs), and for a `net.fetch` and an `fs.read` grant,
/// placeholders filled, parses as the app's grants do: the runner's whole
/// set, the device table, and the sign-in, passkey and payment grammars.
/// A line in another namespace's form (`device.auth.apple purpose.<key>`)
/// would fail the device table.
#[test]
fn grant_lines_parse() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let script = r#"
        import { readdirSync, readFileSync } from 'node:fs';
        import { grantLines } from './scripts/grant-advice.mjs';
        const all = readdirSync('packages').flatMap((p) => { try { return [[p, JSON.parse(readFileSync(`packages/${p}/exact.json`, 'utf8')).grants ?? []]]; } catch { return []; } });
        all.push(['(net.fetch)', ['net.fetch']], ['(fs.read)', ['fs.read']]);
        console.log(JSON.stringify(all.map(([p, g]) => [p, grantLines(g)])));
    "#;
    let out = std::process::Command::new("bun")
        .current_dir(&root)
        .args(["--eval", script])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let packages: Vec<(String, Vec<Vec<String>>)> = serde_json::from_slice(&out.stdout).unwrap();
    let filled = |line: &str| {
        let target = match line.split_whitespace().next() {
            Some("net.fetch") => "https://example.com",
            Some("fs.read") => "/data",
            _ => "",
        };
        line.replace("<key>", "k")
            .replace("<client id>", "com.example.app")
            .replace("<rp id>", "example.com")
            .replace("<merchant id>", "merchant.com.example.app")
            .replace("<its target>", target)
    };
    let mut checked = 0;
    for (package, grants) in &packages {
        for lines in grants {
            let spec = lines
                .iter()
                .map(|l| filled(l))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !spec.contains('<'),
                "{package}: `{spec}` has a placeholder this test does not fill"
            );
            let why = exact_runner::grants::parse(&spec)
                .err()
                .map(|e| e.join("; "))
                .or_else(|| {
                    exact_runner::device::device_grants(&spec)
                        .err()
                        .map(|e| e.join("; "))
                })
                .or_else(|| sign_in(&spec, "ios").err())
                .or_else(|| passkey_rp_ids(&spec).err())
                .or_else(|| payment_lines(&spec).err());
            assert!(
                why.is_none(),
                "{package}: `{spec}`: {}",
                why.unwrap_or_default()
            );
            checked += 1;
        }
    }
    let names = |p: &str| packages.iter().any(|(n, _)| n == p);
    for p in [
        "sign-in",
        "payments",
        "push",
        "notifications",
        "(net.fetch)",
        "(fs.read)",
    ] {
        assert!(names(p), "{p} was checked");
    }
    assert!(checked > 30, "{checked} lines");
    // The form the advice must not take: the device table refuses it.
    assert!(exact_runner::device::device_grants("device.auth.apple purpose.k").is_err());
}

/// Apply the diagnostic's lines verbatim, filling only its documented placeholders.
fn apple_metadata_cases() -> serde_json::Value {
    use serde_json::json;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let out = crate::bun()
        .current_dir(&root)
        .args(["--eval", r#"
            import { appleMetadataOwners, appleMetadataProblem } from './host/apple/plist.mjs';
            const cases = [];
            for (const [platform, kinds] of Object.entries(appleMetadataOwners()))
              for (const [kind, keys] of Object.entries(kinds))
                for (const [key, owners] of keys) for (const owner of owners) {
                  const message = appleMetadataProblem(`host.${platform}.${kind}.${key}: not a known key`);
                  if (!message.includes(owner)) throw new Error(message);
                  const lines = [...owner.matchAll(/`([^`]+)`/g)].map(m => m[1]);
                  if (!owner.startsWith('write ') || !lines.length) throw new Error(`unproven advice: ${owner}`);
                  cases.push({ platform, kind, key, owner, ceiling: lines.join('\n') });
                }
            console.log(JSON.stringify(cases));
        "#])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut cases: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    let dir = super::tests::app(&[("en", r#"{"purpose":"Use this device."}"#)]);
    for case in &mut cases {
        let platform = case["platform"].as_str().unwrap();
        let grants = case["ceiling"]
            .as_str()
            .unwrap()
            .replace("purpose.<key>", "purpose")
            .replace("<client id>", "com.example.app");
        assert!(!grants.contains('<'), "unfilled advice: {case}");
        let mut c = super::tests::compat(&grants);
        super::derive(&dir.0, platform, None, &mut c).unwrap_or_else(|e| panic!("{case}: {e}"));
        if case["kind"] == "infoPlist" {
            assert!(
                c.reach["usage"]
                    .get(case["key"].as_str().unwrap())
                    .is_some(),
                "{case}"
            );
        }
        case["ceiling"] = json!(grants);
        case["reach"] = c.reach;
    }
    for key in [
        "aps-environment",
        "com.apple.developer.applesignin",
        "NSPhotoLibraryAddUsageDescription",
        "NSMicrophoneUsageDescription",
    ] {
        assert!(
            cases.iter().any(|c| c["key"] == key),
            "missing diary case: {key}"
        );
    }
    assert!(cases.len() > 30, "{} advice cases", cases.len());
    json!(cases)
}

#[test]
fn apple_metadata_advice_grants_derive_without_added_prerequisites() {
    apple_metadata_cases();
}

/// The Apple writer integration is separate from the portable grant checks.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "async lane: Apple metadata writers"]
fn apple_metadata_advice_emits_the_key_on_the_named_bundle() {
    assert_eq!(
        std::env::consts::OS,
        "macos",
        "run the Apple integration on a Mac"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let out = crate::bun()
        .current_dir(root)
        .args(["test", "host/apple/metadata-advice.test.mjs"])
        .env("EXACT_METADATA_CASES", apple_metadata_cases().to_string())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let output = String::from_utf8_lossy(&out.stdout);
    assert!(
        output.contains("platform/key/owner writes verified"),
        "writer test did not run: {output}"
    );
    print!("{output}");
}
