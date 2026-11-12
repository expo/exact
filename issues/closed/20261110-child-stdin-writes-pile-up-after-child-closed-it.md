# Writes to a child that closed its stdin pile up until every batch is refused

**Status:** Closed
**Resolution:** The stdin thread keeps running after `EPIPE` and drops each queued and later write, so the queue drains and `drain` follows instead of the count growing until writes are refused.
**Systems:** host/apple/src/executor_process_real.rs, host/apple/src/executor_process.rs, desktop processes (LLP 1133.007 D2)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, the thread that writes a real child's stdin ends at its first failed write (`host/apple/src/executor_process_real.rs:450-456`): it clears the queue, sets the count to 0, tells the supervisor and breaks. The `Stdin` it served stays in `RealChild`, so each later `RealChild::write` (`:223-229`) adds to `queued` and pushes onto a queue nothing reads. The handle's count (`ChildCtl::queued`) falls only by what the child consumed, so from then on it only grows, and once a write would take it past 1 MiB `process::check` refuses it (`executor_process.rs:930-934`). `check` judges a batch whole (`executor_socket.rs:287`), so the batch's other operations are refused with it. A write that answered `false` never hears `drain`, and `writableLength` never falls.

LLP 1133.007 §17 says a write to a child that has gone "fails with `EPIPE`, which drops what its stdin queued". Only what was queued at the first failure is dropped.

## The failure

Spawn `/bin/sh -c 'exec 0<&-; sleep 6'`, wait 400 ms, then send six writes of 300,000 bytes, 300 ms apart, each asking for `drain`. The queue `Core::socket` reports after each:

```
[Ok(300000), Ok(300000), Ok(600000), Ok(900000),
 Err("QuotaExceededError: the child's stdin queue exceeds 1 MiB"),
 Err("QuotaExceededError: the child's stdin queue exceeds 1 MiB")]
queued now: Some(900000)
events: ["drain"]
```

The first write is dropped and drains. The next three are held for as long as the child runs; the fifth and every later one is refused.

Expected: each later write is dropped as the first was, so the queue stays empty and `drain` follows; or the handle's stdin reads as ended and `write` refuses with `InvalidStateError`, as after `end`. Node 26.11.1 on the same child reports `EPIPE` on `child.stdin` at the first write and destroys the stream: the later writes fail with `ERR_STREAM_DESTROYED` and `writableLength` stays 0.

## Verification

A temporary test in `host/apple/src/executor_process_real_tests.rs` (`cargo test -p exact-apple --lib <name> -- --nocapture`) sent the six batches as a mutation's (`ProcessOp::Write { data, drain: true }`), read `core.socket_reader()` and drained the stream's events. It printed the lines above. Node's result is `child_process.spawn` with the same script and writes. Not run: Linux (same file).

## Direction

Mark the `Stdin` closed when a write fails and have `RealChild::write` drop bytes for a closed one, or set `ChildCtl::ended` so `check` answers "the child's stdin is ended".

## Related

QUEUE.md's desktop process entries do not cover stdin. `a_real_child_reads_its_stdin_and_streams_its_output_in_order` covers only a child that reads.
