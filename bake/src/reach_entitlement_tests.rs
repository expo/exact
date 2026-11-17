//! Rows whose entitlement takes a value that is no Boolean.

use super::{
    derive,
    tests::{app, compat},
};
use serde_json::json;

#[test]
fn cloudkit_derives_its_arrays_without_usage_strings_or_push() {
    let dir = app(&[("en", "{}")]);
    let services = "com.apple.developer.icloud-services";
    let containers = "com.apple.developer.icloud-container-identifiers";
    let environment = "com.apple.developer.icloud-container-environment";
    for target in [None, Some("iCloud.com.example.shared")] {
        let line = format!(
            "device.cloudkit{}",
            target.map_or(String::new(), |t| format!(" {t}"))
        );
        for platform in ["ios", "macos", "linux"] {
            let mut c = compat(&line);
            derive(&dir.0, platform, None, &mut c).unwrap();
            assert_eq!(c.reach["usage"], json!({}));
            assert!(c.reach.get("backgroundModes").is_none());
            if platform == "linux" {
                assert!(c.reach.get("entitlementValues").is_none());
                continue;
            }
            let keys = json!([environment, containers, services]);
            assert_eq!(
                c.reach[if platform == "ios" {
                    "entitlements"
                } else {
                    "profileEntitlements"
                }],
                keys
            );
            assert_eq!(c.reach["entitlementValues"][services], json!(["CloudKit"]));
            assert_eq!(
                c.reach["entitlementValues"][containers],
                target.map_or(json!(null), |t| json!([t]))
            );
            assert!(c.reach["entitlementValues"].get(environment).is_none());
            if platform == "ios" {
                assert_eq!(c.reach["tvOmits"], keys);
            }
        }
    }
}

#[test]
fn nfc_signs_its_formats_array_on_ios_and_leaves_tvos_out() {
    // LLP 1133.035 D15, D16 item 2.
    let dir = app(&[("en", r#"{"nfc": "Reads the tags on your books."}"#)]);
    let derive_for = |platform: &str| {
        let mut c = compat("device.nfc nfc");
        derive(&dir.0, platform, None, &mut c).unwrap();
        c.reach
    };
    let formats = "com.apple.developer.nfc.readersession.formats";
    let ios = derive_for("ios");
    assert_eq!(
        ios["usage"],
        json!({"NFCReaderUsageDescription": {"en": "Reads the tags on your books."}})
    );
    assert_eq!(ios["entitlements"], json!([formats]));
    assert_eq!(ios["entitlementValues"], json!({formats: ["TAG"]}));
    assert_eq!(
        ios["tvOmits"],
        json!(["NFCReaderUsageDescription", formats])
    );
    // Android's bake derives as `linux`.
    assert_eq!(
        derive_for("linux")["android"],
        json!(["android.permission.NFC"])
    );
    let mac = derive_for("macos");
    assert_eq!(mac["entitlements"], json!([]));
    assert!(mac.get("entitlementValues").is_none());
}

#[test]
fn sensitive_content_analysis_signs_its_array_on_ios_and_the_mac_and_leaves_tvos_out() {
    // expo/exact#708.
    let dir = app(&[("en", "{}")]);
    let derive_for = |platform: &str| {
        let mut c = compat("device.sensitive-content-analysis");
        derive(&dir.0, platform, None, &mut c).unwrap();
        c.reach
    };
    let key = "com.apple.developer.sensitivecontentanalysis.client";
    let ios = derive_for("ios");
    assert_eq!(ios["usage"], json!({}));
    assert_eq!(ios["entitlements"], json!([key]));
    assert_eq!(ios["entitlementValues"], json!({key: ["analysis"]}));
    assert_eq!(ios["tvOmits"], json!([key]));
    let mac = derive_for("macos");
    assert_eq!(mac["entitlements"], json!([]));
    assert_eq!(mac["profileEntitlements"], json!([key]));
    assert_eq!(mac["entitlementValues"], json!({key: ["analysis"]}));
    assert_eq!(derive_for("linux")["android"], json!([]));
}
