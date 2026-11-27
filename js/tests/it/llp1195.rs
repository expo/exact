//! LLP 1195.000 Stage 1 integration tests on Hermes + Runner + Bake:
//! D1 (process shim, WebSocket/XHR/EventSource undefined),
//! D2 (virtualized clock, isolated --seed Math.random, module-sticky & mixed-owner
//! transitive bake taint, UTC bake pinning, set_time re-ask, no reply-commit loop,
//! cross-epoch carry exclusion, Q2 warn-once),
//! D5 P0a (Event/EventTarget/CustomEvent, Blob, File, FormData, Request, Response.blob/bytes),
//! D5 P0b (crypto.subtle: HMAC, AES-GCM, PBKDF2, HKDF, Ed25519, RSASSA-PKCS1-v1_5 verify, ECDSA verify).
#![cfg(exact_js_engine)]

use exact_data::crypto::{generate_p256, sign_es256};
use exact_data::Mixed;
use exact_js::Module;
use exact_kernel::Kernel;
use exact_plan::{Plan, Value};
use exact_runner::{Answer, DataError, DataSource, Outcome, Response, Runner, Store};

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/llp1195.hbc"));
const APP: &str = "test.llp1195";
const GRANTS: &str = "net.fetch https://fixture.exact.test\n";

// 2048-bit RSASSA-PKCS1-v1_5 SHA-256 test vector generated via WebCrypto over "llp-1195-rsa-vector".
const RSA_SPKI_HEX: &str = "30820122300d06092a864886f70d01010105000382010f003082010a0282010100bb5149baf57af68e065516e7f2977fd35fb6e1f3c31c8ad6e0b2e09d878bd8f103d083000ad57e5f409e0098047f75d7a8aa0566105be115bb06ee0584c2df6eb09d9ae9835b5459d6294ab712f0ea398d3439a8a038513802470ff0d6c1b2b8eb3fa4799d7b16a7867aa5214aa4ce51bb235735487dbf0db727f30425503ff4cf94b2ca232834492d3b962f30dc894886a5f643c02048de2a697d3c9ff0214402161a8695bb9172b940ed65f4340883e2367a613a49f8b0f0e12591f7f572f075ce10cd9a95e3a8e20cdbffe45a474f4405a3472f9fbf2d0a37361fed2ca082f41a85fee2b283b2a3790bb51b3cc7862fdf5ffb855cf2de0015ded60b6c95070203010001";
const RSA_JWK_JSON: &str = r#"{"kty":"RSA","n":"u1FJuvV69o4GVRbn8pd_01-24fPDHIrW4LLgnYeL2PED0IMACtV-X0CeAJgEf3XXqKoFZhBb4RW7Bu4FhMLfbrCdmumDW1RZ1ilKtxLw6jmNNDmooDhROAJHD_DWwbK46z-keZ17FqeGeqUhSqTOUbsjVzVIfb8NtyfzBCVQP_TPlLLKIyg0SS07li8w3IlIhqX2Q8AgSN4qaX08n_AhRAIWGoaVu5FyuUDtZfQ0CIPiNnphOkn4sPDhJZH39XLwdc4QzZqV46jiDNv_5FpHT0QFo0cvn78tCjc2H-0soIL0GoX-4rKDsqN5C7UbPMeGL99f-4Vc8t4AFd7WC2yVBw","e":"AQAB","alg":"RS256"}"#;
const RSA_SIG_HEX: &str = "0ff64158291cb231ba887f54cbada4b4cb88e7dacf52dc7ac7bcd1b2ec5629310d6f4f30353552f4942e9ce951d74cd379eaa670e4cad24de137378667e5c6c419e649e4d2a9bf25594de205331a2590917026151e6c1b84e7dad1c13766a9bef881ed76f1d155fa3edb348ac16e199b4ad8329590632337ba36f8cfcbc4fc207d0755c008541175d92187a68285eb8a10bb1f5baf500c794f2db3ee4494323edfb5505b8e289d34a63303be918dc6f9c7078879278cf0e1190d90c2d79adf125d403c3cda5fc3ae0e341f6295c2ac01e3884cf4cf372a3135c669b4c8f31de7148222db8cfce5189e07c0136b5acf32c7a4cf40068ba8423ec07bcdd97112ed";

const ALL_SRC: &str = r#"
component App
  mutation out as shape string
  action a1
    send out = d1Globals()
  action a2
    send out = clockNow()
  action a3
    send out = dateEpochUtc()
  action a4
    send out = randomSeq()
  action a5
    send out = uuidThenRandom()
  action a6
    send out = eventsTest()
  action a7
    send out = blobFileTest()
  action a8
    send out = formDataTest()
  action a9
    send out = requestResponseTest()
  action a10
    send out = cryptoHmac()
  action a11
    send out = cryptoAesGcm()
  action a12
    send out = cryptoPbkdf2()
  action a13
    send out = cryptoHkdf()
  action a14
    send out = cryptoEd25519()
  action a15
    send out = cryptoRsaVerify("", "", "")
  action a16
    send out = cryptoEcdsaVerify("", "", "")
  action a17
    send out = cryptoGenKeys()
  view
    text "ok"
"#;

fn all_plan() -> Plan {
    contract::compile(ALL_SRC).expect("llp1195 all-sources plan compiles")
}

fn fresh_module(plan: &Plan) -> Module {
    let mut m = Module::loaded(HBC.to_vec(), APP, GRANTS).expect("llp1195 fixture loads");
    m.set_budget_ms(f64::INFINITY);
    m.bind(plan);
    m
}

fn seeded_module(plan: &Plan, seed: u64) -> Module {
    let mut m = Module::new(HBC.to_vec(), APP, GRANTS).with_agent_seed(Some(seed));
    m.set_budget_ms(f64::INFINITY);
    m.load().expect("seeded llp1195 fixture loads");
    m.bind(plan);
    m
}

fn text(answer: Answer) -> String {
    match answer {
        Answer::Now(v) => v.as_str().expect("string answer").to_string(),
        Answer::Later(_) => panic!("expected Answer::Now"),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn d1_minimal_frozen_process_and_undefined_browser_io() {
    let plan = all_plan();
    let mut m = fresh_module(&plan);
    let mut store = Store::new(GRANTS, vec![]);
    let out: serde_json::Value =
        serde_json::from_str(&text(m.answer(&mut store, "d1Globals", &[]).unwrap())).unwrap();
    assert_eq!(out["nodeEnv"], "production");
    assert_eq!(out["frozen"], true);
    assert_eq!(out["envFrozen"], true);
    assert_eq!(out["hasCwd"], "undefined");
    assert_eq!(out["ws"], "undefined");
    assert_eq!(out["xhr"], "undefined");
    assert_eq!(out["es"], "undefined");
}

#[test]
fn d2_virtual_clock_and_isolated_seeded_random_substream() {
    let plan = all_plan();
    let mut m1 = seeded_module(&plan, 7);
    let mut m2 = seeded_module(&plan, 7);
    let mut s1 = Store::new(GRANTS, vec![]);
    let mut s2 = Store::new(GRANTS, vec![]);

    // Deterministic virtual clock follows Store epoch_at_zero + clock_ms.
    s1.set_epoch_at_zero(1_767_225_600_000.0);
    s1.set_clock_ms(2_500.0);
    let now: serde_json::Value =
        serde_json::from_str(&text(m1.answer(&mut s1, "clockNow", &[]).unwrap())).unwrap();
    assert_eq!(now["now"], 1_767_225_602_500.0);
    assert_eq!(now["newDate"], 1_767_225_602_500.0);
    assert_eq!(now["perf"], 2_500.0);
    assert_eq!(now["intl"], "01/01/2026");
    assert!(s1.clock_reads() > 0);
    assert_eq!(m1.ambient_taint(""), (true, false));

    // Seeded Math.random() repeats bit-for-bit across modules with the same seed.
    let r1: Vec<f64> =
        serde_json::from_str(&text(m1.answer(&mut s1, "randomSeq", &[]).unwrap())).unwrap();
    let r2: Vec<f64> =
        serde_json::from_str(&text(m2.answer(&mut s2, "randomSeq", &[]).unwrap())).unwrap();
    assert_eq!(r1, r2);
    assert!(r1.iter().all(|&x| (0.0..1.0).contains(&x)));
    assert_eq!(m1.ambient_taint(""), (true, true));

    // Math.random() draws from its own "ts-random" substream and never shifts crypto.randomUUID().
    let mut m_iso = seeded_module(&plan, 1);
    let mut s_iso = Store::new(GRANTS, vec![]);
    let combo: serde_json::Value = serde_json::from_str(&text(
        m_iso.answer(&mut s_iso, "uuidThenRandom", &[]).unwrap(),
    ))
    .unwrap();
    assert_eq!(combo["u1"], "deb201fb-035c-4c32-bbbf-3da08991a485");
    // Expected second UUID directly from the "typescript" stream without any Math.random() drift.
    let mut ts = exact_data::crypto::AgentStream::new(1, "typescript");
    let mut bytes = [0u8; 32];
    ts.fill(&mut bytes);
    let b = &mut bytes[16..32];
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let expected_u2 = format!(
        "{}-{}-{}-{}-{}",
        hex_encode(&b[0..4]),
        hex_encode(&b[4..6]),
        hex_encode(&b[6..8]),
        hex_encode(&b[8..10]),
        hex_encode(&b[10..16]),
    );
    assert_eq!(combo["u2"], expected_u2);
}

#[test]
fn d2_lazy_cache_and_mixed_js_to_rust_transitive_bake_taint() {
    struct RustEcho;
    impl DataSource for RustEcho {
        fn app_id(&self) -> &str {
            APP
        }
        fn query(&mut self, name: &str, args: &[Value]) -> Result<Value, DataError> {
            match name {
                "rustFromJs" => Ok(Value::str(&format!(
                    "rust:{}",
                    args.first().and_then(Value::as_str).unwrap_or("")
                ))),
                "rustPure" => Ok(Value::str("rust-pure")),
                _ => Err(DataError::UnknownSource(name.into())),
            }
        }
    }

    let plan = contract::compile(
        r#"
component App
  resource seeded = lazyCacheSeed() as shape string
  resource lazy = lazyCacheRead() as shape string
  resource rustDerived = rustFromJs(seeded) as shape string
  resource rustClean = rustPure() as shape string
  view
    column
      text seeded
      text lazy
      text rustDerived
      text rustClean
"#,
    )
    .unwrap();
    let mut js = Module::inspect(HBC.to_vec()).unwrap();
    js.set_budget_ms(f64::INFINITY);
    js.bind(&plan);
    let mixed = Mixed::new(
        js,
        RustEcho,
        &["lazyCacheSeed", "lazyCacheRead"],
        &["rustFromJs", "rustPure"],
    )
    .unwrap();
    let baked = contract::bake(plan, mixed).unwrap();
    let row = |name: &str| {
        baked
            .resources
            .iter()
            .find(|r| baked.str(r.name) == name)
            .unwrap()
            .clone()
    };
    assert_eq!(
        row("seeded").initial.len,
        0,
        "ambient clock resource not baked"
    );
    assert!(
        !row("seeded").reader,
        "ambient clock reader stays distinct from store-contents reader (LLP 1195.000 D2.2)"
    );
    assert_eq!(
        row("lazy").initial.len,
        0,
        "module-sticky clock taint excludes second JS resource reading cached timestamp"
    );
    assert_eq!(
        row("rustDerived").initial.len,
        0,
        "transitive taint excludes Rust resource fed by tainted JS resource"
    );
    assert!(
        row("rustClean").initial.len > 0 && !row("rustClean").reader,
        "unrelated Rust resource stays baked"
    );
}

#[cfg(unix)]
#[test]
fn d2_bake_pins_utc_under_non_utc_host_tz() {
    const CHILD: &str = "EXACT_TEST_LLP1195_UTC_BAKE";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "llp1195::d2_bake_pins_utc_under_non_utc_host_tz",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("TZ", "America/Los_Angeles")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let plan = contract::compile(
        "component App\n  resource stamp = dateEpochUtc() as shape string\n  view\n    text stamp\n",
    )
    .unwrap();
    let mut baking = Module::inspect(HBC.to_vec()).unwrap();
    baking.set_budget_ms(f64::INFINITY);
    baking.bind(&plan);
    let baked = contract::bake(plan, baking).unwrap();
    let r = Runner::boot(
        baked,
        fresh_module(&contract::compile("component App\n  view\n    text \"ok\"\n").unwrap()),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let val = r.resource("stamp").and_then(Value::as_str).unwrap();
    assert!(
        val.starts_with("0/0/") && val.contains("00:00:00"),
        "expected UTC-pinned baked date under TZ=America/Los_Angeles, got: {val}"
    );
}

#[test]
fn d2_set_time_reask_no_reply_loop_carry_exclusion_and_warn_once() {
    let plan = contract::compile(
        r#"
component App
  resource now = clockNow() as shape string
  resource fetched = clockFetch() as shape string
  mutation mutated as shape string
  action refreshNow
    refresh now
  action doMutate
    send mutated = clockMutate()
  view
    column
      text now
      text fetched
"#,
    )
    .unwrap();
    let m = fresh_module(&plan);
    let mut r = Runner::boot(
        plan.clone(),
        m,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();

    // Answer the initial clockFetch; completing it must NOT auto-re-ask clockFetch in a loop.
    let reqs = r.take_requests();
    assert_eq!(reqs.len(), 1);
    r.fulfill(
        reqs[0].ticket,
        Outcome::Response(Response {
            status: 200,
            headers: vec![],
            body: b"pong".to_vec(),
        }),
    )
    .unwrap();
    assert!(
        r.take_requests().is_empty(),
        "reply commit must not auto-re-ask a clock-reading resource"
    );

    // Q2: warn-once per app resource; refreshNow and doMutate must not add duplicates.
    r.act("refreshNow", vec![]).unwrap();
    r.act("doMutate", vec![]).unwrap();
    let warnings: Vec<String> = r
        .journal()
        .filter(|line| line.contains("[ambient-clock]"))
        .map(str::to_string)
        .collect();
    assert_eq!(
        warnings
            .iter()
            .filter(|w| w.contains("resource `now`"))
            .count(),
        1,
        "warn-once for resource `now`: {warnings:?}"
    );
    assert_eq!(
        warnings.iter().filter(|w| w.contains("mutated")).count(),
        0,
        "mutations must not emit [ambient-clock]: {warnings:?}"
    );

    // Cross-epoch carry exclusion: same epoch carries; epoch shift >= 1000 ms drops carried clock_reader.
    let mut same_epoch = r.carry();
    assert!(
        same_epoch
            .clock_readers
            .iter()
            .any(|(n, s)| n == "now" && s == "clockNow"),
        "carry must record clock_readers: {:?}",
        same_epoch.clock_readers
    );
    // Stamp a distinctive sentinel into carried `now` so we can prove carry vs fresh re-ask.
    for (n, _, _, val) in &mut same_epoch.resources {
        if n == "now" {
            *val = Value::str("carried-sentinel");
        }
    }
    let r_same = Runner::boot_carrying(
        plan.clone(),
        fresh_module(&plan),
        Kernel::with_monospace(),
        &same_epoch,
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(
        r_same.resource("now"),
        Some(&Value::str("carried-sentinel")),
        "same epoch carries clock-reading resource"
    );
    let mut shifted = same_epoch;
    shifted.epoch_at_zero += 5_000.0;
    let r_shifted = Runner::boot_carrying(
        plan.clone(),
        fresh_module(&plan),
        Kernel::with_monospace(),
        &shifted,
        Default::default(),
        "/",
    )
    .unwrap();
    assert_ne!(
        r_shifted.resource("now"),
        Some(&Value::str("carried-sentinel")),
        "cross-epoch carry (>= 1000 ms) must drop carried clock-reading answer and re-ask"
    );

    // set_time re-asks clock-reading resources with the new epoch.
    let before: serde_json::Value =
        serde_json::from_str(r.resource("now").and_then(Value::as_str).unwrap()).unwrap();
    r.set_time(1_767_225_600_000.0, 0.0).unwrap();
    let after: serde_json::Value =
        serde_json::from_str(r.resource("now").and_then(Value::as_str).unwrap()).unwrap();
    assert_ne!(before["now"], after["now"]);
    assert_eq!(after["now"], 1_767_225_600_000.0);
    // Drain the re-asked clockFetch.
    let reasked = r.take_requests();
    assert_eq!(reasked.len(), 1);
    r.fulfill(
        reasked[0].ticket,
        Outcome::Response(Response {
            status: 200,
            headers: vec![],
            body: b"pong2".to_vec(),
        }),
    )
    .unwrap();
}

#[test]
fn d5_p0a_events_blob_file_formdata_request_and_response() {
    let plan = all_plan();
    let mut m = fresh_module(&plan);
    let mut store = Store::new(GRANTS, vec![]);
    store.set_epoch_at_zero(1_767_225_600_000.0);
    store.set_clock_ms(500.0);

    // Event / EventTarget / CustomEvent
    let ev: serde_json::Value =
        serde_json::from_str(&text(m.answer(&mut store, "eventsTest", &[]).unwrap())).unwrap();
    assert_eq!(ev["calls"], 1);
    assert_eq!(ev["detailN"], 7);
    assert_eq!(ev["dispatched"], false);
    assert_eq!(ev["defaultPrevented"], true);
    assert_eq!(ev["type"], "ping");

    // Blob & File (virtualized lastModified + structuredClone)
    let bf: serde_json::Value =
        serde_json::from_str(&text(m.answer(&mut store, "blobFileTest", &[]).unwrap())).unwrap();
    assert_eq!(bf["size"], 11);
    assert_eq!(bf["type"], "text/plain");
    assert_eq!(bf["text"], "hello world");
    assert_eq!(bf["sliceText"], "world");
    assert_eq!(bf["abLen"], 11);
    assert_eq!(bf["bytesLen"], 11);
    assert_eq!(bf["streamText"], "hello world");
    assert_eq!(bf["clonedBlobText"], "hello world");
    assert_eq!(bf["fileName"], "note.txt");
    assert_eq!(bf["fileType"], "text/plain");
    assert_eq!(bf["fileLastModifiedMatchesNow"], true);
    assert_eq!(bf["clonedFileName"], "note.txt");
    assert_eq!(bf["clonedFileText"], "abc");

    // FormData + RFC 7578 multipart fetch upload
    let Answer::Later(upload_req) = m.answer(&mut store, "formDataTest", &[]).unwrap() else {
        panic!("formDataTest should issue a POST fetch");
    };
    let ct = upload_req
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    assert!(
        ct.starts_with("multipart/form-data; boundary="),
        "expected multipart Content-Type, got: {ct}"
    );
    let body_text = String::from_utf8_lossy(&upload_req.body);
    assert!(body_text.contains("name=\"field\"\r\n\r\nalpha"));
    assert!(body_text.contains("name=\"doc\"; filename=\"hello.txt\""));
    assert!(body_text.contains("hello-file"));
    let fd: serde_json::Value = serde_json::from_str(&text(
        m.parse(
            &mut store,
            "formDataTest",
            &[],
            Outcome::Response(Response {
                status: 201,
                headers: vec![],
                body: b"stored".to_vec(),
            }),
        )
        .unwrap(),
    ))
    .unwrap();
    assert_eq!(fd["getField"], "alpha");
    assert_eq!(fd["getAllField"], serde_json::json!(["alpha", "beta"]));
    assert_eq!(fd["hasDrop"], false);
    assert_eq!(fd["getSingle"], "two");
    assert_eq!(fd["localCtHasMultipart"], true);
    assert_eq!(fd["localHasFile"], true);
    assert_eq!(fd["uploadStatus"], 201);
    assert_eq!(fd["uploadAck"], "stored");

    // Request + Response.blob() & Response.bytes()
    let Answer::Later(req_fetch) = m.answer(&mut store, "requestResponseTest", &[]).unwrap() else {
        panic!("requestResponseTest should issue a fetch");
    };
    assert_eq!(req_fetch.method, "POST");
    assert_eq!(req_fetch.body, b"payload-bytes");
    let rr: serde_json::Value = serde_json::from_str(&text(
        m.parse(
            &mut store,
            "requestResponseTest",
            &[],
            Outcome::Response(Response {
                status: 200,
                headers: vec![("content-type".into(), "application/octet-stream".into())],
                body: b"XYZ".to_vec(),
            }),
        )
        .unwrap(),
    ))
    .unwrap();
    assert_eq!(rr["method"], "POST");
    assert_eq!(rr["header"], "stage1");
    assert_eq!(rr["copyText"], "payload-bytes");
    assert_eq!(rr["copyUsed"], true);
    assert_eq!(rr["reqUsedBefore"], false);
    assert_eq!(rr["reqUsedAfter"], true);
    assert_eq!(rr["blobSize"], 3);
    assert_eq!(rr["blobType"], "application/octet-stream");
    assert_eq!(rr["blobText"], "XYZ");
    assert_eq!(rr["bytesHex"], "58595a");
}

#[test]
fn d5_p0b_webcrypto_algorithms_vectors_and_entropy_accounting() {
    let plan = all_plan();
    let mut m = fresh_module(&plan);
    let mut store = Store::new(GRANTS, vec![]);

    // 1. HMAC SHA-256 RFC 4231 TC1 (deterministic: 0 entropy draws)
    let hm: serde_json::Value =
        serde_json::from_str(&text(m.answer(&mut store, "cryptoHmac", &[]).unwrap())).unwrap();
    assert_eq!(
        hm["sig"],
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    assert_eq!(hm["ok"], true);
    assert_eq!(hm["bad"], false);
    assert_eq!(hm["jwkKty"], "oct");
    assert_eq!(hm["jwkAlg"], "HS256");
    assert_eq!(hm["jwkSig"], hm["sig"]);
    assert_eq!(hm["nonExtErr"], "InvalidAccessError");

    // 2. AES-GCM 128 & 256 (deterministic: 0 entropy draws)
    let ae: serde_json::Value =
        serde_json::from_str(&text(m.answer(&mut store, "cryptoAesGcm", &[]).unwrap())).unwrap();
    assert_eq!(ae["roundTrip"], "secret-message-llp1195");
    assert_eq!(ae["roundTrip128"], "aes128");
    assert_eq!(ae["jwkAlg"], "A256GCM");
    assert_eq!(ae["tamperErr"], "OperationError");

    // 3. PBKDF2 SHA-256 RFC 6070 + deriveKey -> AES-GCM
    let pb: serde_json::Value =
        serde_json::from_str(&text(m.answer(&mut store, "cryptoPbkdf2", &[]).unwrap())).unwrap();
    assert_eq!(
        pb["bits"],
        "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
    );
    assert_eq!(pb["derivedKeyMatchesBits"], true);
    assert_eq!(pb["roundTrip"], "derived-ok");

    // 4. HKDF SHA-256 RFC 5869 TC1 + deriveKey -> HMAC
    let hk: serde_json::Value =
        serde_json::from_str(&text(m.answer(&mut store, "cryptoHkdf", &[]).unwrap())).unwrap();
    assert_eq!(
        hk["okm"],
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
    );
    assert_eq!(hk["verified"], true);

    // 5. Ed25519 RFC 8032 TV1 (empty msg) + raw/spki/pkcs8/jwk round-trips
    let ed: serde_json::Value =
        serde_json::from_str(&text(m.answer(&mut store, "cryptoEd25519", &[]).unwrap())).unwrap();
    assert_eq!(
        ed["sig"],
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    );
    assert_eq!(ed["verifyRaw"], true);
    assert_eq!(ed["verifySpki"], true);
    assert_eq!(ed["verifyJwk"], true);
    assert_eq!(ed["verifyTampered"], false);
    assert_eq!(ed["pkcs8RoundTrip"], true);
    assert_eq!(ed["rawRoundTrip"], true);
    assert_eq!(ed["privJwkCrv"], "Ed25519");
    assert_eq!(ed["pubJwkCrv"], "Ed25519");

    // 6. RSASSA-PKCS1-v1_5 SHA-256 verify (SPKI & JWK)
    let rsa: serde_json::Value = serde_json::from_str(&text(
        m.answer(
            &mut store,
            "cryptoRsaVerify",
            &[
                Value::str(RSA_SPKI_HEX),
                Value::str(RSA_JWK_JSON),
                Value::str(RSA_SIG_HEX),
            ],
        )
        .unwrap(),
    ))
    .unwrap();
    assert_eq!(rsa["okSpki"], true);
    assert_eq!(rsa["okJwk"], true);
    assert_eq!(rsa["badSig"], false);
    assert_eq!(rsa["badMsg"], false);
    assert_eq!(rsa["spkiMatches"], true);
    assert_eq!(rsa["jwkKty"], "RSA");
    assert_eq!(rsa["jwkAlg"], "RS256");

    // 7. ECDSA P-256 verify (using a real Rust-generated signature)
    let pair = generate_p256(&Store::new("", []), true).unwrap();
    let ecdsa_sig = sign_es256(&Store::new("", []), &pair.private, b"verify-me").unwrap();
    let ec: serde_json::Value = serde_json::from_str(&text(
        m.answer(
            &mut store,
            "cryptoEcdsaVerify",
            &[
                Value::str(&pair.public.to_jwk().unwrap().to_json()),
                Value::str(&hex_encode(&ecdsa_sig)),
                Value::str("verify-me"),
            ],
        )
        .unwrap(),
    ))
    .unwrap();
    assert_eq!(ec["ok"], true);
    assert_eq!(ec["bad"], false);

    // Every deterministic operation above drew ZERO entropy.
    assert_eq!(
        store.entropy_draws(),
        0,
        "deterministic HMAC, AES-GCM, PBKDF2, HKDF, Ed25519 sign/verify, RSA verify, and ECDSA verify draw zero entropy"
    );

    // 8. generateKey (HMAC + AES-GCM + Ed25519) draws entropy for each generated key.
    let gen: serde_json::Value =
        serde_json::from_str(&text(m.answer(&mut store, "cryptoGenKeys", &[]).unwrap())).unwrap();
    assert_eq!(gen["hmLen"], 64);
    assert_eq!(gen["aeLen"], 32);
    assert_eq!(gen["edOk"], true);
    assert_eq!(store.entropy_draws(), 3);
}
