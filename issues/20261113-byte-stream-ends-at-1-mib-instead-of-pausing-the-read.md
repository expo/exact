# A byte stream ends `Refused` when 1 MiB waits for a busy module, instead of pausing the read

**Status:** Open
**Systems:** host/executor/src/bytes.rs (compiled by every native host), host/web/http-body.js
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

A TypeScript stream whose body is not `text/event-stream` is a byte stream (LLP 1016.000, amendment of 2026-10-23). Natively its reader appends each read to the one message the runner has not taken (`host/executor/src/bytes.rs`, `append`, `:77-105`). When that message would pass 1 MiB, or the stream's ceiling if lower (`RECEIVE_BYTES`, `:13`), the reader ends the stream: `Refused`, `byte stream receive queue exceeds 1 MiB` (`:42-56`). The web does the same (`host/web/http-body.js:130-133`). `docs/reference.md` documents it (around line 2040).

The queue fills when the module is slow to take messages, for instance while it is busy in other answers, not because the server sent anything wrong. A server cannot see how much the client has taken, so it cannot pace itself to the limit. The reader runs on its own thread and blocks in `body.read` anyway, so it could simply stop reading while the queue is full and let TCP flow control hold the sender back.

## The failure

An app reading NDJSON with `fetch` and `exactStream` (macOS, exact2 `9e53578f2`) had its stream ended `Refused` when the server wrote 1.1 to 1.4 MB of 200-byte lines at once while the data module was answering other requests. The stream's handler only queued the bytes. The reporter asked for backpressure, or failing that a way for the app to raise the ceiling.

Checked by reading the code; not reproduced.

## Fix

When `append` would overflow, wait (on the core's condition variable, woken when the runner takes the message) instead of returning `Overflow`, and read again once the queue has room; keep the 1 MiB as the most bytes held. A stream let go or retired while waiting ends as now. On the web, stop pulling the `ReadableStream` reader while the queue is full. Update the reference paragraph, and pin it with a test whose server writes 4 MiB at once to a runner that takes messages slowly.
