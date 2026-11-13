//! The Linux service against `modules/observe/tests/wire.json`, the bodies all
//! three Observe services must send. `wire.test.mjs` runs the web and Apple ones.

use super::*;

const FIXTURE: &str = include_str!("../../../../../modules/observe/tests/wire.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

/// Numbers as f64, so `1` and `1.0` compare equal as they do once parsed in JS.
fn floats(v: &mut Value) {
    match v {
        Value::Number(n) => *v = json!(n.as_f64()),
        Value::Array(a) => a.iter_mut().for_each(floats),
        Value::Object(o) => o.values_mut().for_each(floats),
        _ => {}
    }
}

/// A body in the form wire.json compares: see its `about`.
fn normalize(mut body: Value, platform: &str) -> Value {
    let f = fixture();
    let shared = &f["expected"]["metrics"]["resourceMetrics"][0]["resource"]["attributes"];
    let mut keep: Vec<String> = shared
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["key"].as_str().unwrap().to_string())
        .collect();
    keep.extend(
        f["platforms"][platform]["resource"]
            .as_object()
            .unwrap()
            .keys()
            .cloned(),
    );
    for r in body
        .as_object_mut()
        .unwrap()
        .values_mut()
        .flat_map(|v| v.as_array_mut().unwrap())
    {
        let attrs = r["resource"]["attributes"].as_array_mut().unwrap();
        attrs.retain(|a| keep.iter().any(|k| a["key"] == k.as_str()));
        attrs.sort_by(|a, b| a["key"].as_str().cmp(&b["key"].as_str()));
        for scope in r
            .get_mut("scopeMetrics")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            let metrics = scope["metrics"].as_array_mut().unwrap();
            for a in metrics.iter_mut().flat_map(|m| {
                m["gauge"]["dataPoints"][0]["attributes"]
                    .as_array_mut()
                    .unwrap()
            }) {
                if let (true, Some(text)) = (
                    a["key"] == "expo.custom_params",
                    a["value"]["stringValue"].as_str(),
                ) {
                    a["value"]["stringValue"] = serde_json::from_str(text).unwrap();
                }
            }
            let time = |m: &Value| m["gauge"]["dataPoints"][0]["timeUnixNano"].as_u64();
            metrics
                .sort_by(|a, b| (a["name"].as_str(), time(a)).cmp(&(b["name"].as_str(), time(b))));
        }
    }
    floats(&mut body);
    body
}

/// wire.json's expected body with the platform's own resource attributes and launch params added.
fn expected(signal: &str, platform: &str) -> Value {
    let f = fixture();
    let p = &f["platforms"][platform];
    let mut body = f["expected"][signal].clone();
    for r in body
        .as_object_mut()
        .unwrap()
        .values_mut()
        .flat_map(|v| v.as_array_mut().unwrap())
    {
        let attrs = r["resource"]["attributes"].as_array_mut().unwrap();
        attrs.extend(
            p["resource"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| json!({ "key": k, "value": { "stringValue": v } })),
        );
        for scope in r
            .get_mut("scopeMetrics")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            for m in scope["metrics"].as_array_mut().unwrap() {
                let startup = m["name"].as_str().unwrap().starts_with("expo.app_startup.");
                for a in m["gauge"]["dataPoints"][0]["attributes"]
                    .as_array_mut()
                    .unwrap()
                {
                    let Some(text) = a["value"]["stringValue"]
                        .as_str()
                        .filter(|_| a["key"] == "expo.custom_params")
                    else {
                        continue;
                    };
                    let mut params: Map<String, Value> = serde_json::from_str(text).unwrap();
                    let extra = if startup {
                        &p["startupParams"]
                    } else if params["isAppLaunch"] == true {
                        &p["launchNavigationParams"]
                    } else {
                        &Value::Null
                    };
                    params.extend(extra.as_object().cloned().unwrap_or_default());
                    a["value"]["stringValue"] = Value::String(Value::Object(params).to_string());
                }
            }
        }
    }
    normalize(body, platform)
}

#[test]
fn the_linux_service_sends_wire_json_s_bodies() {
    let f = fixture();
    let dir = std::env::temp_dir().join(format!("exact-observe-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("eas-client-id"), f["clientId"].as_str().unwrap()).unwrap();
    let config = json!({ "environment": f["environment"] });
    let app = &f["app"];
    let mut s = Service::new(
        config.as_object().unwrap().clone(),
        app,
        false,
        dir.clone(),
        f["session"].as_str().unwrap().into(),
        f["sessionStart"].as_f64().unwrap(),
    );
    s.device = || fixture()["device"].as_object().unwrap().clone();
    for e in f["events"].as_array().unwrap() {
        s.event(e);
    }
    let _ = std::fs::remove_dir_all(&dir);
    for (signal, rows) in [("metrics", &s.metrics), ("logs", &s.logs)] {
        assert_eq!(
            normalize(s.body(signal, rows), "linux"),
            expected(signal, "linux"),
            "{signal} differ from modules/observe/tests/wire.json"
        );
    }
}
