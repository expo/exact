# A native event stream or byte stream may be cut by its transport's own limits

**Status:** Closed
**Resolution:** A native event stream or byte stream is open-ended (ibex2 patch 48) and takes exactIdleTimeout (LLP 1016.000 D11, amended 2026-11-17); driven on the Linux host and macOS.
**Systems:** vendor/ibex ibex2 transport (rustls_http.rs, darwin_http.mm), host/executor/src/stream.rs
**Author:** Claude (Fable 5.2), from the review of LLP 1016.000 D11
**Date:** 2026-11-11

## What is wrong

Read from the code by two reviewers and not yet driven. An HTTP stream (`exactStream` on an
`http:` or `https:` URL: server-sent events, or bytes) that is not an upload is opened with no
timeout of its own (`host/executor/src/stream.rs`, `req.timeout` stays `None`), so the
transport's defaults apply to it:

- the portable transport (Linux, Windows, Android, and macOS under a proxy) builds its agent
  with `timeout_global` 30 s, `timeout_recv_response` 15 s and `timeout_recv_body` 15 s
  (`vendor/ibex/crates/ibex2/src/transport/rustls_http.rs`, `GLOBAL_TIMEOUT`, `agent_config`).
  ureq's global limit is for the whole call and is checked on each body read. If that is what
  happens, an event stream there ends 30 s after it was asked, whatever it carries, and after
  15 s without bytes;
- URLSession ends a request after 60 s without data (`darwin_http.mm` sets `timeoutInterval`
  only when a timeout is given), so a quiet event stream on Apple ends at 60 s.

The mapper would see an `error` of kind `Network`. LLP 1016.000 D10 states these limits for an
upload with no `exactTimeout`; nothing states them for a stream, and a stream is meant to stay
open.

A WebSocket is not affected after its handshake: on macOS a socket that carried nothing stayed
open for 60 s until its `exactIdleTimeout` ended it (driven 2026-11-11, `apps/mosaic` against a
peer that never sends).

## To see it

Driven 2026-11-17 with Rust tests of the executor against loopback peers, on macOS
(`host/executor/src/stream_quiet_tests.rs`). On rustls, the Linux host's transport, an event
stream had already been exempted (`0aaf6bb17`) and carried an event every 5 s for 50 s, but a
byte stream ended at 15 s, whether the peer sent every 5 s or stayed quiet, as `Network`
`Resource temporarily unavailable (os error 35)`. On URLSession an event stream and a byte
stream whose peer stayed quiet for 70 s after its head both ended at 60 s as `Network`
`The request timed out.`

## What a fix is

A stream that is not an upload is opened with no whole-exchange limit, and its quiet is the
app's to bound. `exactIdleTimeout` was a WebSocket's only because of this (LLP 1016.000 D11,
"Why only a WebSocket"); with the limits settled it is an event stream's and a byte stream's
too.

## As fixed

ibex2's `Request::open_ended` (local patch 48) marks every stream that is not an upload: no
limit of the transport's own ends it once it is open. `exactIdleTimeout` takes any `http:` or
`https:` stream that is not an upload or a long poll, counting any bytes of the body, an event
stream's comments included, and not its head (LLP 1016.000 D11, amended 2026-11-17).

Driven 2026-11-17 on a Mac, in an app outside the repository against a peer that writes each
head at once, on the Linux host (rustls) and on macOS (URLSession): an event stream with an
event every 5 s carried all ten for 50 s; an event stream and a byte stream quiet for 70 s
after their heads each took what came then and ended at the peer's close, as `Network` (`the
event stream ended`, `the byte stream ended`); with `exactIdleTimeout: 3000` an event stream
that carried one comment at 1 s and a byte stream that carried two bytes at 1 s each ended at
4.0 s as `Timeout`, `nothing arrived for 3000 ms`, the byte stream after its message.
