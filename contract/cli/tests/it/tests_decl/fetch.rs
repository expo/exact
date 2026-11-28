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

#[test]
fn answer_stream_schedules_buffered_replies_inheritance_and_refusals() {
    let source = r#"answer fetch "https://api.test/chat" with 200 {"ok":true}
answer stream "https://api.test/chat" with 200 header "x-req" "1"
  event {"delta":"hi"} name "chunk" id "1"
  event "done" after 250ms
  end after 300
answer stream "https://api.test/bin" method POST with 200
  bytes "ab"
  bytes base64 "AQID" after 100
  fail "dropped mid-stream" after 200
test "inherits both fetch and stream on same prefix"
  clock +250
  pass stream "https://api.test/chat"
test "overrides stream and uses buffered error"
  answer stream "https://api.test/chat" with 503 {"err":"busy"} after 50
  clock data
"#;
    let tests = contract::tests(source).unwrap();
    let json: serde_json::Value = serde_json::from_str(&contract::tests_json(&tests)).unwrap();
    let s0 = json[0]["steps"].as_array().unwrap();
    assert_eq!(s0[0]["op"], "answer-fetch");
    assert_eq!(s0[1]["op"], "answer-stream");
    assert_eq!(s0[1]["stream"]["status"], 200);
    assert_eq!(s0[1]["stream"]["mode"], "events");
    assert_eq!(
        s0[1]["stream"]["headers"]["content-type"],
        "text/event-stream"
    );
    assert_eq!(s0[1]["stream"]["headers"]["x-req"], "1");
    assert_eq!(s0[1]["stream"]["steps"][0]["kind"], "event");
    assert_eq!(s0[1]["stream"]["steps"][0]["data"], "{\"delta\":\"hi\"}");
    assert_eq!(s0[1]["stream"]["steps"][0]["name"], "chunk");
    assert_eq!(s0[1]["stream"]["steps"][0]["id"], "1");
    assert_eq!(s0[1]["stream"]["steps"][0]["afterMs"], 0);
    assert_eq!(s0[1]["stream"]["steps"][1]["afterMs"], 250);
    assert_eq!(s0[1]["stream"]["steps"][2]["kind"], "end");
    assert_eq!(s0[2]["op"], "answer-stream");
    assert_eq!(s0[2]["method"], "POST");
    assert_eq!(s0[2]["stream"]["mode"], "bytes");
    assert_eq!(
        s0[2]["stream"]["headers"]["content-type"],
        "application/octet-stream"
    );
    assert_eq!(s0[2]["stream"]["steps"][0]["base64"], "YWI=");
    assert_eq!(s0[2]["stream"]["steps"][1]["base64"], "AQID");
    assert_eq!(s0[2]["stream"]["steps"][2]["kind"], "fail");
    assert_eq!(s0[2]["stream"]["steps"][2]["message"], "dropped mid-stream");
    assert_eq!(s0[4]["op"], "pass-stream");

    let s1 = json[1]["steps"].as_array().unwrap();
    let chat_stream = s1
        .iter()
        .find(|st| st["op"] == "answer-stream" && st["prefix"] == "https://api.test/chat")
        .unwrap();
    assert_eq!(chat_stream["stream"]["status"], 503);
    assert_eq!(chat_stream["stream"]["body"], "{\"err\":\"busy\"}");
    assert_eq!(chat_stream["stream"]["afterMs"], 50);

    // Refusals: mixed event/bytes, rows after end/fail, non-monotonic after, bad base64,
    // mismatched content-type, explicit HEAD on streaming 2xx, inline body on 200, block on 503,
    // body on 204, redirects.
    for bad in [
        "answer stream \"/s\" with 200\n  event \"a\"\n  bytes \"b\"\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 200\n  end\n  event \"late\"\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 200\n  fail \"x\"\n  end\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 200\n  event \"a\" after 200\n  event \"b\" after 100\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 200\n  bytes base64 \"!!!\"\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 200 header \"content-type\" \"application/json\"\n  event \"a\"\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 200 header \"content-type\" \"text/event-stream\"\n  bytes \"a\"\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" method HEAD with 200\n  event \"a\"\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 200 \"inline\"\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 503 \"err\"\n  event \"a\"\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 204 \"nope\"\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 302\ntest \"t\"\n  clock data\n",
        "answer stream \"/s\" with 200\nanswer stream \"/s\" with 200\ntest \"t\"\n  clock data\n",
    ] {
        assert!(contract::tests(bad).is_err(), "expected refusal for:\n{bad}");
    }
}
