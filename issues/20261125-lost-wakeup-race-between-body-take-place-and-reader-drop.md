# Lost wakeup race between body::take_place and Reader::drop permanently stalls queued exactBodyFrom uploads

**Status:** Open
**Systems:** exact-executor (native desktop/mobile upload queue and file-body reader pool)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `body::take_place` (`host/executor/src/body.rs:376-386`) and `Reader::drop` (`host/executor/src/body.rs:391-400`) synchronize on the `READERS.running` atomic counter separately from the `READERS.waiting` mutex:
1. `take_place(shared)` calls `READERS.take()` at `body.rs:377` (atomic CAS checking `running < MAX_READERS` = 4) without holding `READERS.waiting`. When `None` is returned, it acquires `READERS.waiting.lock().unwrap()` at `body.rs:379` and inserts `Arc::downgrade(shared)` at `body.rs:382` without re-trying `READERS.take()` under the lock.
2. `Reader::drop` decrements `READERS.running` via `fetch_sub(1, Ordering::AcqRel)` at `body.rs:393-395`, then acquires `READERS.waiting.lock().unwrap()`, drains it (`body.rs:396`), drops the lock, and calls `upload::start(&shared)` (`body.rs:398`).

**Triggering sequence:** A TypeScript `fetch(url, { method: "POST", exactUploadProgress: true, exactBodyFrom: "app:/tmp/..." })` (or Rust `Request` with `upload_progress` and `body_from`) is admitted via `admission.rs:245` -> `upload::queue` (`host/executor/src/upload.rs:195-224`) -> `upload::start` (`upload.rs:232-274`). Suppose all 4 process-wide `READERS` slots are in use (e.g., by another session's `Core` in a multi-session host, or by background `exact-body-from` reader threads from `body::resolve` / `body::prepare` / `async_file::Machine::file` / `async_body_to::Machine::file_reply` finishing after their own requests completed, timed out, or were cancelled). While `upload::start` calls `body::take_place(shared)` (`upload.rs:259`):
1. `READERS.take()` sees `running == 4` and returns `None`; the thread pauses before `body.rs:379`.
2. All 4 active `Reader`s drop concurrently: each `Reader::drop` decrements `running` down to `0` and drains an empty `READERS.waiting`, waking nothing.
3. `take_place` resumes, locks `READERS.waiting`, pushes `Arc::downgrade(shared)`, and returns `None`; `upload::start` breaks (`upload.rs:261`), leaving the upload queued at the front of `budget.queue`.

**Actual vs. expected:** With `READERS.running == 0` and no other active requests on `shared` to trigger `complete` / `place_freed` / `async.rs:797` and no other active `Reader` in the process, nothing ever invokes `upload::start(shared)` again and the queued upload (and any uploads behind it in the FIFO `budget.queue`) stalls permanently. Expected: per `host/executor/src/body.rs:374-375` ("or `None`, in which case `shared` is started again when a place is given back") and `host/executor/src/upload_burst_tests.rs:122-125` (LLP 1133.008.000 D4), an upload that waits for a reader place must reliably start once a place is released.

**Verification and sibling sweep:** Verified by reading `host/executor/src/body.rs:323-401`, `host/executor/src/upload.rs:195-274`, `host/executor/src/stream.rs:56-67`, `host/executor/src/lib.rs:653, 952`, `host/executor/src/async.rs:797`, `host/executor/src/admission.rs:245`, and `host/executor/src/forget.rs:313` — confirming there is no timer or fallback polling `budget.queue`. Checked sibling wait/pool mechanisms in `host/executor/src/`: `body::acquire` (`body.rs:407-421`) and `body::reader` (`body.rs:625-648`) fail fast instead of waiting; `workers.rs`, `timer.rs`, and `lib.rs` (`shared.ready` Condvar) all check their condition under the same mutex as the notifier. `body::take_place`/`Reader::drop` is the only split atomic-counter + mutex-waitlist pattern.

**Suggested fix and regression test:** Protect both `running: usize` and `waiting: Vec<Weak<Shared>>` under a single `Mutex` inside `Readers` (eliminating the split between `AtomicUsize` and `Mutex`), updating `running` and `waiting` together in `take`, `take_place`, and `Reader::drop` while still releasing the mutex before calling `upload::start(&shared)` to preserve lock order (`shared.state` -> `shared.uploads` -> `READERS` lock). Add a deterministic regression test in `host/executor/src/upload_burst_tests.rs` (or `body_tests.rs`) that pauses `take_place` after checking capacity while the held `Reader`s drop, then resumes and asserts the queued `exactBodyFrom` upload starts and completes.

### Siblings checked

host/executor/src/body.rs (acquire and reader fail fast without waiting); host/executor/src/workers.rs, timer.rs, and lib.rs shared.ready Condvar (all check state under the same mutex as wakeup).
