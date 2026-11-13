//! Rust literals for the bake's parsed process-start definitions (LLP 1162.000 D2).

use serde_json::Value;

/// Emit definitions already checked by the bake, without device-side template parsing.
pub fn preload_expression(parsed: &Value) -> String {
    let root = "exact_logic::exact_runner::request";
    let text = |v: &Value| format!("{:?}", v.as_str().unwrap());
    let parts = |v: &Value| {
        let values = v
            .as_array()
            .unwrap()
            .iter()
            .map(|part| {
                let kind = if part[0] == "literal" {
                    "Literal"
                } else {
                    "Secret"
                };
                format!("{root}::PreloadPart::{kind}({})", text(&part[1]))
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("&[{values}]")
    };
    let definitions = parsed.as_array().into_iter().flatten().map(|entry| {
        let headers = entry["headers"].as_array().unwrap().iter().map(|h| format!("({}, {})", text(&h[0]), parts(&h[1]))).collect::<Vec<_>>().join(",");
        let secrets = entry["secrets"].as_array().unwrap().iter().map(text).collect::<Vec<_>>().join(",");
        let body = if entry["body"].is_null() { "None".into() } else { format!("Some({})", parts(&entry["body"])) };
        let timeout = entry["timeoutMs"].as_u64().map_or("None".into(), |n| format!("Some({n})"));
        let when = if entry["when"] == "foreground" { "Foreground" } else { "Start" };
        format!("{root}::Preload {{ name: {}, method: {}, url: {}, headers: &[{headers}], body: {body}, timeout_ms: {timeout}, when: {root}::PreloadWhen::{when}, secrets: &[{secrets}] }}", text(&entry["name"]), text(&entry["method"]), parts(&entry["url"]))
    }).collect::<Vec<_>>().join(",\n");
    format!("&[{definitions}]")
}
