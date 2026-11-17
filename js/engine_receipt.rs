use std::path::Path;

/// v5 predates build flags. A bundle claiming recoverable OOM must supply
/// the exact flags the shim relies on, including RTTI for its exception catch.
pub(crate) fn oom_enabled(bytes: Option<&[u8]>, engine: &Path) -> Result<bool, &'static str> {
    let receipt = bytes.and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok());
    let flags = receipt
        .as_ref()
        .and_then(|r| r["build"]["flags"].as_array());
    let enabled = [
        "-DHERMESVM_EXCEPTION_ON_OOM=true",
        "-DHERMES_ENABLE_RTTI=true",
    ]
    .iter()
    .all(|required| flags.is_some_and(|flags| flags.iter().any(|flag| flag == required)));
    let v6 = |tag: &str| tag == "v6" || tag.ends_with("-v6");
    let tagged = engine
        .components()
        .any(|part| v6(&part.as_os_str().to_string_lossy()))
        || receipt
            .as_ref()
            .and_then(|r| r["tag"].as_str())
            .is_some_and(v6);
    let declared = flags.is_some_and(|flags| {
        flags.iter().any(|flag| {
            flag.as_str().is_some_and(|flag| {
                flag.to_ascii_uppercase()
                    .contains("HERMESVM_EXCEPTION_ON_OOM")
            })
        })
    });
    if !enabled && (tagged || declared) {
        return Err("recoverable-OOM Hermes requires build.flags containing both -DHERMESVM_EXCEPTION_ON_OOM=true and -DHERMES_ENABLE_RTTI=true; rebuild the bundle with these exact flags");
    }
    Ok(enabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recovery_claims_require_both_exact_flags_and_v5_stays_supported() {
        let v5 = Path::new("hermes-vanilla-d412d3bd8512-v5/digest");
        let v6 = Path::new("hermes-vanilla-d412d3bd8512-v6/digest");
        for bytes in [None, Some(b"{}".as_slice()), Some(b"invalid".as_slice())] {
            assert_eq!(oom_enabled(bytes, v5), Ok(false));
            assert!(oom_enabled(bytes, v6).is_err());
        }
        for flags in [json!([]), json!(["-DHERMES_ENABLE_RTTI=true"])] {
            let bytes = json!({"build": {"flags": flags}}).to_string();
            assert_eq!(oom_enabled(Some(bytes.as_bytes()), v5), Ok(false));
            assert!(oom_enabled(Some(bytes.as_bytes()), v6).is_err());
        }
        for oom in [
            "-DHERMESVM_EXCEPTION_ON_OOM=true",
            "-DHERMESVM_EXCEPTION_ON_OOM=ON",
            "-DHERMESVM_EXCEPTION_ON_OOM:BOOL=1",
            "-dhermesvm_exception_on_oom=True",
        ] {
            for flags in [json!([oom]), json!([oom, "-DHERMES_ENABLE_RTTI=ON"])] {
                let bytes = json!({"build": {"flags": flags}}).to_string();
                let error = oom_enabled(Some(bytes.as_bytes()), v5).unwrap_err();
                assert!(error.contains("-DHERMESVM_EXCEPTION_ON_OOM=true"));
                assert!(error.contains("-DHERMES_ENABLE_RTTI=true"));
            }
        }
        let bytes = json!({"build": {"flags": [
            "-DHERMESVM_EXCEPTION_ON_OOM=true", "-DHERMES_ENABLE_RTTI=true"
        ]}})
        .to_string();
        for path in [v5, v6] {
            assert_eq!(oom_enabled(Some(bytes.as_bytes()), path), Ok(true));
        }
        assert!(oom_enabled(Some(br#"{"tag":"hermes-vanilla-d412d3bd8512-v6"}"#), v5).is_err());
    }
}
