//! A setting a launch may give (LLP 1153.002): baked once, the module reads
//! the launch's value where the process was given a non-empty one, else the
//! build's, and every instance in that launch reads the same; a host answers
//! only the names the module's grants carry. Each launch is this test binary
//! again, with the environment that launch gives.
use super::{bake, Fixture, Tools, APP};
use exact_js::{Module, Paired, Placement};
use exact_kernel::Kernel;
use exact_runner::{
    Answer, DataSource, Dispatch, FailureKind, Outcome, Reply, Runner, Store, Value, Work,
};
use serde_json::Value as Json;
use std::path::Path;

const NAME: &str = "LAUNCH_TEST_ORIGIN";
const BUILT: &str = "http://built.test";
const GRANTED: &str = "net.fetch https://granted.test";
/// The marked app's grants: its own, then the bake's line for the name.
const GRANTS: &str = "net.fetch https://granted.test\nenv.launch LAUNCH_TEST_ORIGIN";
/// The launch this binary plays: the two generations' directories and the
/// value the marked one should read, beside the launch's own environment.
const CHILD: &str = "EXACT_LAUNCH_SETTING_TEST";
/// The launch module's own code (js/bake/src/typescript.mjs `launchModule`).
const LAUNCHED: &str = "Object.freeze(Object.create(null))";
const CONTRACT: &str = r#"
component App
  state count = 0
  resource message = message(count) as shape string
  resource streamed = streamed("") as shape string
  mutation probe as shape string
  action refused
    send probe = fetched()
  view
    column
      text message testId="message"
"#;
const SOURCE: &str = r#"
import type { Sources, Answer } from './app.contract.d.ts';
import './spoof';
const API = import.meta.env.LAUNCH_TEST_ORIGIN ?? 'http://built.test';
export const appId = 'test.exact.logic';
export const grants = 'net.fetch https://granted.test';
const sources: Sources = {
  message: ([count]) => `${API}/${count}`,
  fetched: async () => { try { await fetch(`${API}/x`); return 'sent'; } catch (e) { return String((e as Error).message); } },
  // A stream's end is its error event's message: at the launch's origin, or at `to`.
  streamed: ([to]) => fetch(`${to || API}/s`, { exactStream: (e: { type: string; message?: string }) => e.type === 'error' ? String(e.message) : e.type } as RequestInit) as unknown as Promise<string>,
};
export const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);
"#;
/// A module app.ts imports, evaluated before its reads: it rebinds
/// `globalThis`, puts a getter on `Object.prototype` (Hermes's hardened realm
/// refuses that), asks for the settings again, and binds the launch module's
/// names.
const SPOOF: &str = r#"
const spoof = { LAUNCH_TEST_ORIGIN: 'http://spoof.test' };
try { Object.defineProperty(globalThis, 'globalThis', { value: { __exact_launch: spoof }, configurable: true }); } catch {}
try { Object.defineProperty(Object.prototype, 'LAUNCH_TEST_ORIGIN', { get: () => 'http://prototype.test', configurable: true }); } catch {}
try { (globalThis as unknown as { __exact_launch_settings: (n: string[]) => unknown }).__exact_launch_settings(['LAUNCH_TEST_ORIGIN']); } catch {}
const __exact_launch = spoof, launched = spoof;
export const named = __exact_launch === launched ? __exact_launch : null;
"#;
/// An app that marks nothing and asks anyway, from its own top level, for a
/// variable every process has and for the name the other app marks.
const UNMARKED: &str = r#"
import type { Sources, Answer } from './app.contract.d.ts';
const global = globalThis as unknown as { __exact_launch_settings?: (names: string[]) => Record<string, string> };
const given = global.__exact_launch_settings?.(['HOME', 'LAUNCH_TEST_ORIGIN']);
const read = JSON.stringify(given ?? null);
const API = given?.LAUNCH_TEST_ORIGIN ?? 'http://built.test';
export const appId = 'test.exact.logic';
export const grants = 'net.fetch https://granted.test';
const sources: Sources = {
  message: ([count]) => `${read} ${API}/${count}`,
  fetched: async () => 'unused',
  streamed: async () => 'unused',
};
export const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);
"#;

#[test]
fn a_setting_a_launch_gives_is_read_as_the_module_loads_else_the_builds() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    if let Some(child) = std::env::var_os(CHILD) {
        let child = child.into_string().unwrap();
        let [marked, unmarked, expected] = child.splitn(3, '|').collect::<Vec<_>>()[..] else {
            panic!("{child}");
        };
        launched(Path::new(marked), expected);
        return asked_unmarked(Path::new(unmarked));
    }
    let f = Fixture::new();
    f.write("app.contract", CONTRACT);
    f.write("app.ts", UNMARKED);
    f.write(
        "app.json",
        &format!(r#"{{"app":{{"id":"{APP}","name":"Logic"}}}}"#),
    );
    let unmarked = bake(&f.0, &Tools::default()).unwrap();
    let unmarked_out = f.0.join("unmarked");
    unmarked.write_new(&unmarked_out).unwrap();
    f.write("app.ts", SOURCE);
    f.write("spoof.ts", SPOOF);
    f.write(
        "app.json",
        &format!(
            r#"{{"name":"Logic","app":{{"id":"{APP}","name":"Logic"}},"env":[{{"name":"{NAME}","launch":true}}]}}"#
        ),
    );
    // An app file of the launch module's name, with any extension, is refused
    // by name: the import is the producer's module.
    f.write(
        "__exact_launch.js",
        "export const launched = { LAUNCH_TEST_ORIGIN: 'http://shadow.test' };\n",
    );
    let shadowed = bake(&f.0, &Tools::default())
        .err()
        .expect("an app file named __exact_launch is refused");
    assert!(
        shadowed.contains("__exact_launch.js: __exact_launch is the producer's launch module"),
        "{shadowed}"
    );
    std::fs::remove_file(f.0.join("__exact_launch.js")).unwrap();
    let baked = bake(&f.0, &Tools::default()).unwrap();
    // The record says the value the build gave and that a launch may replace
    // it; the grants carry the name, the bake's line; the bundle asks for it
    // before any module's code, with a prelude an older binary lacks.
    let receipt: Json = serde_json::from_str(&baked.receipt).unwrap();
    assert_eq!(
        receipt["env"],
        serde_json::json!({NAME: {"value": BUILT, "source": "default", "launch": true}})
    );
    assert_eq!(receipt["grants"], GRANTS);
    assert_eq!(receipt["typescriptRuntime"], 13);
    let script = String::from_utf8_lossy(&baked.script);
    // The launch module's code (the only `Object.freeze(Object.create(null))`)
    // comes before the module app.ts imports first.
    let at = |text: &str| script.find(text).unwrap_or(usize::MAX);
    assert!(
        at(LAUNCHED) < at("http://spoof.test") && at("http://spoof.test") < usize::MAX,
        "{script}"
    );
    // An update that marks a new name changes the grants, which a binary
    // admits only as it was built with them.
    let refused = Paired::decode(
        &baked.receipt,
        &baked.plan,
        baked.bytecode.clone(),
        APP,
        GRANTED,
    )
    .err()
    .expect("a candidate with another grant line is refused");
    assert!(
        refused.contains("changes the client's admitted grants"),
        "{refused}"
    );
    web_reads_the_builds(&f.0);
    let out = f.0.join("generation");
    baked.write_new(&out).unwrap();
    // The environment's value is the launch's however it came to be there:
    // one Bun loaded from `.env` (whose name it notes, LLP 1153 D4.2) too.
    for (given, noted, expected) in [
        (Some("http://launch.test"), "", "http://launch.test"),
        (Some(""), "", BUILT),
        (None, "", BUILT),
        (Some("http://env-file.test"), NAME, "http://env-file.test"),
    ] {
        let mut launch = std::process::Command::new(std::env::current_exe().unwrap());
        launch
            .args([
                "launch::a_setting_a_launch_gives_is_read_as_the_module_loads_else_the_builds",
                "--exact",
                "--test-threads=1",
            ])
            .env(
                CHILD,
                format!("{}|{}|{expected}", out.display(), unmarked_out.display()),
            )
            .env("EXACT_BUN_LOADED", noted);
        match given {
            Some(value) => launch.env(NAME, value),
            None => launch.env_remove(NAME),
        };
        let ran = launch.output().unwrap();
        assert!(
            ran.status.success() && String::from_utf8_lossy(&ran.stdout).contains("1 passed"),
            "{given:?}: {}{}",
            String::from_utf8_lossy(&ran.stdout),
            String::from_utf8_lossy(&ran.stderr)
        );
    }
}

/// The web's bundles of the marked app in `app`, the page's and the render's:
/// no launch gives a value there, so each holds the build's, inlined, and
/// nothing reads a launch's (LLP 1153.002 D2).
fn web_reads_the_builds(app: &Path) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = app.join("web-out");
    let built = std::process::Command::new(std::env::var_os("BUN").unwrap_or_else(|| "bun".into()))
        .arg(root.join("host/web-js/build.mjs"))
        .args(["logic", "--render", "js", "--out"])
        .arg(&out)
        .env("EXACT_APP_DIR", app)
        .env_remove(NAME)
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let (page, render) = (out.join("app.js"), out.join(".gen/server.js"));
    for bundle in [&page, &render] {
        let text = std::fs::read_to_string(bundle).unwrap();
        // No read of a launch's value: none of the launch module, which
        // every such read imports.
        assert!(
            text.contains(BUILT) && !text.contains(LAUNCHED),
            "{}",
            bundle.display()
        );
    }
}

/// The generation in `dir`: its receipt, plan and bytecode.
fn generation(dir: &Path) -> (String, Vec<u8>, Vec<u8>) {
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap();
    let receipt = String::from_utf8(read("app.module.json")).unwrap();
    (receipt, read("app.plan"), read("app.hbc"))
}

/// One launch of the marked generation in `dir`, whose module should read `expected`.
fn launched(dir: &Path, expected: &str) {
    let (receipt, plan, bytecode) = generation(dir);
    let paired = Paired::decode(&receipt, &plan, bytecode.clone(), APP, GRANTS).unwrap();
    let mut live = Runner::boot(
        paired.plan,
        paired.module,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    // The first frame is the bake's, made with the build's value; the launch
    // asks again once the module has loaded.
    assert_eq!(
        live.resource("message"),
        Some(&Value::str(&format!("{BUILT}/0")))
    );
    live.data().load().unwrap();
    live.data_ready().unwrap();
    assert_eq!(
        live.resource("message"),
        Some(&Value::str(&format!("{expected}/0")))
    );
    // Fixed for the launch: an instance made after the environment changes
    // (a reload's, a worker's) reads what the first one read.
    std::env::set_var(NAME, "http://changed.test");
    let plan = live.plan().clone();
    let mut reloaded = Module::new(bytecode.clone(), APP, GRANTS);
    reloaded.bind(&plan);
    reloaded.load().unwrap();
    let mut store = Store::new(GRANTS, []);
    let Answer::Now(message) = reloaded
        .answer(&mut store, "message", &[Value::Number(1.0)])
        .unwrap()
    else {
        panic!("a message now");
    };
    assert_eq!(message, Value::str(&format!("{expected}/1")));
    let mut worker = Module::new(bytecode.clone(), APP, GRANTS);
    worker.bind(&plan);
    let mut worker = worker.placed(Placement::Worker);
    worker.activate().unwrap();
    let Answer::Later(turn) = worker
        .answer(&mut store, "message", &[Value::Number(2.0)])
        .unwrap()
    else {
        panic!("a turn for the owner");
    };
    let Dispatch::Run(Work::Later(work)) = worker.dispatch(turn.continuation.unwrap(), &store)
    else {
        panic!("a turn for the owner");
    };
    let (tx, rx) = std::sync::mpsc::channel();
    work(Reply::new(move |outcome| {
        let _ = tx.send(outcome);
    }));
    let outcome = rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
    let Answer::Now(message) = worker
        .parse(&mut store, "message", &[Value::Number(2.0)], outcome)
        .unwrap()
    else {
        panic!("the owner's message");
    };
    assert_eq!(message, Value::str(&format!("{expected}/2")));
    // A process that renders the web's pages reads the build's value, as
    // the browser's bundle does, whatever its environment, on main or on a
    // worker built from its template.
    for placement in [Placement::Main, Placement::Worker] {
        let mut web = Module::new(bytecode.clone(), APP, GRANTS);
        web.bind(&plan);
        web.render_web();
        let mut web = web.placed(placement);
        web.activate().unwrap();
        let read = match web
            .answer(&mut store, "message", &[Value::Number(3.0)])
            .unwrap()
        {
            Answer::Now(message) => message,
            Answer::Later(turn) => {
                let Dispatch::Run(Work::Later(work)) =
                    web.dispatch(turn.continuation.unwrap(), &store)
                else {
                    panic!("a turn for the owner");
                };
                let (tx, rx) = std::sync::mpsc::channel();
                work(Reply::new(move |outcome| {
                    let _ = tx.send(outcome);
                }));
                let outcome = rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
                let reply = web.parse(&mut store, "message", &[Value::Number(3.0)], outcome);
                let Answer::Now(message) = reply.unwrap() else {
                    panic!("the owner's message");
                };
                message
            }
        };
        assert_eq!(read, Value::str(&format!("{BUILT}/3")), "{placement:?}");
    }
    // A request the grants refuse at the launch's origin names the setting
    // (D3); at the build's, the refusal is as it was.
    let Answer::Later(request) = reloaded.answer(&mut store, "fetched", &[]).unwrap() else {
        panic!("a request");
    };
    assert_eq!(request.url, format!("{expected}/x"));
    let refusal = "outside the app's grants (net.fetch)";
    let failed = Outcome::Failed {
        kind: FailureKind::Refused,
        message: refusal.into(),
    };
    let Answer::Now(said) = reloaded.parse(&mut store, "fetched", &[], failed).unwrap() else {
        panic!("the refusal's message");
    };
    let named = format!("{refusal}: {expected} is {NAME}'s value, which this launch gave, and grants are fixed at build: grant what a launch may name (net.fetch https:, net.fetch http:, or that origin)");
    let want = if expected == BUILT {
        refusal.to_owned()
    } else {
        named
    };
    assert_eq!(said, Value::str(&want));
    // A stream's refusal there names it too; at another origin it is as it was.
    for (to, plain) in [("", false), ("https://elsewhere.test", true)] {
        let args = [Value::str(to)];
        let Answer::Later(request) = reloaded.answer(&mut store, "streamed", &args).unwrap() else {
            panic!("a stream");
        };
        assert!(request.stream, "{to}");
        let failed = Outcome::Failed {
            kind: FailureKind::Refused,
            message: refusal.into(),
        };
        let Answer::Now(said) = reloaded
            .parse(&mut store, "streamed", &args, failed)
            .unwrap()
        else {
            panic!("the stream's end");
        };
        assert_eq!(
            said,
            Value::str(if plain { refusal } else { &want }),
            "{to}"
        );
    }
}

/// The generation in `dir`, which marks nothing: its own top level asks for
/// `HOME` and for the name the other app marks, and reads neither, since its
/// grants carry neither; its read of the name is the build's value.
fn asked_unmarked(dir: &Path) {
    let (receipt, plan, bytecode) = generation(dir);
    let paired = Paired::decode(&receipt, &plan, bytecode, APP, GRANTED).unwrap();
    let mut module = paired.module;
    module.bind(&paired.plan);
    module.load().unwrap();
    let mut store = Store::new(GRANTED, []);
    let Answer::Now(message) = module
        .answer(&mut store, "message", &[Value::Number(0.0)])
        .unwrap()
    else {
        panic!("a message now");
    };
    assert!(std::env::var_os("HOME").is_some());
    assert_eq!(message, Value::str(&format!("{{}} {BUILT}/0")));
}
