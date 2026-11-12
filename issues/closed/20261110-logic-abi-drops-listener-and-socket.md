# A Rust module's local listener crosses the logic seam as an HTTP stream to its `unix:` address instead of being refused

**Status:** Closed
**Resolution:** `encode_result` refuses any request with a listener, socket mark or none, and a byte stream, by name; a WebSocket's `socket` crosses as tag 11 since 38971bd6, which also refused a `Request::listen` request but not a listener without the mark.
**Systems:** logic/abi/src/lib.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

The module seam's request encoding carries a method, a URL, headers, a body and a few flags. For everything else a request can say, `encode_result` refuses rather than drops, and says so each time: a timeout "never silently becomes none", a held mark is "never silently waited on", an upload's progress is "never sent as a plain stream", and likewise `exactBodyFrom`, `exactBodyTo` and a redirect mode (`logic/abi/src/lib.rs:305-342`, `:381-386`).

Two fields have no such arm. `listen` (a local listener, made by `Request::listen`, which also sets `socket`) and `socket` alone (a two-way WebSocket, LLP 1016.000 D7) fall into the HTTP arm (`lib.rs:388-418`), which writes neither, and `read_http` fills in `socket: false` and `listen: None` (`lib.rs:498`, `:508`).

LLP 1133.005 D1 gives a Rust source the listener: "Rust sources answer `Answer::stream(Request::listen(address))` and read the same events through their `parse`." Embedded, that works. Loaded as a module (a development reload, a signed Rust update), the same request reaches the host as a stream to the URL `unix:/…`, and `parse` gets a network failure for a fetch the source never asked for. For `socket` alone the host opens a stream that only listens, which is all LLP 1016.000 gives a Rust module, but the mark is dropped silently where the file refuses its neighbours.

## The failure

Across the seam (`encode_result`, then `read_result`):

```rust
Answer::stream(Request::listen("unix:/tmp/probe.sock", Listen::default()))
```

```
listener: sent socket=true listen=true | arrived stream=true socket=false listen=false url=unix:/tmp/probe.sock
```

The macOS executor, given the request each way with `net.listen` granted:

```
embedded: first outcome Message(Message { event: "listening", id: "0", data: "unix:/…/a.sock", … })
module:   admitted Ok(()), first outcome Failed { kind: Network, message: "TypeError: URL has no host" }
```

Expected, by the file's own rule: `Err(DataError::Unavailable("… cannot cross the Rust module seam"))` when the module answers.

## Verification

Ran a test in `logic/abi/src/tests.rs` that encodes and decodes the answer (`cargo test -p exact-logic-abi --lib`), and one in `host/apple/src/executor_listen_tests.rs` that runs its `listen(..)` request unchanged and then with `socket` and `listen` cleared (`cargo test -p exact-apple --lib`). They printed the lines above. No Rust source in the tree calls `Request::listen` outside tests.

## Direction

Add two refusal arms beside the others, for `r.listen.is_some()` and `r.socket`. `bytes` (a byte stream, set only by TypeScript today) would take the same arm.

## Related

QUEUE.md line 1716 records the held mark's refusal at this seam; line 140 ("The local listener, what is owed") lists nothing about Rust modules. `rules/DEFERRED.md` keeps Rust replies on a listener out, not a Rust listener.
