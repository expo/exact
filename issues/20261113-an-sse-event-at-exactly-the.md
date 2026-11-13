# An SSE event at exactly the ceiling is refused

**Status:** Open
**Systems:** host
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

An SSE parser may carry up to `limit` bytes of event data. A `data` field is refused when `data.len() + value.len() + 1 > limit`, and the value is then stored with a newline. Dispatch removes one trailing newline. A single-line event whose data is exactly `limit` bytes fails that check. The data that would be delivered is `limit` bytes, which the parser's comment allows.

## Where

`host/apple/src/executor_stream.rs`, `EventStream::line_done` for the `data` field, and the pop of the trailing newline.

## How to reproduce

Push one `data:` line whose value length equals the parser's limit, then a blank line. `push` returns `"an event exceeds the response ceiling"`.

## Direction

Count the newline only when another `data` field will keep it. A single field of `limit` bytes, and a delivered event of `limit` bytes, should pass.
