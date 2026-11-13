use super::*;

const GRANTS: &str = "net.fetch https://api.example\nsecret.keep session";

fn entry(value: Value) -> Result<Value, String> {
    check(vec![("feed".into(), value)], Some(GRANTS))
}

#[test]
fn every_refusal_names_the_entry_and_field() {
    let cases = [
        (json!({"url":"/relative"}), "url"),
        (json!({"url":"file:///path"}), "url"),
        (json!({"url":"https://name:password@api.example"}), "url"),
        (json!({"url":"https://@api.example"}), "url"),
        (json!({"url":"https://api.example/#"}), "url"),
        (json!({"url":"https://elsewhere.example"}), "url"),
        (
            json!({"url":"https://api.example", "method":"bad method"}),
            "method",
        ),
        (json!({"url":"https://api.example", "body":""}), "body"),
        (
            json!({"url":"https://api.example", "method":"HEAD", "body":"x"}),
            "body",
        ),
        (
            json!({"url":"https://api.example", "method":"POST", "body":"é".repeat(32769)}),
            "body",
        ),
        (
            json!({"url":"https://api.example", "timeoutMs":0}),
            "timeoutMs",
        ),
        (
            json!({"url":"https://api.example", "timeoutMs":1.5}),
            "timeoutMs",
        ),
        (
            json!({"url":"https://api.example", "timeoutMs":3600001}),
            "timeoutMs",
        ),
        (
            json!({"url":"https://api.example", "headers":{"bad name":"x"}}),
            "headers.bad name",
        ),
        (
            json!({"url":"https://api.example", "headers":{"Host":"x"}}),
            "headers.Host",
        ),
        (
            json!({"url":"https://api.example", "headers":{"Accept":"x","accept":"y"}}),
            "headers.accept",
        ),
        (json!({"url":"https://api.example/{secret:other}"}), "url"),
        (
            json!({"url":"https://api.example", "headers":{"Authorization":"{secret:other}"}}),
            "headers.Authorization",
        ),
        (
            json!({"url":"https://api.example", "method":"POST", "body":"{secret:other}"}),
            "body",
        ),
        (json!({"url":"https://api.example/{secret:}"}), "url"),
        (json!({"url":"https://api.example/{secret:session"}), "url"),
        (json!({"url":"https://api.example/{literal}"}), "url"),
    ];
    for (value, field) in cases {
        let error = entry(value).unwrap_err();
        assert!(
            error.starts_with(&format!("preload.feed.{field}:")),
            "{error}"
        );
    }
    let nine = (0..9)
        .map(|i| (format!("request-{i}"), json!({"url":"https://api.example"})))
        .collect();
    assert!(check(nine, Some(GRANTS))
        .unwrap_err()
        .starts_with("preload.request-8.name:"));
    assert!(check(
        vec![("Wrong".into(), json!({"url":"https://api.example"}))],
        Some(GRANTS)
    )
    .unwrap_err()
    .starts_with("preload.Wrong.name:"));
}

#[test]
fn strings_parse_once_with_escaped_braces_and_a_unique_secret_set() {
    let parsed = entry(json!({"url":"https://api.example/{secret:session}", "method":"post", "headers":{"Authorization":"Bearer {secret:session}"}, "body":"{{{secret:session}", "timeoutMs":3600000,"when":"foreground"})).unwrap();
    assert_eq!(
        parsed[0]["url"],
        json!([["literal", "https://api.example/"], ["secret", "session"]])
    );
    assert_eq!(
        parsed[0]["headers"][0][1],
        json!([["literal", "Bearer "], ["secret", "session"]])
    );
    assert_eq!(
        parsed[0]["body"],
        json!([["literal", "{"], ["secret", "session"]])
    );
    assert_eq!(parsed[0]["secrets"], json!(["session"]));
    assert_eq!(parsed[0]["method"], "POST");
    assert_eq!(parsed[0]["when"], "foreground");
    assert_eq!(parsed[0]["timeoutMs"], 3600000);
    let defaults = entry(json!({"url":"https://api.example"})).unwrap();
    assert_eq!(defaults[0]["method"], "GET");
    assert_eq!(defaults[0]["when"], "start");
    assert!(defaults[0]["body"].is_null());
    assert!(defaults[0]["timeoutMs"].is_null());
}

#[test]
fn the_manifest_order_survives_the_sorted_json_map() {
    let text = r#"{"preload":{"z-last":{"url":"https://api.example/z"},"a-first":{"url":"https://api.example/a"}}}"#;
    let manifest: ManifestEntries = serde_json::from_str(text).unwrap();
    let parsed = check(manifest.preload.0, Some(GRANTS)).unwrap();
    assert_eq!(parsed[0]["name"], "z-last");
    assert_eq!(parsed[1]["name"], "a-first");
    let code = contract::preload_expression(&parsed);
    assert!(code.find("z-last").unwrap() < code.find("a-first").unwrap());
}

#[test]
fn fetch_and_preloads_use_the_same_forbidden_headers() {
    for name in [
        "host",
        "connection",
        "content-length",
        "transfer-encoding",
        "upgrade",
        "keep-alive",
        "proxy-connection",
        "te",
        "trailer",
    ] {
        assert!(
            entry(json!({"url":"https://api.example","headers":{name:"x"}}))
                .unwrap_err()
                .contains(&format!("preload.feed.headers.{name}:"))
        );
    }
}

#[test]
fn header_values_must_be_field_values_before_they_are_normalized() {
    for value in [
        "ok\nX-Other: yes",
        "bad\rvalue",
        "nul\0byte",
        "\nvalue",
        "value\r",
        " leading",
        "trailing ",
        "\tleading",
        "trailing\t",
        "wide\u{100}",
        "Bearer {secret:session}\n",
    ] {
        let error =
            entry(json!({"url":"https://api.example", "headers":{"X-Note":value}})).unwrap_err();
        assert!(error.starts_with("preload.feed.headers.X-Note:"), "{error}");
    }
    for value in [
        "",
        "has space\tinside",
        "Bearer {secret:session}",
        "byte\u{ff}",
    ] {
        assert!(entry(json!({"url":"https://api.example", "headers":{"X-Note":value}})).is_ok());
    }
}

#[test]
fn repeated_header_names_are_refused_before_json_can_collapse_them() {
    for headers in [
        r#""Accept":"one","Accept":"two""#,
        r#""Accept":"one","accept":"two""#,
    ] {
        let text = format!(
            r#"{{"preload":{{"feed":{{"url":"https://api.example","headers":{{{headers}}}}}}}}}"#
        );
        let error = serde_json::from_str::<ManifestEntries>(&text)
            .err()
            .unwrap()
            .to_string();
        assert!(error.starts_with("preload.feed.headers."), "{error}");
        assert!(error.contains("repeated header ignoring case"), "{error}");
    }
}

#[test]
fn malformed_header_objects_name_the_entry_and_field() {
    let text = r#"{"preload":{"feed":{"url":"https://api.example","headers":[]}}}"#;
    let error = serde_json::from_str::<ManifestEntries>(text)
        .err()
        .unwrap()
        .to_string();
    assert!(error.starts_with("preload.feed.headers:"), "{error}");
}

#[test]
fn unknown_grants_defer_permission_checks_but_known_empty_grants_refuse() {
    let entries = vec![(
        "feed".into(),
        json!({"url":"https://api.example/{secret:session}"}),
    )];
    assert!(check(entries.clone(), None).is_ok());
    assert!(check(entries.clone(), Some(""))
        .unwrap_err()
        .starts_with("preload.feed.url: secret session"));
    assert!(check(entries.clone(), Some("secret.keep session"))
        .unwrap_err()
        .starts_with("preload.feed.url: origin"));
    assert!(check(entries, Some(GRANTS)).is_ok());
}

#[test]
fn integral_deadlines_match_the_manifest_schema() {
    for timeout in [json!(1), json!(1.0), json!(3600000.0)] {
        let parsed = entry(json!({"url":"https://api.example","timeoutMs":timeout})).unwrap();
        assert_eq!(
            parsed[0]["timeoutMs"].as_u64(),
            Some(timeout.as_f64().unwrap() as u64)
        );
    }
}

#[test]
fn secret_references_before_the_url_path_are_refused_under_wildcard_grants() {
    for url in [
        "https://{secret:session}.example.com/feed",
        "{secret:session}://api.example/feed",
        "https://api.example:{secret:session}/feed",
        "https://{secret:session}@api.example/feed",
        "https:{secret:session}.example.com/feed",
        "https:\n//{secret:session}.example.com/feed",
        " \thttps://{secret:session}.example.com/feed",
    ] {
        let error = check(vec![("feed".into(), json!({"url":url}))],
            Some("net.fetch https://*.example.com\nnet.fetch https://api.example\nsecret.keep session")).unwrap_err();
        assert!(error.starts_with("preload.feed.url:"), "{error}");
        assert!(
            error.contains("secret references are allowed only in the URL path or query"),
            "{error}"
        );
    }
    assert!(entry(json!({"url":"https://api.example/feed?tenant={secret:session}"})).is_ok());
}
