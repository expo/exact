# Each independent HTTP worker has its own transport, so a launch's first reads each open a new connection

**Status:** Open
**Systems:** host/executor/src/lib.rs (compiled by every native host), host/executor/src/async.rs
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261110-scoped-request-opens-new-transport-each.md

## What is wrong

The worker executor (the default; the asynchronous core is off unless `EXACT_HTTP_ASYNC=1`, `host/executor/src/async.rs:924-929`) gives each shared worker its own bindings. `owners` (`host/executor/src/lib.rs:278-293`) leaves every slot but the ordered one `None`, and `worker` (`:1050-1062`) fills it with `host().endow(g)`, a new `ibex2::host::Host` and so a new platform transport: on Apple a new `NSURLSession`, elsewhere a new rustls pool. The comment at `:22` states the design: "Shared HTTP workers, each with its own transport".

HTTP/2 is meant to carry many exchanges on one connection. With a transport per worker, the first request each worker runs pays a TCP and TLS handshake, and the connections never pool across workers. With the default six workers that is up to six handshakes to one origin, plus the ordered lane's; with `EXACT_HTTP_WORKERS` raised, more. The asynchronous core already uses one `NSURLSession` per lane (`async.rs:5-6`).

Since `61ed608c` an ordinary resource read runs on these shared workers too, not only one marked `exactIndependentHttp`, so every app with overlapping reads pays this. A request with a grant scope now runs on its worker's own transport (`b4a2605d`, which closed the scoped-request issue), so it reuses that worker's connections but still not another worker's.

## The failure

In the port of a large React Native app (iOS device, exact2 `94b8671f`), the first eleven requests of a cold launch each opened a new HTTP/2 connection, with time to first byte of 390 to 1,100 ms against 150 to 250 ms on an open connection. The app had raised `EXACT_HTTP_WORKERS`. The reporter estimates one shared session would save about 300 ms per cold launch.

Checked by reading the code; the connection counts are the reporter's.

## Fix

Give the independent lane's workers one shared transport (bindings endowed once, cloned into each worker), keeping streams on their own as now. Alternatively make the asynchronous core the default on Apple, which has one session per lane. A test beside `host/executor/src/scope_tests.rs` that counts the connections a loopback HTTP/1.1 keep-alive server accepts for six concurrent independent reads, as the scoped-request issue's did, would pin it.

## Owner

LLP 1162 owns this: D5 (one loader a process) and D8 (the transport is the platform's client, with stage 4's campaign deciding whether the asynchronous machine becomes Apple's default). It is built in that LLP's stages, not as a separate fix. A design review on 2026-11-12 ruled that the per-worker transport should go: on the portable transport the independent workers share one transport, and on Apple the asynchronous machine, with one `URLSession` a lane, is the way there. It also recommended that D8's thread-or-memory condition be reported rather than required, since this is now a defect and not an optimisation. Close this issue in the commit that lands that change, or say here why the campaign kept the workers.
