use super::*;

#[test]
#[ignore = "async lane: measures a two-megabyte fetch against the source's CPU budget"]
fn a_two_megabyte_body_stays_within_a_source_turn() {
    let mut m = module();
    m.bind(&contract::compile("component App\n  resource result = largeUpload(\"buffer\") as shape string\n  view\n    text result\n").unwrap());
    m.set_budget_ms(100.0);
    let mut s = store();
    let mut expected = vec![0xa5; 2_000_000];
    for (i, byte) in expected[..256].iter_mut().enumerate() {
        *byte = i as u8;
    }
    for kind in ["buffer", "view", "dataview"] {
        let args = [Value::str(kind)];
        let started = std::time::Instant::now();
        let answer = m.answer(&mut s, "largeUpload", &args);
        eprintln!(
            "2,000,000-byte Hermes {kind} request: {:?} wall; 100 ms CPU budget; error: {:?}",
            started.elapsed(),
            answer.as_ref().err()
        );
        let request = later(answer.unwrap());
        assert_eq!(request.body.len(), expected.len(), "{kind}");
        assert!(request.body == expected, "{kind}: bytes changed");
        assert!(request.body_from.is_none());
        assert!(!request
            .headers
            .iter()
            .any(|(name, _)| name == "content-type"));
        assert_eq!(
            now(m
                .parse(&mut s, "largeUpload", &args, response(200, "ok"))
                .unwrap()),
            Value::str("ok")
        );
    }
    assert_eq!(m.overruns(), 0);
}

/// LLP 1069.002 D4: an ArrayBuffer or a view as a fetch's body is sent as
/// its bytes, every value of a byte intact, copied when `fetch` is called.
#[test]
fn a_buffer_source_body_is_sent_as_its_bytes() {
    let mut m = unloaded();
    m.load().unwrap();
    m.bind(&contract::compile("component App\n  resource result = upload(\"view\") as shape string\n  view\n    text result\n").unwrap());
    let mut s = store();
    for (kind, bytes) in [
        ("view", vec![0u8, 255, 128, 10, 13, 0]),
        ("dataview", vec![0, 255, 128, 10, 13, 0]),
        ("buffer", vec![9, 0, 255, 128, 10, 13, 0, 9]),
        ("wide", vec![255, 128, 10, 13]),
        ("one", vec![0]),
        ("two", vec![0, 255]),
        ("emptyBuffer", vec![]),
        ("emptyView", vec![]),
        ("emptyDataView", vec![]),
    ] {
        let args = [Value::str(kind)];
        let request = later(m.answer(&mut s, "upload", &args).unwrap());
        assert_eq!(request.method, "POST");
        assert_eq!(request.body, bytes, "{kind}");
        assert!(request
            .headers
            .contains(&("content-type".into(), "image/png".into())));
        assert_eq!(
            now(m
                .parse(&mut s, "upload", &args, response(200, "ok"))
                .unwrap()),
            Value::str("ok")
        );
    }
}

#[test]
fn concurrent_byte_bodies_are_owned_by_their_fetch() {
    use exact_runner::Target;
    let mut m = module();
    m.bind(&contract::compile("component App\n  resource result = parallelUpload() as shape string\n  view\n    text result\n").unwrap());
    let mut s = store();
    let target = Target::Resource(0);
    let first = later(m.answer_for(target, &mut s, "parallelUpload", &[]).unwrap());
    let mut requests = vec![first];
    requests.extend(m.more_for(target));
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].body, [0, 255]);
    assert_eq!(requests[1].body, [128, 255]);
    assert!(requests[2].body.is_empty());
    assert!(requests
        .windows(2)
        .all(|pair| pair[0].fetch != pair[1].fetch));
    let mut answer = None;
    for i in (0..3).rev() {
        answer = m
            .parse_fetch_for(
                target,
                requests[i].fetch,
                &mut s,
                "parallelUpload",
                &[],
                response(200, ["first", "second", "empty"][i]),
            )
            .unwrap();
        if i != 0 {
            assert!(answer.is_none());
        }
    }
    assert_eq!(now(answer.unwrap()), Value::str("first/second/empty"));
    assert_eq!(m.in_flight(), 0);
}

/// Fetch refuses any body on a GET or HEAD, an empty one too, and a view on
/// a SharedArrayBuffer or a resizable buffer, before any request; a detached
/// buffer is sent as no bytes (Web IDL's copy of a BufferSource).
#[test]
fn a_body_fetch_refuses_is_refused_before_any_request() {
    let mut m = unloaded();
    m.load().unwrap();
    m.bind(&contract::compile("component App\n  resource result = oddUpload(\"get\") as shape string\n  view\n    text result\n").unwrap());
    let mut s = store();
    for (kind, message) in [
        ("get", "a GET request cannot have a body"),
        ("head", "a HEAD request cannot have a body"),
        (
            "shared",
            "data on a SharedArrayBuffer is not a BufferSource",
        ),
        (
            "resizable",
            "data on a resizable ArrayBuffer is not a BufferSource",
        ),
    ] {
        let refused = match m.answer(&mut s, "oddUpload", &[Value::str(kind)]).unwrap() {
            Answer::Now(v) => v,
            Answer::Later(r) => panic!("{kind}: a request for {}", r.url),
        };
        let text = refused.as_str().unwrap_or_default();
        assert!(
            text == "unsupported" || text.contains(message),
            "{kind}: {refused:?}"
        );
        assert_eq!(m.in_flight(), 0, "{kind}");
    }
    let args = [Value::str("detached")];
    match m.answer(&mut s, "oddUpload", &args).unwrap() {
        Answer::Now(v) => assert_eq!(v, Value::str("unsupported")),
        Answer::Later(request) => {
            assert!(request.body.is_empty());
            assert_eq!(
                now(m
                    .parse(&mut s, "oddUpload", &args, response(200, "ok"))
                    .unwrap()),
                Value::str("ok")
            );
        }
    }
}

#[test]
fn a_fetch_body_supplies_its_content_type_unless_headers_name_one() {
    let mut m = module();
    m.bind(&contract::compile("component App\n  resource result = typedUpload(\"string\", \"POST\", \"none\") as shape string\n  view\n    text result\n").unwrap());
    let mut s = store();
    let mut mismatches = Vec::new();
    for (kind, body, inferred) in [
        ("string", &b"hello"[..], Some("text/plain;charset=UTF-8")),
        ("empty", &b""[..], Some("text/plain;charset=UTF-8")),
        (
            "form",
            &b"grant_type=authorization_code&code=a+b"[..],
            Some("application/x-www-form-urlencoded;charset=UTF-8"),
        ),
        (
            "emptyForm",
            &b""[..],
            Some("application/x-www-form-urlencoded;charset=UTF-8"),
        ),
        (
            "formOwnString",
            &b"code=a+b"[..],
            Some("application/x-www-form-urlencoded;charset=UTF-8"),
        ),
        ("view", &b"\0\xff"[..], None),
        ("buffer", &b"\0\xff"[..], None),
        ("dataview", &b"\0\xff"[..], None),
        ("emptyBytes", &b""[..], None),
        ("null", &b""[..], None),
        ("undefined", &b""[..], None),
    ] {
        for method in ["POST", "PUT", "PATCH"] {
            for headers in ["none", "other", "record", "pairs", "headers"] {
                let args = [Value::str(kind), Value::str(method), Value::str(headers)];
                let request = later(m.answer(&mut s, "typedUpload", &args).unwrap());
                assert_eq!(request.method, method);
                assert_eq!(request.body, body, "{kind}");
                let types: Vec<_> = request
                    .headers
                    .iter()
                    .filter(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                    .map(|(_, value)| value.as_str())
                    .collect();
                let expected: Vec<_> = match headers {
                    "none" | "other" => inferred,
                    "headers" => Some(""),
                    _ => Some("application/custom"),
                }
                .into_iter()
                .collect();
                if types != expected {
                    mismatches.push(format!(
                        "{method} {kind} {headers}: {types:?}, expected {expected:?}"
                    ));
                }
                assert_eq!(
                    now(m
                        .parse(&mut s, "typedUpload", &args, response(200, "ok"))
                        .unwrap()),
                    Value::str("ok")
                );
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
