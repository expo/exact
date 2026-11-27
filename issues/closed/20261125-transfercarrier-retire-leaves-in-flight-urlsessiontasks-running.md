# TransferCarrier.retire leaves in-flight URLSessionTasks running with unversioned taskDescription keys, corrupting restarted transfers and allowing cross-package overwrites

**Status:** Closed
**Resolution:** Namespace TransferCarrier records and folders by (package, id), stamp URLSessionTask descriptions with per-start generation tokens, cancel live tasks on retire and downloadTotal overflow, and drop stale-generation callbacks.
**Systems:** ExactWakeCarriers / @exact/background-transfer (iOS, macOS)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `TransferCarrier` in `host/apple/Sources/ExactWakeCarriers/Transfers.swift` (reached via `packages/background-transfer/apple/Transfers.swift`) lacks task-generation isolation, live-task cancellation on retirement, per-package key scoping, and registration-wide cancellation on `downloadTotal` overflow:

1. **Retire leaves in-flight `URLSessionTask`s running under unversioned keys:**
   - `packages/background-transfer/apple/Transfers.swift:49-54` calls `context.transfers.retire(id: id)` directly on `retireBackgroundFetch` with no `ended` check.
   - `TransferCarrier.handle` (`case "retire"`, `host/apple/Sources/ExactWakeCarriers/Transfers.swift:160-164`) deletes `records[id]` and removes `folder(id)` without checking `r.ended` or cancelling active `URLSessionTask`s. Even if `abortBackgroundFetch` was called first, `case "abort"` (`Transfers.swift:154-159`) cancels tasks asynchronously inside `session(r.discretionary).getAllTasks { ... }`, and `URLSession` delivers `didCompleteWithError` asynchronously afterward.
   - `records` (`Transfers.swift:58`), `folder(id)` (`Transfers.swift:85`), and `task.taskDescription` (`Transfers.swift:246`: `"\(id) \(item.index)"`) carry no generation token.
   - When the app calls `backgroundFetch(id: "sync", ...)` again after `retire` (or `abort` + `retire`), `start` (`Transfers.swift:173-175`) sees `records[id] == nil` and creates a new `Record`, `folder(id)`, and tasks with the exact same `taskDescription`.
   - Any callback from an old task (`received` at `:315-320`, `progressed` at `:325-337`, `downloaded` at `:344-349`, `completed` at `:351-385`) resolves `(id, index)` via `which(task)` (`:411-422`) to the **new** `records[id]`, appending/moving the old response into the new transfer's `body(id, index)`, corrupting its byte counts/status, and marking the new transfer `ended`/`aborted` (and an in-flight `abort`'s `getAllTasks` callback can cancel the newly created tasks matching `belongs(..., to: id)`).
2. **Cross-package record overwrite:**
   - `abort`, `retire`, `record`, and `ids` (`Transfers.swift:155, 161, 165-166`) isolate registrations by `r.package == package`, and LLP 1133.017 D8 documents `context.transfers` as a shared host carrier usable by multiple packages.
   - However, `records`, `preparing`, `offering`, `told`, and `folder(id)` (`Transfers.swift:58-59, 66, 68, 85`) are keyed only by `id`, and `start` (`Transfers.swift:173-175`) checks `(records[id].map { $0.ended } ?? true)` without checking `r.package == package`. If package A has an ended, unretired transfer `"sync"`, package B calling `start(id: "sync", ...)` replaces `folder("sync")` (`:237`) and overwrites `records["sync"]` (`:242`), destroying package A's durable record and response files; conversely, an in-flight `"sync"` in package A causes `start(id: "sync", ...)` in package B to fail with `TypeError`.
3. **Exceeding `downloadTotal` cancels only one request:**
   - In `progressed` (`Transfers.swift:328-331`), once aggregate received bytes exceed `r.downloadTotal`, `r.failureReason` is set to `"download-total-exceeded"` and only `task.cancel()` is called. Sibling `URLSessionTask`s in a multi-request registration keep running, and because `r.failureReason.isEmpty` is now false on their later `progressed` callbacks, they are never cancelled and download to completion.

### Trigger
- From `@exact/background-transfer`, call `backgroundFetch({ id: "sync", requests: [...] })`, then call `retireBackgroundFetch({ id: "sync" })` while it is in flight (or immediately after `abortBackgroundFetch({ id: "sync" })` before `getAllTasks` and `didCompleteWithError` finish), and immediately call `backgroundFetch({ id: "sync", requests: [...] })` again.
- Or have two packages using `context.transfers` share an `id` when one has an ended, unretired registration.
- Or start a multi-request `backgroundFetch` with a `downloadTotal` ceiling that is exceeded while multiple requests are downloading concurrently.

### Actual vs. Expected
- **Actual:** Old tasks remain live and mutate/overwrite the restarted registration's files and state (or an old `abort`'s `getAllTasks` cancels the new tasks); another package can overwrite an ended unretired registration under the same `id`; sibling requests in a multi-request registration continue downloading after `downloadTotal` is exceeded.
- **Expected:** Per LLP 1133.017 D3/D8/D10 (`llp/1133.017-background.rfc.md`) and `Transfers.swift:155, 161, 165-166`, retiring a registration cancels its live tasks (or rejects retirement until ended), every registration carries a unique generation token so callbacks and cancellations from retired/replaced tasks cannot affect a newer registration, registrations and storage folders are scoped by `(package, id)`, and exceeding `downloadTotal` cancels all tasks in the registration.

### Verification & Siblings Checked
- **Verified by reading** `host/apple/Sources/ExactWakeCarriers/Transfers.swift`, `packages/background-transfer/apple/Transfers.swift`, `packages/background-transfer/exact.json`, and `host/apple/tests/ExactKitTests/TransferCarrierTests.swift`.
- **Siblings checked:**
  - Android `packages/background-transfer/android/{Lifecycle.kt,Transfers.kt,Registrations.kt,Jobs.kt}`: uses `Key(namespace, id, generation: UUID)`, cancels jobs and I/O on `retire` (`Lifecycle.kt:703-721`), waits for draining operations before deleting files, and executes requests sequentially so `TotalExceeded` ends the registration immediately (not affected).
  - Web `packages/background-transfer/web/index.js`: returns `NotSupportedError` stubs (not affected).
  - Apple `host/apple/Sources/ExactWakeCarriers/Regions.swift`: has the same unnamespaced cross-package key collision (tracked separately in C122).

### Suggested Fix & Regression Test
- Key `records`, `preparing`, `offering`, `told`, and on-disk folders by `(package, id)` (preserving migration for existing `record.json` directories on `install()`), and include a unique per-start `generation` token (e.g. UUID) in `Record` and `task.taskDescription` (`"<generation> <index>"`).
- In `case "retire"`, cancel all `URLSessionTask`s whose `taskDescription` matches the retired record's `generation` (matching `case "abort"`, which should also filter by `generation`), and drop any delegate callback whose task `generation` does not match the current `records[key]?.generation`.
- In `progressed`, when `downloadTotal` is exceeded, cancel all tasks in `session(r.discretionary)` matching `r.generation`, not just `task`.
- Add unit tests in `host/apple/tests/ExactKitTests/TransferCarrierTests.swift` covering: (1) stale task callbacks after `retire` + restart with the same `id` being ignored, (2) two packages starting/retiring the same `id` without clobbering each other's records or folders, and (3) `downloadTotal` overflow cancelling every request in a multi-request registration.

### Siblings checked

packages/background-transfer/android/{Lifecycle.kt,Transfers.kt,Registrations.kt,Jobs.kt} (generation-keyed, drains on retire, sequential requests: not affected); packages/background-transfer/web/index.js (unsupported stub: not affected); host/apple/Sources/ExactWakeCarriers/Regions.swift (same cross-package key collision, covered in C122)
