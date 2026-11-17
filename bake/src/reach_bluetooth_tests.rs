use super::{
    derive,
    tests::{app, compat},
};
use serde_json::json;

#[test]
fn bluetooth_derives_platform_metadata_from_its_localized_purpose() {
    let dir = app(&[
        ("en", r#"{"purpose.strap":"Reads your heart rate."}"#),
        (
            "fr",
            r#"{"purpose.strap":"Lit votre fréquence cardiaque."}"#,
        ),
    ]);
    let grants = "device.bluetooth purpose.strap\ndevice.bluetooth-background";
    for platform in ["ios", "macos", "linux", "web", "windows"] {
        let mut c = compat(grants);
        derive(&dir.0, platform, None, &mut c).unwrap();
        assert_ne!(c.id, "old");
        if matches!(platform, "ios" | "macos") {
            assert_eq!(
                c.reach["usage"],
                json!({"NSBluetoothAlwaysUsageDescription": {
                    "en": "Reads your heart rate.", "fr": "Lit votre fréquence cardiaque."
                }})
            );
        } else {
            assert_eq!(c.reach["usage"], json!({}));
        }
        assert_eq!(c.reach["entitlements"], json!([]));
        if platform == "ios" {
            assert_eq!(c.reach["backgroundModes"], json!(["bluetooth-central"]));
        } else {
            assert!(c.reach.get("backgroundModes").is_none());
        }
        if platform == "linux" {
            assert_eq!(
                c.reach["android"],
                json!([
                    "android.permission.BLUETOOTH maxSdkVersion=30",
                    "android.permission.BLUETOOTH_CONNECT"
                ])
            );
            assert_eq!(
                c.reach["androidFeatures"],
                json!([
                    "android.hardware.bluetooth_le required=false",
                    "android.software.companion_device_setup"
                ])
            );
        } else {
            assert_eq!(c.reach["android"], json!([]));
        }
    }
    let mut foreground = compat("device.bluetooth purpose.strap");
    derive(&dir.0, "ios", None, &mut foreground).unwrap();
    assert!(foreground.reach.get("backgroundModes").is_none());
    let mut missing = compat("device.bluetooth purpose.missing");
    assert!(derive(&dir.0, "ios", None, &mut missing)
        .unwrap_err()
        .contains("grant-purpose:"));
    let mut blank = compat("device.bluetooth purpose.strap");
    let dir = app(&[("en", r#"{"purpose.strap":" "}"#)]);
    assert!(derive(&dir.0, "macos", None, &mut blank)
        .unwrap_err()
        .contains("grant-purpose:"));
}
