# A native event stream or byte stream may be cut by its transport's own limits

**Status:** Open
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

Not done yet. An app with an event stream resource against a local server that sends one event
every 5 s, on the Linux host (`cargo build --profile host-dev -p <app>-linux`, which runs
headless on a Mac): `agent linux "clock until state <resource> == … within 45000"` shows whether
the stream is still delivering after 30 s. The same on macOS with a server that sends nothing
for 70 s.

## What a fix is

If it is so: a stream that is not an upload is opened with no whole-exchange limit, and its
quiet is the app's to bound. `exactIdleTimeout` is a WebSocket's only today because of this
(LLP 1016.000 D11, "Why only a WebSocket"); with the limits settled it can be an event stream's
and a byte stream's too.
