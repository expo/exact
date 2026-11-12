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
