# An Android recording stop can hang without an answer

**Status:** Open
**Systems:** packages/recording
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

Stopping a take sets the recorder inactive and queues `encoder.finish`. That asks for one input buffer and, if none is free within 10 ms, never queues end-of-stream. `drain(true)` then loops: a `TRY_AGAIN` returns only when `end` is false. The writer never reaches `muxer.stop` or the callback that sets `answer`.

`muxer.stop` and `part.finish` are also uncaught. An `IllegalStateException` kills the writer thread with `answer` still unset. `stopMediaRecorder` waits on that answer. A later stop appends another waiter and returns, because the state is already inactive. Without a background part, the MP4 index is written only at `muxer.stop`, so the file stays unreadable.

## Where

`packages/recording/android/Recording.kt`, `Take.finish` and `Encoder.finish` and `drain`.

## How to reproduce

Stop a recording while the encoder's input buffers are full. The stop never answers. Stop again. That call waits on the same unanswered take.

## Direction

Retry until an input buffer accepts end-of-stream, and bound `drain` when ending. Stop and release the muxer and the part inside try/catch. On failure set `answer` to an error and invoke every waiter.
