//! LLP 1069.005 D2/D3 through the Hermes bytecode executor, runner and
//! bake: secure randomness is a counted device read, refused during module
//! initialization, and never compiled into a plan. `js/web/tests/it/browser.rs`
//! holds the browser realms to the same answers.
#![cfg(exact_js_engine)]

use exact_js::Module;
use exact_kernel::Kernel;
use exact_plan::{Plan, Value};
use exact_runner::{Answer, DataSource, Dispatch, Event, Outcome, Response, Runner, Store};

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/entropy.hbc"));
const APP: &str = "test.entropy";
/// `crypto`'s shape, the same string the browser realms answer.
const GLOBALS: &str = "object/function/function/[object Crypto]/getRandomValues,randomUUID,subtle/[object SubtleCrypto]";
const GRANTS: &str = "net.fetch https://fixture.exact.test\n";

const SRC: &str = r#"
component App
  state form = "uuid"
  resource id = uuid() as shape string
  resource plain = plain() as shape string
  resource abc = abcDigest() as shape string
  mutation result as shape string
  mutation count as shape number
  action atInit
    send result = atInit(form)
  action bytes
    send result = bytes(4)
  action later
    send result = uuidLater()
  action laterBytes
    send count = bytesLater()
  action turn
    send result = entropyTurn("helper")
  action refusals
    send result = refusals()
  action describe
    send result = globals()
  action digests
    send result = digests()
  action digestRefusals
    send result = digestRefusals()
  action digestLater
    send result = digestLater()
  view
    column
      text id testId="id"
      text plain testId="plain"
      text abc testId="abc"
"#;

fn plan() -> Plan {
    contract::compile(SRC).expect("entropy fixture compiles")
}

fn module() -> Module {
    let mut module = Module::loaded(HBC.to_vec(), APP, GRANTS).expect("entropy fixture loads");
    module.set_budget_ms(f64::INFINITY);
    module.bind(&plan());
    module
}

fn text(answer: Answer) -> String {
    match answer {
        Answer::Now(value) => value.as_str().expect("a string").to_string(),
        Answer::Later(_) => panic!("expected an answer now"),
    }
}

fn is_v4(id: &str) -> bool {
    let bytes = id.as_bytes();
    id.len() == 36
        && [8, 13, 18, 23].iter().all(|&i| bytes[i] == b'-')
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && id
            .chars()
            .all(|c| c == '-' || c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[test]
fn a_draw_is_a_counted_read_and_refuses_during_initialization() {
    let mut module = module();
    for form in ["uuid", "bytes", "late-uuid", "late-bytes"] {
        let message = module.query("atInit", &[Value::str(form)]).unwrap();
        // Hermes first drains a module's queued reactions after its first query has ended initialization.
        let phase = if form.starts_with("late-") {
            "outside an answer"
        } else {
            "during module initialization"
        };
        assert!(
            message
                .as_str()
                .unwrap()
                .contains(&format!("unavailable {phase}; call it inside an answer")),
            "{form}: {message:?}"
        );
    }
    let mut store = Store::new(GRANTS, vec![]);
    let first = text(module.answer(&mut store, "uuid", &[]).unwrap());
    assert!(is_v4(&first), "{first}");
    assert_eq!((store.reads(), store.entropy_draws()), (1, 1));
    let second = text(module.answer(&mut store, "uuid", &[]).unwrap());
    assert!(is_v4(&second) && second != first);
    let bytes = text(
        module
            .answer(&mut store, "bytes", &[Value::Number(4.0)])
            .unwrap(),
    );
    assert_eq!(bytes.split(',').count(), 4, "{bytes}");
    assert_eq!(store.entropy_draws(), 3);

    // Refusals and shape: the same strings the browser realms answer.
    let before = store.reads();
    for (source, expected) in [
        ("plain", "no randomness"),
        ("refusals", "QuotaExceededError/TypeMismatchError/TypeError"),
        ("globals", GLOBALS),
    ] {
        assert_eq!(
            text(module.answer(&mut store, source, &[]).unwrap()),
            expected
        );
    }
    assert_eq!(store.reads(), before, "no draw, no read");

    // A draw after a fetch belongs to the answer the fetch resumes.
    assert!(matches!(
        module.answer(&mut store, "uuidLater", &[]),
        Ok(Answer::Later(_))
    ));
    let reply = Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: vec![],
    });
    let later = text(module.parse(&mut store, "uuidLater", &[], reply).unwrap());
    assert!(is_v4(&later));
    assert_eq!(store.entropy_draws(), 4);
    let answer = module.answer(&mut store, "bytesLater", &[]).unwrap();
    assert!(matches!(answer, Answer::Later(_)));
    let reply = Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: vec![],
    });
    assert!(matches!(
        module.parse(&mut store, "bytesLater", &[], reply).unwrap(),
        Answer::Now(Value::Number(4.0))
    ));
}

#[test]
fn pending_answer_continuations_may_draw_entropy() {
    let mut module = module();
    // Drain the load-time reaction before starting the pending call under test.
    module.query("plain", &[]).unwrap();
    let mut store = Store::new(GRANTS, vec![]);
    for form in [
        "helper",
        "json",
        "yield",
        "microtask",
        "cached",
        "cached",
        "response",
        "responseJson",
    ] {
        let args = [Value::str(form)];
        let mut answer = module.answer(&mut store, "entropyTurn", &args).unwrap();
        if let Answer::Later(request) = answer {
            let outcome = if form == "yield" {
                let token = request.continuation.unwrap();
                assert!(matches!(module.dispatch(token, &store), Dispatch::Again));
                Dispatch::again_outcome()
            } else {
                assert_eq!(form, "json");
                Outcome::Response(Response {
                    status: 200,
                    headers: vec![],
                    body: b"{}".to_vec(),
                })
            };
            answer = module
                .parse(&mut store, "entropyTurn", &args, outcome)
                .unwrap();
        }
        let value = text(answer);
        let (uuid, count) = value.split_once('\n').unwrap();
        assert!(is_v4(uuid), "{form}: {value}");
        assert_eq!(count, "4", "{form}");
    }
    assert_eq!(store.entropy_draws(), 16);
}

#[test]
fn a_let_go_fetch_restores_only_its_async_continuation() {
    for asynchronous in [false, true] {
        let mut module = module();
        module.query("plain", &[]).unwrap();
        let src = format!(
            "component App\n  state active = true\n  resource value = entropyKept(active, {asynchronous}) as shape string\n  resource observed = entropyObserved() as shape string\n  action stop\n    active = false\n  view\n    button press=stop testId=\"stop\"\n      text value\n"
        );
        let plan = contract::compile(&src).unwrap();
        module.bind(&plan);
        let mut runner = Runner::boot(
            plan,
            module,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let request = runner.take_requests().pop().unwrap();
        let stop = runner.kernel().find_by_test_id("stop")[0];
        let stop = runner.kernel().node_by_key(stop).unwrap().id;
        runner.dispatch(stop, Event::Press).unwrap();
        runner
            .fulfill(
                request.ticket,
                Outcome::Response(Response {
                    status: 200,
                    headers: vec![],
                    body: vec![],
                }),
            )
            .unwrap();
        let value = runner.data().query("entropyObserved", &[]).unwrap();
        let (uuid, count) = value.as_str().unwrap().split_once('\n').unwrap();
        if asynchronous {
            assert!(is_v4(uuid), "{uuid}");
            assert_eq!(count, "4");
        } else {
            assert_eq!(
                uuid,
                "crypto.randomUUID() is unavailable outside an answer; call it inside an answer"
            );
            assert_eq!(count, "crypto.getRandomValues() is unavailable outside an answer; call it inside an answer");
        }
        assert_eq!(
            runner.store().entropy_draws(),
            if asynchronous { 2 } else { 0 }
        );
    }
}

#[test]
fn synchronous_and_asynchronous_mappers_may_draw_entropy() {
    for asynchronous in [false, true] {
        let src = format!(
            "component App\n  resource id = entropyStream({asynchronous}) as shape string\n  view\n    text id\n"
        );
        let mut module = module();
        let plan = contract::compile(&src).unwrap();
        module.bind(&plan);
        let mut runner = Runner::boot(
            plan,
            module,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let request = runner.take_requests().pop().unwrap();
        runner
            .fulfill(
                request.ticket,
                Outcome::Message(exact_runner::Message::default()),
            )
            .unwrap();
        let answer = runner.resource("id").unwrap().as_str().unwrap();
        let (uuid, bytes) = answer.split_once('/').unwrap();
        assert!(is_v4(uuid), "{answer}");
        assert_eq!(bytes, "4");
    }
}

#[test]
fn bake_compiles_no_random_value_and_the_device_asks() {
    let a = contract::bake(plan(), module()).unwrap();
    let b = contract::bake(plan(), module()).unwrap();
    assert_eq!(a.encode(), b.encode(), "no draw reaches the plan's bytes");
    let row = |name: &str| {
        a.resources
            .iter()
            .find(|r| a.str(r.name) == name)
            .unwrap()
            .clone()
    };
    assert_eq!(row("id").initial.len, 0, "a draw is not compiled");
    assert!(row("id").reader, "and it is the device's to answer");
    assert!(row("plain").initial.len > 0 && !row("plain").reader);
    // D1: a digest is pure, so bake compiles its value like any other.
    assert!(row("abc").initial.len > 0 && !row("abc").reader);

    let runner = Runner::boot(
        a,
        module(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let id = runner.resource("id").unwrap().as_str().unwrap().to_string();
    assert!(is_v4(&id), "{id}");
    assert!(runner.resource_reads_store("id"));
    assert!(runner.resource_draws_entropy("id") && !runner.resource_draws_entropy("plain"));
}

/// What the fixture's `digests` answers: each SHA-2 size over the empty
/// string, `abc` and 1 MiB of `i % 251`, then SHA-256 of `abc` twice more
/// (an algorithm object with a lowercase name over a view, and a buffer).
fn expected_digests() -> String {
    use sha2::Digest;
    let large: Vec<u8> = (0..1usize << 20).map(|i| (i % 251) as u8).collect();
    let inputs: [&[u8]; 3] = [b"", b"abc", &large];
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let mut lines = Vec::new();
    for input in inputs {
        lines.push(hex(&sha2::Sha256::digest(input)));
    }
    for input in inputs {
        lines.push(hex(&sha2::Sha384::digest(input)));
    }
    for input in inputs {
        lines.push(hex(&sha2::Sha512::digest(input)));
    }
    lines.push(hex(&sha2::Sha256::digest(b"abc")));
    lines.push(hex(&sha2::Sha256::digest(b"abc")));
    lines.join("\n")
}

#[test]
fn a_digest_is_pure_on_every_input_and_refuses_the_rest_by_name() {
    let mut module = module();
    assert_eq!(
        module.query("atInit", &[Value::str("digest")]).unwrap(),
        Value::str("ran: function"),
        "a digest is allowed during module initialization"
    );
    let mut store = Store::new(GRANTS, vec![]);
    let mut answer = |source: &str| text(module.answer(&mut store, source, &[]).unwrap());
    assert_eq!(answer("digests"), expected_digests());
    assert_eq!(
        answer("digestRefusals"),
        "NotSupportedError/NotSupportedError/TypeError/TypeError/NotSupportedError/NotSupportedError"
    );
    assert_eq!(
        (store.reads(), store.entropy_draws()),
        (0, 0),
        "a digest is no read"
    );
    // After a fetch, in the answer the fetch resumes.
    assert!(matches!(
        module.answer(&mut store, "digestLater", &[]),
        Ok(Answer::Later(_))
    ));
    let reply = Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: vec![],
    });
    assert_eq!(
        text(module.parse(&mut store, "digestLater", &[], reply).unwrap()),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// The first UUID of the agent's stream for seed 1 on a TypeScript
/// executor (`exact_data::crypto`'s pinned vector; the browser realms'
/// too).
const SEED_1_UUID: &str = "deb201fb-035c-4c32-bbbf-3da08991a485";

#[test]
fn under_the_agent_two_runs_draw_the_same_stream_and_outside_it_the_os() {
    let run = |seed: Option<u64>| {
        // Seeded before it loads, as a host makes it: the prelude's own probe at load draws nothing.
        let mut module = Module::new(HBC.to_vec(), APP, GRANTS).with_agent_seed(seed);
        module.set_budget_ms(f64::INFINITY);
        module.load().expect("the fixture loads");
        module.bind(&plan());
        let said = |module: &mut Module| {
            (DataSource::take_logs(module).iter())
                .filter(|line| {
                    line.starts_with("crypto: the driver seeds this launch's randomness")
                })
                .count()
        };
        let mut store = Store::new(GRANTS, vec![]);
        text(module.answer(&mut store, "plain", &[]).unwrap());
        assert_eq!(said(&mut module), 0, "nothing is said before a draw");
        let first = text(module.answer(&mut store, "uuid", &[]).unwrap());
        let bytes = text(
            module
                .answer(&mut store, "bytes", &[Value::Number(20.0)])
                .unwrap(),
        );
        let second = text(module.answer(&mut store, "uuid", &[]).unwrap());
        // Still a device read: bake behaves the same under the agent.
        assert_eq!(store.entropy_draws(), 3);
        // The journal says once that the draws are seeded, and only when they are.
        assert_eq!(said(&mut module), usize::from(seed.is_some()));
        (first, bytes, second)
    };
    let a = run(Some(1));
    assert_eq!(a, run(Some(1)), "two agent runs mint the same values");
    assert_eq!(a.0, SEED_1_UUID);
    assert!(is_v4(&a.2) && a.2 != a.0);
    // The bytes are the stream's next, as the Rust stream draws them.
    let mut stream = exact_data::crypto::AgentStream::new(1, "typescript");
    let mut expected = [0u8; 36];
    stream.fill(&mut expected);
    let expected: Vec<String> = expected[16..].iter().map(u8::to_string).collect();
    assert_eq!(a.1, expected.join(","));
    assert_ne!(run(Some(2)).0, a.0, "another seed, another stream");
    // Outside the agent every draw is the OS's.
    let (os_a, os_b) = (run(None), run(None));
    assert!(os_a.0 != os_b.0 && os_a.0 != SEED_1_UUID && is_v4(&os_a.0));
    // The bake's module never draws the stream, whatever the environment.
    let mut baked = Module::inspect(HBC.to_vec()).unwrap();
    baked.bind(&plan());
    let mut store = Store::new(GRANTS, vec![]);
    assert_ne!(
        text(baked.answer(&mut store, "uuid", &[]).unwrap()),
        SEED_1_UUID
    );
}
