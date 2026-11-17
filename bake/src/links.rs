//! The verified https links an app claims, as the compatibility identity records them (LLP 1038 D8).
use serde_json::{json, Value};

/// The link claims a binary bakes in (LLP 1038 D8), as the manifest writes
/// them: on Apple the claimed origins, which the entitlements sign as
/// `applinks:`; on Android the origins and the scope, which its filters
/// carry. A scope alone changes only the Apple origin's file, so it is not
/// Apple's. Nothing unless `handle_links` is `preferred`.
pub(crate) fn claims(json: &Value, platform: &str) -> Option<Value> {
    if json.get("handle_links").and_then(Value::as_str) != Some("preferred") {
        return None;
    }
    let origins: Vec<Value> = std::iter::once(json.pointer("/app/origin"))
        .chain(
            json.get("scope_extensions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|e| e.get("origin")),
        )
        .flatten()
        .cloned()
        .collect();
    match platform {
        "macos" | "ios" => Some(json!({ "origins": origins })),
        "android" => Some(json!({
            "origins": origins,
            "scope": json.get("scope").cloned().unwrap_or(Value::Null),
            "startUrl": json.get("start_url").cloned().unwrap_or(Value::Null),
        })),
        _ => None,
    }
}
