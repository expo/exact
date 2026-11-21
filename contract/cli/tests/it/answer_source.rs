//! LLP 1017 P7: a test's `answer source` line, a launch fact when it leads
//! the test or the file, and a step after.

#[test]
fn authored_source_answers_are_launch_facts_and_steps() {
    let tests = contract::tests(
        r#"answer source pairing with {"phase": "idle"}
test "inherited"
  expect state phase == "idle"
test "overridden"
  answer source pairing with {"phase": "code", "code": "123456"}
  tap "pair"
  answer source pairing with {"phase": "done"}
"#,
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&contract::tests_json(&tests)).unwrap();
    assert_eq!(json[0]["steps"][0]["op"], "answer-source");
    assert_eq!(json[0]["steps"][0]["source"], "pairing");
    assert_eq!(json[0]["steps"][0]["json"], r#"{"phase":"idle"}"#);
    assert_eq!(
        json[1]["steps"].as_array().unwrap().len(),
        3,
        "the test's own answer replaces the file's"
    );
    assert_eq!(json[1]["steps"][2]["json"], r#"{"phase":"done"}"#);
    let later = contract::tests(
        "test \"t\"\n  answer source a with {}\n  clock data\n  answer source a with {}\n  answer source a with {}\n",
    );
    assert!(
        later.is_ok(),
        "a later line is a step, as often as the test needs"
    );
    let duplicate = contract::tests(
        "test \"t\"\n  answer source a with {}\n  answer source a with {}\n  clock data\n",
    );
    assert!(duplicate.is_err(), "two leading lines for one source");
}
