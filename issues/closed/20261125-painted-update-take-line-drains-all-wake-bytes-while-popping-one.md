# `painted-update` `take_line()` drains all wake bytes while popping one message, and `check_and_wake()` clears `checking` before enqueueing, stranding check outcomes and breaking `immediate` activation

**Status:** Closed
**Resolution:** Queue painted-update check outcomes behind a mutex synchronized with the wake descriptor, clear checking only after enqueueing and signaling (and after the Apple completion callback), and drain all ready outcomes before ticking in Presenter::poll_update.
**Systems:** painted host update loop (`exact-painted-update`, Linux and Android)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Asymmetric pairs)

Verification: verified by reading.

At `9c0431fd5`, two defects in `host/painted-update/src/store.rs` break delivery of update check outcomes to `Presenter::poll_update` (`host/painted/src/presenter/delivery.rs:448-467`):

1. **Drain/pop mismatch (`host/painted-update/src/store.rs:205-213`):** `Updates::take_line()` loops `while let Ok(n) = (&self.wake).read(&mut buf)` to drain **all** queued wake bytes from `self.wake`, then calls `self.lines.try_recv().ok()` only **once**.
2. **Premature flag release (`host/painted-update/src/store.rs:190-195`):** `Updates::check_and_wake()` clears `checking.store(false, Ordering::SeqCst)` at line 191 **before** `tx.send(line)` (line 192) and `signal.write_all(&[1])` (line 193).

### Reachable trigger
On Linux (`host/linux/src/display/event_loop.rs:238,307`, `host/linux/src/window.rs:323,343`) or Android (`host/painted/src/canvas.rs:1210,1320`, `host/painted/src/canvas/jni.rs:490`, `host/painted/src/android.rs:651`), every host loop calls `Presenter::poll_update()` at most once per turn and sleeps on `Updates::fd()` (`self.wake`) between turns. Two outcomes accumulate in `self.lines` whenever:
- Check #1 finishes (`checking` becomes `false`, `line1` enters `self.lines`, 1 byte enters `self.wake`), and before `take_line()` runs, a second check starts and completes — reachable via `deliveryCheck` (`host/painted/src/presenter/commands.rs:102`), `WindowEvent::Focused(true)` -> `Presenter::update_foreground(true)` (`host/linux/src/window.rs:323`, `host/painted/src/presenter/delivery.rs:434`), or `Presenter::poll_update()`'s own `c.tick(now)` -> `self.check_update()` (`host/painted/src/presenter/delivery.rs:450-452`), which runs *before* `u.take_line()` at line 453; or
- Check #1's thread is preempted between `checking.store(false)` (`store.rs:191`) and `tx.send(line)` (`store.rs:192`), allowing check #2 to run and enqueue `line2` ahead of `line1` (also reversing outcome order).

### Actual vs. expected result
- **Actual:** The single `u.take_line()` call in `Presenter::poll_update()` (`delivery.rs:453`) drains all wake bytes from `self.wake` and pops only `line1`. `line2` (for example `"staged seq N"`) remains stranded in `self.lines` with `self.fd()` unreadable, so `poll()` never wakes for it. Under `deploy.activate: "immediate"`, `Checks::completed(&line)` (`delivery.rs:462`) never sees `"staged seq N"` and `self.owe_update(false)` (`delivery.rs:464`) is never called. Every subsequent check is then permanently off-by-one: check #k writes 1 wake byte and pushes `line_k`, `take_line()` drains that byte and pops `line_{k-1}`, and `line_k` stays stranded.
- **Expected:** As documented in `host/painted-update/src/store.rs:7-8` and `host/painted/src/delivery.rs:60-67`, completed checks must remain ordered and `fd()` / `take_line()` must never strand an unread outcome with a silent descriptor.

### Verification and sibling sites checked
Verified by reading `host/painted-update/src/store.rs:175-213`, `host/painted-update/src/lib.rs:51-59`, `host/painted/src/presenter/delivery.rs:359-467`, `host/painted/src/presenter/commands.rs:102`, `host/painted/src/presenter/checks.rs:86-107`, and all host callers (`event_loop.rs:238,307`, `window.rs:323,343`, `canvas.rs:1210,1320`, `canvas/jni.rs:490`, `android.rs:651`, `agent/stdio.rs:42,145`).

Sibling sites checked:
- `host/apple-update/src/store.rs:422-437` and `host/apple/Sources/ExactUpdates/Updates.swift:112-117`: Apple also clears `lock(&SNAPSHOT).checking = false` (line 423) before calling `done` (line 436), allowing callback reordering on a race, though each completion dispatches its own `DispatchQueue.main.async` block rather than polling a socketpair with a single `try_recv()`, so outcomes are not stranded.
- `host/painted/src/executor.rs:165-180`: drains all wake bytes in `begin_pump()` and drains all ready outcomes via `Vec` in `drain()` (safe).
- `host/painted/src/content_region/worker.rs:271-281`: drains all wake bytes and pops a single `Option` slot (at most one completed item; safe).
- `host/painted/src/image/workers.rs:425-429`: drains all wake bytes and polls stateful revision/requests rather than popping one item from an mpsc queue (safe).

### Suggested fix and regression test
1. In `host/painted-update/src/store.rs:190-195`, enqueue and signal before clearing `checking` (or clear `checking` after `tx.send(line)` and `signal.write_all(&[1])`, and mirror the ordering in `host/apple-update/src/store.rs:423-437`).
2. In `Updates::take_line()` (`store.rs:205-213`), either replace the mpsc channel with a single `Mutex<Option<String>>` (latest outcome wins, matching `Store::take_line`'s contract "Take the latest asynchronous outcome" and eliminating unbounded/off-by-one queuing) or drain only 1 byte per popped line / drain all queued lines in `Presenter::poll_update()` (`while let Some(line) = ...`), and call `u.take_line()` before `c.tick(now)` in `Presenter::poll_update()`.
3. Add a unit test in `host/painted-update/src/store.rs` that enqueues two outcomes (e.g., `"current"` then `"staged seq 2"`) with two wake bytes before calling `take_line()` once, and asserts that either `"staged seq 2"` is returned directly (latest-wins) or `fd()` remains readable (`poll` returns `POLLIN`) until the second outcome is taken.

### Siblings checked

host/apple-update/src/store.rs:422-437 (clears checking before done, but DispatchQueue.main.async delivers every callback without stranding); host/painted/src/executor.rs:165-180 (drains all wake bytes and drains Vec); host/painted/src/content_region/worker.rs:271-281 (single Option slot); host/painted/src/image/workers.rs:425-429 (stateful revision polling)
