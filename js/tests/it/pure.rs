//! Actual Hermes versus Chrome's standard web utilities.
//! @ref LLP 1027.001 D1 — Unicode and URL semantics survive a language move
#![cfg(exact_js_engine)]
use exact_js::Module;
use exact_runner::DataSource;
use std::process::Command;

#[test]
fn pure_utilities_match_the_web_executor() {
    let script = concat!(env!("OUT_DIR"), "/pure.js");
    let reference = Command::new(std::env::var_os("BUN").unwrap_or_else(|| "bun".into()))
        .args(["--input-type=module", "-e", CHROME_UTILITIES])
        .env("EXACT_PURE_SCRIPT", script)
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
        .output()
        .expect("Bun launches the Chrome utility oracle");
    if let Some(why) = String::from_utf8_lossy(&reference.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("missing chrome: "))
        .map(str::to_owned)
    {
        // EXACT_REQUIRED=1 (the async lane): a missing prerequisite fails (scripts/coverage.mjs).
        assert_ne!(
            std::env::var("EXACT_REQUIRED").as_deref(),
            Ok("1"),
            "missing chrome: {why}"
        );
        eprintln!("pure utility browser sweep unavailable: {why}");
        return;
    }
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let expected: Vec<String> = serde_json::from_slice(&reference.stdout).unwrap();
    let plan = contract::compile("component App\n  resource text = text() as shape string\n  resource url = url() as shape string\n  resource base64 = base64() as shape string\n  resource standard = standard() as shape string\n  resource microtask = microtask() as shape string\n  resource abort = abort() as shape string\n  resource hooks = hooks() as shape string\n  resource intl = intl() as shape string\n  resource abortAny = abortAny() as shape string\n  resource response = response() as shape string\n  resource streams = streams() as shape string\n  view\n    text text\n").unwrap();
    let mut module = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        "",
    )
    .unwrap();
    module.bind(&plan);
    // A formatter per locale and option set is slow to make on Apple's Intl.
    module.set_budget_ms(f64::INFINITY);
    for (source, expected) in [
        "text",
        "url",
        "base64",
        "standard",
        "microtask",
        "abort",
        "hooks",
        "intl",
        "abortAny",
        "response",
        "streams",
    ]
    .into_iter()
    .zip(expected)
    {
        let actual = module.query(source, &[]).unwrap();
        let actual = actual.as_str().unwrap();
        if source == "hooks" {
            assert_eq!(
                actual,
                r#"{"present":false,"own":"undefined","subscribe":"undefined","headersHooks":false}"#,
                "the abort and headers hook handoffs and their functions must be unreachable after load"
            );
        }
        assert_eq!(actual, expected, "{source}");
    }
}

const CHROME_UTILITIES: &str = r#"
import { Cdp } from './scripts/agent.mjs';
import { chromium } from './scripts/agent-launch.mjs';
const { executable: browserExecutable, unavailable: browserMissing } = chromium();
if (browserMissing) { console.log(`missing chrome: ${browserMissing}`); process.exit(0); }
import { spawn } from 'node:child_process';
import { existsSync, readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const profile = mkdtempSync(join(tmpdir(), 'exact-pure-browser-'));
const child = spawn(browserExecutable, [
  '--headless=new', '--no-sandbox', '--remote-debugging-pipe', '--no-first-run',
  '--disable-background-networking', `--user-data-dir=${profile}`, 'about:blank',
], { detached:true, stdio:['ignore','ignore','ignore','pipe','pipe'] });
const cdp = new Cdp(child.stdio[3], child.stdio[4]);
const exited = new Promise(resolve => {
  child.on('error', error => { cdp.fail(String(error)); resolve(); });
  child.on('exit', () => { cdp.fail('Chrome closed'); resolve(); });
});
try {
  const { targetInfos } = await cdp.send('Target.getTargets');
  const { sessionId } = await cdp.send('Target.attachToTarget', {
    targetId:targetInfos.find(target => target.type === 'page').targetId, flatten:true,
  });
  const fixture = readFileSync(process.env.EXACT_PURE_SCRIPT, 'utf8');
  const result = await cdp.send('Runtime.evaluate', {
    expression:fixture + '\n;Promise.all(["text","url","base64","standard","microtask","abort","hooks","intl","abortAny","response","streams"].map(source=>globalThis.exact.answer(source))).then(JSON.stringify)',
    returnByValue:true, awaitPromise:true,
  }, sessionId);
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
  console.log(result.result.value);
} finally {
  if (child.pid && child.exitCode === null && child.signalCode === null) {
    try { process.kill(-child.pid, 'SIGKILL'); } catch {}
  }
  await exited;
  rmSync(profile, { recursive:true, force:true, maxRetries:3, retryDelay:100 });
}
"#;

#[test]
fn readable_stream_bodies_are_collected_by_responses() {
    let mut module = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        "",
    )
    .unwrap();
    module.bind(&contract::compile("component App\n  resource value = streamBody() as shape string\n  view\n    text value\n").unwrap());
    assert_eq!(
        module.query("streamBody", &[]).unwrap().as_str().unwrap(),
        r#"{"viaResponse":"hello","magic":[31,139],"unpacked":"hello"}"#
    );
}

/// Each step of a URLSearchParams iteration once parsed the whole query
/// again, so iterating N pairs parsed N² of them: 2,000 pairs three ways
/// overran a two-second turn. An unchanged query is now parsed once.
#[test]
fn iterating_unchanged_search_params_is_linear_in_them() {
    let mut module = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        "",
    )
    .unwrap();
    module.bind(&contract::compile("component App\n  resource value = searchParamsWork() as shape string\n  view\n    text value\n").unwrap());
    module.set_budget_ms(2_000.0);
    let seen = module.query("searchParamsWork", &[]).unwrap();
    assert_eq!(seen.as_str().unwrap(), "6000");
}

/// Each step of a Headers iteration once copied and sorted the whole list
/// twice, once for the name and once for the value: 2N sorts of N pairs.
/// The sorted order is now kept until a header is added or deleted.
#[test]
fn iterating_unchanged_headers_is_linear_in_them() {
    let mut module = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        "",
    )
    .unwrap();
    module.bind(&contract::compile("component App\n  resource value = headersWork() as shape string\n  view\n    text value\n").unwrap());
    module.set_budget_ms(2_000.0);
    let seen = module.query("headersWork", &[]).unwrap();
    assert_eq!(seen.as_str().unwrap(), "12000");
}

#[test]
fn readable_stream_bodies_and_chunks_use_intrinsic_brands() {
    let mut module = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        "",
    )
    .unwrap();
    module.bind(&contract::compile("component App\n  resource value = streamBodyBrands() as shape string\n  view\n    text value\n").unwrap());
    assert_eq!(
        module
            .query("streamBodyBrands", &[])
            .unwrap()
            .as_str()
            .unwrap(),
        r#"{"chunks":["TypeError","TypeError",[1,2],[3,4]],"bodies":["A",["B","B"],["B","B"]]}"#
    );
}

#[test]
fn readable_stream_request_bodies_refuse_before_a_request_is_sent() {
    use exact_runner::{Answer, Store};
    let grants = "";
    let mut module = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        grants,
    )
    .unwrap();
    module.bind(&contract::compile("component App\n  resource value = streamUpload() as shape string\n  view\n    text value\n").unwrap());
    let mut store = Store::new(grants, Vec::<(String, Vec<u8>)>::new());
    let answer = module.answer(&mut store, "streamUpload", &[]).unwrap();
    let Answer::Now(value) = answer else {
        panic!("a ReadableStream body must be refused, got {answer:?}");
    };
    let results: Vec<String> = serde_json::from_str(value.as_str().unwrap()).unwrap();
    assert_eq!(results.len(), 6);
    for text in results {
        assert!(
            text.starts_with("TypeError: fetch: ReadableStream"),
            "{text}"
        );
        assert!(text.ends_with("locked=false; stringified=false"), "{text}");
    }
}

#[test]
fn large_native_strings_preserve_json_semantics_and_call_ownership() {
    use exact_js::{from_json, to_json, Shape};
    use exact_plan::Value;
    let script = concat!(env!("OUT_DIR"), "/pure.js");
    let declarations = "shape Leaf\n  text: string\nshape Pair\n  first: string\n  second: string\nshape Branch\n  left: Leaf\n  right: Leaf\nshape Branches\n  first: Branch\n  second: Leaf\n  last: Branch[]\n  reads: number\nshape Object\n  first: string\n  second: string\n  aliases: Leaf[]\n  reads: number\n";
    let mut module = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        "",
    )
    .unwrap();
    for (mode, ty) in [
        ("plain", "string"),
        ("boxed", "string"),
        ("object", "Object"),
        ("branches", "Branches"),
        ("nullable", "option<number>[]"),
        ("extra", "Pair"),
        ("missing", "Pair"),
        ("throw", "Pair"),
        ("cycle", "Pair"),
        ("bigint", "Pair"),
        ("lone", "string"),
        ("arrayMethodsHook", "Pair"),
        ("arrayHook", "string"),
        ("private", "string"),
        ("small", "string"),
    ] {
        let plan = contract::compile(&format!(
            "{declarations}\ncomponent App\n  resource value = transfer(\"{mode}\") as shape {ty}\n  view\n    text \"fixture\"\n"
        ))
        .unwrap();
        let shape = Shape::from_plan(&plan, plan.sources[0].ty).unwrap();
        let reference = Command::new(std::env::var_os("BUN").unwrap_or_else(|| "bun".into()))
            .args(["-e", &format!("require({}); process.stdout.write(JSON.stringify(globalThis.exact.answer('transfer',[{}])))", serde_json::to_string(script).unwrap(), serde_json::to_string(mode).unwrap())])
            .output()
            .unwrap();
        let expected = if reference.status.success() {
            serde_json::from_slice(&reference.stdout)
                .map_err(|error| error.to_string())
                .and_then(|json| from_json(&json, &shape))
        } else {
            Err("the standard serializer refused".into())
        };
        module.bind(&plan);
        let actual = module.query("transfer", &[Value::str(mode)]);
        if matches!(mode, "arrayMethodsHook" | "arrayHook") {
            assert!(
                actual.is_err(),
                "{mode}: universal hardening must refuse application mutation of Array intrinsics"
            );
            continue;
        }
        match expected {
            Ok(value) => assert_eq!(
                to_json(
                    &actual.unwrap_or_else(|error| panic!("{mode}: {error:?}")),
                    &shape
                )
                .unwrap(),
                to_json(&value, &shape).unwrap(),
                "{mode}"
            ),
            Err(_) => assert!(actual.is_err(), "{mode} must refuse"),
        }
    }
}

#[test]
fn universal_hardening_prevents_application_envelope_parser_replacement() {
    use exact_plan::Value;
    let plan = contract::compile("component App\n  resource reply = wire(\"\", \"sync\") as shape string\n  resource later = transfer(\"small\") as shape string\n  view\n    text \"fixture\"\n").unwrap();
    let big = "a\\\0é😀\u{2028}\u{2029}".repeat(8192);
    for (wire, mode, _previously_injectable) in [
        (r#"{"tag":0,"value":7,"value":"last"}"#, "sync", Ok("last")),
        (r#"{"value":"first","tag":2,"tag":0}"#, "sync", Ok("first")),
        (
            r#"{"tag":0,"value":"first","value":7}"#,
            "sync",
            Err("expected a string"),
        ),
        (
            r#"{"tag":0,"value":7,"extra":1e400}"#,
            "sync",
            Err("number out of range"),
        ),
        (
            r#"{"tag":0,"value":"\ud800"}"#,
            "sync",
            Err("answered something other than JSON"),
        ),
        (
            r#"{"tag":2,"value":7,"kind":"BadArguments","message":"wire refusal"}"#,
            "sync",
            Err("BadArguments(\"wire refusal\")"),
        ),
        (
            r#"{"tag":2,"value":1e400,"message":"wire refusal"}"#,
            "sync",
            Err("number out of range"),
        ),
        (r#"{"tag":3}"#, "async", Err("a call with no id")),
        (r#"{"value":7,"call":1,"tag":3}"#, "async", Ok("settled")),
        (
            r#"{"tag":3,"call":1,"value":1e400}"#,
            "async",
            Err("number out of range"),
        ),
        (r#"{"tag":1,"call":1}"#, "sync", Err("pending on no ticket")),
        (
            r#"{"tag":1,"call":1,"ticket":123}"#,
            "sync",
            Err("awaits a fetch it never made"),
        ),
        (
            r#"{"value":"missing tag"}"#,
            "sync",
            Err("answered with no tag"),
        ),
        ("[]", "sync", Err("answered with no tag")),
        ("null", "sync", Err("answered with no tag")),
        (
            r#"{"tag":0,"value":"valid"} false"#,
            "sync",
            Err("trailing characters"),
        ),
        (
            r#"{"tag":0,"value":""} false"#,
            "capture",
            Err("trailing characters"),
        ),
    ] {
        let mut module = Module::loaded(
            include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
            "test.pure",
            "",
        )
        .unwrap();
        module.set_budget_ms(f64::INFINITY);
        module.bind(&plan);
        let answer = module.query("wire", &[Value::str(wire), Value::str(mode)]);
        let value = answer.unwrap();
        let expected = if mode.contains("capture") {
            big.as_str()
        } else {
            "settled"
        };
        assert_eq!(value.as_str(), Some(expected), "{wire}");
        // A refused or captured reply cannot contaminate the next answer.
        assert_eq!(
            module
                .query("transfer", &[Value::str("small")])
                .unwrap()
                .as_str(),
            Some("a later call")
        );
    }
    for (wire, mode) in [
        (r#"{"tag":0,"value":""}"#, "capture"),
        (r#"{"tag":3,"call":1}"#, "async-capture"),
    ] {
        let mut module = Module::loaded(
            include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
            "test.pure",
            "",
        )
        .unwrap();
        module.bind(&plan);
        assert_eq!(
            module
                .query("wire", &[Value::str(wire), Value::str(mode)])
                .unwrap()
                .as_str(),
            Some(big.as_str())
        );
        module.set_budget_ms(0.0);
        assert!(format!(
            "{:?}",
            module
                .query("transfer", &[Value::str("plain")])
                .unwrap_err()
        )
        .contains("over the 0 ms budget"));
        assert_eq!(module.overruns(), 1);
        module.set_budget_ms(f64::INFINITY);
        assert_eq!(
            module
                .query("transfer", &[Value::str("small")])
                .unwrap()
                .as_str(),
            Some("a later call")
        );
    }
}

#[test]
fn hardened_intrinsics_allow_own_shadowing_assignments() {
    let mut module = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        "",
    )
    .unwrap();
    module.set_budget_ms(f64::INFINITY);
    module.bind(&contract::compile("component App\n  resource value = shadowIntrinsics() as shape string\n  view\n    text value\n").unwrap());
    assert_eq!(
        module
            .query("shadowIntrinsics", &[])
            .unwrap()
            .as_str()
            .unwrap(),
        r#"[["MyError","x",true],["$3.50",true,true,true],"Point(3)","Refused",["a","constructor","valueOf"],[7]]"#
    );
}

#[test]
fn shadowing_setters_keep_intrinsics_and_receiver_descriptors_locked() {
    let mut module = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/pure.hbc")).to_vec(),
        "test.pure",
        "",
    )
    .unwrap();
    module.set_budget_ms(f64::INFINITY);
    module.bind(&contract::compile("component App\n  resource locked = lockedIntrinsics() as shape string\n  view\n    text locked\n").unwrap());
    let value = module.query("lockedIntrinsics", &[]).unwrap();
    let locked: Vec<Vec<bool>> = serde_json::from_str(value.as_str().unwrap()).unwrap();
    assert_eq!(locked, vec![vec![true; 19]; 4]);
}
