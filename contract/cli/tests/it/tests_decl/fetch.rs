#[test]
fn authored_http_responses_are_launch_facts_and_steps() {
    let tests = contract::tests(
        r#"answer fetch "https://api.example.com/" with 200 {"text":"Stubbed greeting"}
test "inherited"
  expect text "greeting" == "Stubbed greeting"
test "overridden"
  fail fetch "https://api.example.com/"
  clock data
  answer fetch "https://api.example.com/" with 503 "unavailable"
  reload
"#,
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&contract::tests_json(&tests)).unwrap();
    assert_eq!(json[0]["steps"][0]["op"], "answer-fetch");
    assert_eq!(json[0]["steps"][0]["response"]["status"], 200);
    assert_eq!(json[1]["steps"][0]["op"], "fail-fetch");
    assert_eq!(json[1]["steps"][2]["op"], "answer-fetch");
    assert!(contract::tests(
        "answer fetch \"x\" with 200\nfail fetch \"x\"\ntest \"t\"\n  clock data\n"
    )
    .is_err());
}

#[test]
fn a_test_overrides_only_the_http_method_it_names() {
    let source = r#"answer fetch "/goal" method GET with 200 "old"
answer fetch "/goal" method PUT with 200 "saved"
test "new read"
  answer fetch "/goal" method GET with 200 "current"
  clock data
"#;
    let tests = contract::tests(source).unwrap();
    let json: serde_json::Value = serde_json::from_str(&contract::tests_json(&tests)).unwrap();
    let steps = json[0]["steps"].as_array().unwrap();
    let answers: Vec<_> = steps
        .iter()
        .filter(|step| step["op"] == "answer-fetch")
        .map(|step| {
            (
                step["method"].as_str().unwrap(),
                step["response"]["body"].as_str().unwrap(),
            )
        })
        .collect();
    assert!(answers.contains(&("GET", "current")));
    assert!(answers.contains(&("PUT", "saved")));
    assert_eq!(answers.len(), 2);
}

#[test]
fn a_response_delay_uses_whole_milliseconds_after_optional_headers_and_body() {
    let source = r#"answer fetch "/old" with 200 header "x-why" "saved" {"value":10} after 200
answer fetch "/empty" with 204 after 1ms
test "wait"
  clock +200
"#;
    let tests = contract::tests(source).unwrap();
    let json: serde_json::Value = serde_json::from_str(&contract::tests_json(&tests)).unwrap();
    assert_eq!(json[0]["steps"][0]["response"]["afterMs"], 200);
    assert_eq!(json[0]["steps"][1]["response"]["afterMs"], 1);
    for delay in ["-1", "0.5", "4294967296"] {
        let source =
            format!("answer fetch \"/x\" with 200 after {delay}\ntest \"t\"\n  clock data\n");
        assert!(contract::tests(&source).is_err(), "{delay}");
    }
}
