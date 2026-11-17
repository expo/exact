use super::{Fixture, APP};
use exact_js::Paired;
use exact_kernel::Kernel;
use exact_runner::{Runner, Value};
use std::process::Command;

const GRANTS: &str = "net.websocket wss://example.test\nnet.fetch https://example.test";
const CONTRACT: &str = r#"
component App
  resource timeout = timeout(0) as shape string
  resource timeout_again = timeout(1) as shape string
  resource idle_zero = idle_zero() as shape string
  resource idle_large = idle_large() as shape string
  resource idle_event = idle_event() as shape string
  resource valid = valid() as shape string
  resource stored = stored() as shape string
  view
    text timeout
"#;
const SOURCE: &str = r#"
import type { Sources, Answer } from './app.contract.d.ts';
export const appId = 'test.exact.logic';
export const grants = 'net.websocket wss://example.test\nnet.fetch https://example.test';
const sources: Sources = {
  timeout: async () => await fetch('wss://example.test', { exactStream: () => 'live', exactTimeout: 1000 }),
  idle_zero: async () => await fetch('wss://example.test', { exactStream: () => 'live', exactIdleTimeout: 0 }),
  idle_large: async () => await fetch('wss://example.test', { exactStream: () => 'live', exactIdleTimeout: 3600001 }),
  idle_event: async () => await fetch('https://example.test', { exactStream: () => 'live', exactIdleTimeout: 1000 }),
  valid: async () => await fetch('wss://example.test', { exactStream: () => 'live', exactIdleTimeout: 1000 }),
  stored: async (_, store, storage) => { await storage.sqlite.open('app:/data/notes.db'); return 'stored'; },
};
export const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);
"#;

#[test]
fn bake_warns_once_per_resource_for_refused_stream_options_and_launch_still_refuses() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = Fixture::new();
    f.write("app.contract", CONTRACT);
    f.write("app.ts", SOURCE);
    let baked = f.bake();
    let rejected = [
        ("timeout", "exactTimeout: a stream has no timeout"),
        ("timeout_again", "exactTimeout: a stream has no timeout"),
        ("idle_zero", "exactIdleTimeout must be an integer"),
        ("idle_large", "exactIdleTimeout must be an integer"),
        ("idle_event", "exactIdleTimeout"),
    ];
    assert_eq!(baked.warnings.len(), rejected.len(), "{:?}", baked.warnings);
    for (name, why) in rejected {
        assert_eq!(
            baked
                .warnings
                .iter()
                .filter(|w| w.contains(&format!("`{name}`")) && w.contains(why))
                .count(),
            1,
            "{:?}",
            baked.warnings
        );
    }
    let printed = Command::new(env!("CARGO_BIN_EXE_exact-js-bake"))
        .arg(&f.0)
        .arg("--out")
        .arg(f.0.join("generation"))
        .output()
        .unwrap();
    assert!(printed.status.success(), "{printed:?}");
    assert_eq!(
        String::from_utf8(printed.stderr)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        baked.warnings
    );
    serde_json::from_slice::<serde_json::Value>(&printed.stdout).unwrap();
    let candidate =
        Paired::decode(&baked.receipt, &baked.plan, baked.bytecode, APP, GRANTS).unwrap();
    assert!(candidate
        .plan
        .resources
        .iter()
        .all(|row| row.initial.len == 0));
    let mut live = Runner::boot(
        candidate.plan,
        candidate.module,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for name in [
        "timeout",
        "timeout_again",
        "idle_zero",
        "idle_large",
        "idle_event",
        "valid",
        "stored",
    ] {
        assert_eq!(live.resource(name), Some(&Value::str("")));
    }
    assert!(live.take_requests().is_empty());
    live.data().load().unwrap();
    live.data_ready().unwrap();
    let failures = live.failed_resources();
    for (name, why) in rejected {
        assert!(
            failures
                .iter()
                .any(|(resource, failure)| *resource == name && failure.contains(why)),
            "{failures:?}"
        );
        assert_eq!(live.resource(name), Some(&Value::str("")));
    }
    assert!(!failures.iter().any(|(name, _)| *name == "valid"));
    assert_eq!(live.take_requests().len(), 1);
    assert!(live.bake_warnings().is_empty());

    f.write(
        "app.ts",
        &SOURCE
            .replace("exactTimeout: 1000", "exactIdleTimeout: 1000")
            .replace("exactIdleTimeout: 0", "exactIdleTimeout: 1000")
            .replace("exactIdleTimeout: 3600001", "exactIdleTimeout: 1000")
            .replace("fetch('https://", "fetch('wss://"),
    );
    assert!(f.bake().warnings.is_empty());
}
