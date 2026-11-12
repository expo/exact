# A request with a grant scope gets a new transport, so it never reuses a connection

**Status:** Closed
**Resolution:** A scoped request runs on its worker's or lane's own fetch binding under the scope's grants (`Fetch::with_grants`, vendored Ibex patch 36), so it reuses that transport's connections, and no host is made per scoped request on either core.
**Systems:** host/apple/src/executor_core.rs (compiled by every native host), data/src/mixed.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, a worker runs a request that carries `Request::grants` on bindings made for that one request. `scoped_bindings` (`host/apple/src/executor_core.rs:1296-1309`) calls `host().endow(g)`, and `host` makes a new platform transport each time (`ibex2::host::Host::new`, `:261` and `:285`; the Linux host's `portable_host`). `worker` drops those bindings when `execute` returns (`:1277-1279`), and the transport with them: on Apple a `DarwinTransport` and its `NSURLSession`, elsewhere a rustls pool. The worker's own bindings, whose transport keeps connections open between requests, serve only unscoped requests (`:1280-1287`).

Only `exact_data::Mixed` sets a scope, and it sets one on every request its sources make (`data/src/mixed.rs:286-300`, `:449`). So each HTTP request of an app that composes TypeScript and Rust sources opens a new connection: a TCP handshake and, for an HTTPS origin, a TLS handshake before every request, and no HTTP/2 connection shared with the last one. The two apps here built on `Mixed`, `apps/fieldnotes` and `apps/update-lab`, hold no `net.fetch` grant, so no app in the repository pays this today.

The asynchronous core (`EXACT_HTTP_ASYNC=1`, off by default) takes only the scope's `Fetch` and starts it on the lane's transport (`executor_async.rs:635`, `:667-672`). LLP 1120's as-built notes: "Scoped grants use the lane's transport with the scope's grants."

## The failure

Ordered `GET`s, eight at a time, each sent when the last has answered, to a loopback HTTP/1.1 keep-alive server that counts the connections it accepts:

```rust
let mut request = Request::get(&format!("http://127.0.0.1:{port}/x"));
request.grants = scope.clone();   // None, or Some("net.fetch http://127.0.0.1:<port>"), the app's grants
core.run(job(ticket, request), None).unwrap();
```

| Eight requests | Connections opened | Time |
|---|---|---|
| unscoped, first (two runs) | 1 | 28 and 45 ms |
| scoped, first (two runs) | 8 | 5 and 10 ms |
| unscoped, warm (three rounds) | 0 | 5.3 to 6.0 ms |
| scoped, warm (three rounds) | 8 | 10.7 to 11.8 ms |

Expected: the scoped requests ride the connection the unscoped ones opened. A scope narrows what a request may reach; it should not change the transport that carries it.

## Verification

A temporary test beside `host/apple/src/executor_tests.rs`, on the platform transport (URLSession), printed the table:

```
cargo test -p exact-apple --lib <name> -- --nocapture
```

The first unscoped round includes URLSession's start. On loopback a warm scoped request costs about twice an unscoped one; the cost of a handshake to an HTTPS origin was not measured. Not run: the Linux host (same file, the rustls transport).

## Direction

Check the scope as now, then run the request on the worker's own transport with the scope's grants (a `Fetch` of the worker's transport and the scoped `GrantSet`), as `executor_async.rs` does. A stream keeps its own transport by design.

## Related

QUEUE.md, "An independent read can wait behind a stream opened after it" is about the lane's bytes. No issue or pitfall names `scoped_bindings`.
