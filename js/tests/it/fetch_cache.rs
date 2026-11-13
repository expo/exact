use super::*;
use exact_runner::CacheMode;

#[test]
fn fetch_carries_every_cache_mode_and_rejects_invalid_values_as_type_errors() {
    let mut module = module();
    module.bind(&contract::compile("component App\n  resource result = cached(\"absent\") as shape string\n  view\n    text result\n").unwrap());
    let mut store = store();
    for (word, mode) in [
        ("absent", CacheMode::Unspecified),
        ("default", CacheMode::Default),
        ("no-store", CacheMode::NoStore),
        ("reload", CacheMode::Reload),
        ("no-cache", CacheMode::NoCache),
        ("force-cache", CacheMode::ForceCache),
        ("only-if-cached", CacheMode::OnlyIfCached),
    ] {
        let request = later(
            module
                .answer(&mut store, "cached", &[Value::str(word)])
                .unwrap(),
        );
        assert_eq!(request.cache, mode, "{word}");
        assert_eq!(
            now(module
                .parse(
                    &mut store,
                    "cached",
                    &[Value::str(word)],
                    Outcome::Response(Response {
                        status: 200,
                        headers: vec![],
                        body: b"reply".to_vec()
                    })
                )
                .unwrap()),
            Value::str("reply")
        );
    }
    for value in [
        Value::str("other"),
        Value::str(""),
        Value::str("DEFAULT"),
        Value::str("number"),
        Value::str("null"),
    ] {
        let answer = now(module.answer(&mut store, "cached", &[value]).unwrap());
        assert!(
            answer
                .as_str()
                .unwrap()
                .starts_with("TypeError: cache must be"),
            "{answer:?}"
        );
        assert_eq!(module.in_flight(), 0);
    }
    assert_eq!(
        now(module
            .answer(&mut store, "cached", &[Value::str("throws")])
            .unwrap()),
        Value::str("Error: cache getter")
    );
}

#[test]
fn fetch_carries_a_preload_name_and_refuses_invalid_names_before_the_door() {
    let mut module = module();
    module.bind(&contract::compile("component App\n  resource result = preloaded(\"absent\") as shape string\n  view\n    text result\n").unwrap());
    let mut store = store();
    for name in ["absent", "feed", "a", "feed-2"] {
        let request = later(
            module
                .answer(&mut store, "preloaded", &[Value::str(name)])
                .unwrap(),
        );
        assert_eq!(
            request.preload.as_deref(),
            (name != "absent").then_some(name)
        );
        assert_eq!(
            now(module
                .parse(
                    &mut store,
                    "preloaded",
                    &[Value::str(name)],
                    Outcome::Response(Response {
                        status: 200,
                        headers: vec![],
                        body: b"reply".to_vec()
                    })
                )
                .unwrap()),
            Value::str("reply")
        );
    }
    for name in [
        "",
        "Feed",
        "2feed",
        "feed_name",
        "feed.name",
        "feed\n",
        "fée",
        "number",
        "null",
    ] {
        let answer = now(module
            .answer(&mut store, "preloaded", &[Value::str(name)])
            .unwrap());
        assert!(
            answer
                .as_str()
                .unwrap()
                .starts_with("TypeError: exactPreload must match"),
            "{answer:?}"
        );
        assert_eq!(module.in_flight(), 0);
    }
    assert_eq!(
        now(module
            .answer(&mut store, "preloaded", &[Value::str("throws")])
            .unwrap()),
        Value::str("Error: preload getter")
    );
}

#[test]
fn fetch_carries_prefetch_and_refuses_non_reads_and_invalid_values() {
    let mut module = module();
    module.bind(&contract::compile("component App\n  resource result = prefetched(\"true\") as shape string\n  view\n    text result\n").unwrap());
    let mut store = store();
    for word in ["absent", "false", "true"] {
        let request = later(
            module
                .answer_for(
                    exact_runner::Target::Resource(0),
                    &mut store,
                    "prefetched",
                    &[Value::str(word)],
                )
                .unwrap(),
        );
        assert_eq!(request.prefetch, word == "true");
        assert_eq!(
            now(module
                .parse_for(
                    exact_runner::Target::Resource(0),
                    &mut store,
                    "prefetched",
                    &[Value::str(word)],
                    Outcome::Response(Response {
                        status: 503,
                        headers: vec![],
                        body: b"reply".to_vec()
                    })
                )
                .unwrap()),
            Value::str("reply")
        );
    }
    for word in ["number", "null", "stream", "file", "independent"] {
        let answer = now(module
            .answer_for(
                exact_runner::Target::Resource(0),
                &mut store,
                "prefetched",
                &[Value::str(word)],
            )
            .unwrap());
        assert!(
            answer
                .as_str()
                .unwrap()
                .starts_with("TypeError: exactPrefetch"),
            "{answer:?}"
        );
        assert_eq!(module.in_flight(), 0);
    }
    assert_eq!(
        now(module
            .answer_for(
                exact_runner::Target::Mutation(0),
                &mut store,
                "prefetched",
                &[Value::str("true")]
            )
            .unwrap()),
        Value::str("TypeError: exactPrefetch requires a plain HTTP read")
    );
    assert_eq!(
        now(module
            .answer_for(
                exact_runner::Target::Resource(0),
                &mut store,
                "prefetched",
                &[Value::str("throws")]
            )
            .unwrap()),
        Value::str("Error: prefetch getter")
    );
}

#[test]
fn a_prefetch_answer_let_go_keeps_no_module_work_or_runner_ticket() {
    for option in ["false", "true"] {
        let src = format!(
            r#"
component App
  state option = "{option}"
  resource result = prefetched(option) as shape string
  action replace
    option = "absent"
  view
    column
      text result
      button press=replace testId="replace"
        text "Replace"
"#
        );
        let mut runner = Runner::boot(
            contract::compile(&src).unwrap(),
            module(),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let first = runner.take_requests();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].request.prefetch, option == "true");
        let key = runner.kernel().find_by_test_id("replace")[0];
        let id = runner.kernel().node_by_key(key).unwrap().id;
        runner.dispatch(id, Event::Press).unwrap();
        let next = runner.take_requests();
        assert_eq!(next.len(), 1);
        assert!(!runner.holds(first[0].ticket));
        assert_eq!(runner.data().in_flight(), 1);
        assert_eq!(runner.data().calls_open(), Some(1));
        runner
            .fulfill(first[0].ticket, response(200, "old"))
            .unwrap();
        assert!(runner.holds(next[0].ticket));
        runner
            .fulfill(next[0].ticket, response(200, "next"))
            .unwrap();
        assert!(runner.pending().is_empty());
        assert_eq!(runner.data().in_flight(), 0);
        assert_eq!(runner.data().calls_open(), Some(0));
    }
}
