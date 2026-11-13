# A worker-placed source's `exactListen` is not refused by name: the envelope never writes the field its reader checks

**Status:** Closed
**Resolution:** `encode_optional` refuses a turn whose request, or any fetch it leaves in flight, is a listener, while the `Request` still says so, and the dead check in `request_from` is gone.
**Systems:** data/src/envelope.rs, local listeners (LLP 1133.005), worker placement (LLP 1027.002)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

A turn's reply crosses from a worker, or from an ordered set's member, as an envelope. `request_from` refuses a listener with a message that tells the author what to do (`data/src/envelope.rs:218`, at `dd6f5dcd`):

```
exactListen is unavailable in a worker-placed source; place it on main
```

It tests `json["listen"]`. `request_json`, the only writer of that JSON (`data/src/envelope.rs:166`), writes no `listen` key, so the test is never true. LLP 1133.005 records the refusal as built: "A worker-placed source's listener is refused (`data/src/envelope.rs`)."

What crosses instead is the request without its listener options and without `socket`, which `request_from` always clears: a listen-only stream `GET` whose URL is the listener's address. The runner hands that to the host as an ordinary request.

## The failure

`js/tests/it/listen.rs`'s fixture, whose `control` resource calls `exactListen`, with the module on a worker:

```rust
let mut source = m.placed(Placement::Worker);   // `m` as in `boot()`
source.bind(&plan);
source.activate().unwrap();
let mut r = Runner::boot(plan, source, Kernel::with_monospace(), Default::default(), "/").unwrap();
// dispatch each continuation and fulfil it with the owner's reply
```

Observed: the journal has no refusal, and the request that leaves the runner is

```
GET unix:/tmp/exact-listen-js/a.sock   stream: true, socket: false, listen: None
```

Expected: `control` fails with `exactListen is unavailable in a worker-placed source; place it on main`. On the main thread the same request leaves with `socket: true` and its listener options.

## Verification

Ran that drive (`cargo test -p exact-js --test it`); it printed the request above and the journal line `request 3 (control): GET unix:/tmp/exact-listen-js/a.sock`. At the envelope alone (`cargo test -p exact-data --lib`), `envelope::apply` of `envelope::encode` of a valid `Request::listen` stream request answers `Ok(Answer::Later(_))` with `listen: None` and `socket: false`. No host was run, so what a host answers to that `GET` is not known.

## Direction

Refuse in `encode`, where the `Request` still has `listen`, as it already refuses a long poll no host may run (`data/src/envelope.rs:47`), and delete the dead test in `request_from`.

## Related

QUEUE's "Streams after LLP 1069.004 slices 1–3" owes worker-placed streams' socket sends; "The local listener, what is owed" does not mention placement. The three closed listener issues of 2026-11-02 concern its path checks, its connection order and a ported callback, not placement.
