# Closing a resource gate does not release its native live source

**Status:** Open
**Systems:** runner, native modules
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, a resource that reads a bound live module source retains that
source after its `when` gate closes. The resource becomes idle and has no
pending request, but its implementation receives neither the whole-source
release nor the argument-group release. Any native subscription acquired by
that source remains active until another lifetime boundary releases it.

`read_by_any` counts a resource solely because its declaration names the
module source (`runner/src/runner/modules/bound.rs:463-466`). It does not
check whether the resource is asleep. The argument-group cleanup also keeps
an asleep resource because it still has a state (`:407-409`). Gate closure
preserves that state and sets `asleep = true`
(`runner/src/runner/settlement.rs:585-600`), so neither cleanup path releases
it. Keeping the last displayed value need not keep a live acquisition.

The following valid fixture uses a live source with a 50 ms poll interval
and a host implementation that records every request and release. Create
these files below `/tmp/exact-review-20261112/native-gate-release`:

`node_modules/@fixture/sensor/package.json`:

```json
{"name":"@fixture/sensor","version":"1.0.0","exports":{".":"./exact.json"}}
```

`node_modules/@fixture/sensor/exact.json`:

```json
{"kind":"interface","name":"@fixture/sensor","version":"1.0.0","grants":[],"shapes":{"Reading":{"level":"number"}},"names":[{"source":"level","live":true,"poll":{"option":"interval","default":50},"options":{"interval":"option<number>"},"answers":"Reading","substitute":{"default":{"level":0}}}]}
```

`app.contract`:

```text
use level, Reading from "@fixture/sensor"
component App
  state open = true
  resource reading = level(interval=50) when open as shape Reading else empty()
  action close
    open = false
  action reopen
    open = true
  view
    text `${reading.level}`
```

`main.rs`:

```rust
use exact_kernel::Kernel;
use exact_runner::{Runner, Reply, Dispatch, Work, Outcome, Response};
use std::sync::{Arc, Mutex};
fn pump(r: &mut Runner<()>) {
    for _ in 0..8 {
        let (receipts, error) = r.apply_announced();
        assert!(error.is_none(), "{error:?}");
        let requests = r.take_requests();
        if receipts.is_empty() && requests.is_empty() { return; }
        for request in requests {
            let outcome = match r.native_work(&request.request) {
                Dispatch::Run(Work::Now(work)) => work(),
                Dispatch::Run(Work::Later(work)) => {
                    let (tx, rx) = std::sync::mpsc::channel();
                    work(Reply::new(move |answer| tx.send(answer).unwrap()));
                    rx.recv().unwrap()
                }
                _ => panic!("expected native work"),
            };
            r.fulfill(request.ticket, outcome).unwrap();
        }
    }
    panic!("did not settle");
}
fn main() {
    let path = std::path::Path::new("/tmp/exact-review-20261112/native-gate-release/app.contract");
    let (plan, _) = contract::compile_path_all(path, false).unwrap();
    let mut r = Runner::boot(plan, (), Kernel::with_monospace(), Default::default(), "/").unwrap();
    let messages: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = messages.clone();
    r.native_slot().host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
        seen.lock().unwrap().push(String::from_utf8(body).unwrap());
        reply.send(Outcome::Response(Response {
            status: 200, headers: vec![], body: br#"{"level":1}"#.to_vec(),
        }));
    })));
    r.bind_extensions(&["sensor"], &[]);
    pump(&mut r);
    println!("before close: {:?}", messages.lock().unwrap());
    messages.lock().unwrap().clear();
    r.act("close", vec![]).unwrap();
    pump(&mut r);
    println!("after close: {:?}; pending={:?}", messages.lock().unwrap(), r.pending());
    r.advance(100.0).unwrap();
    pump(&mut r);
    println!("after 100ms: {:?}", messages.lock().unwrap());
}
```

Link `main.rs` against the checkout's `contract`, `exact-runner` and
`exact-kernel` libraries and run it. The captured output at
`/tmp/exact-review-20261112/native-gate-release/output.txt` is:

```text
before close: ["{\"args\":{\"interval\":50.0},\"extension\":\"sensor\",\"source\":\"level\"}"]
after close: []; pending=[]
after 100ms: []
```

Expected after `close`: the implementation receives
`{"extension":"sensor","release":"level"}` exactly once when this last
active reader goes. If another reader remains, release only the argument
group no active reader still uses. Reopening the gate must acquire it again.
LLP 1161.002 D2 makes a closed resource idle; the native source lifetime is
specified by the last reader in LLP 1133.009 D10 item 12.

Exclude asleep resources from whole-source and argument-group ownership,
while preserving ownership for other active readers and requests in flight.
Check the associated polling state when the final active reader leaves.
Add coverage for closing and reopening a live-source gate and for two
readers that close independently.

Duplicate check: searched open and closed filesystem issues and the cached
GitHub issue titles and bodies for native live-source release, last readers,
`read_by_any` and `release_idle`. No matching report was found. The separately
filed undispatched-continuation leak during a multi-pass settlement has a
different cause and does not cover an already answered native live source.
