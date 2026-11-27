# Global REPLACING table leaks open directory FDs and poisons subsequent writes with stale Replaced state after OOM reload or unload

**Status:** Closed
**Resolution:** Move atomicWriteFile replacement tracking from a global static to per-Module HostState cleared in drop_runtime, register tokens only after storageCall admits the call, and reject native recentDocuments/forgetDocument in overlays.
**Systems:** exact-js & exact-data (doc.keep atomicWriteFile replacement tracking)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Unbounded growth)

Verification: verified by reading.

At `9c0431fd5`, `js/src/storage.rs:268-311` keeps in-flight TypeScript `atomicWriteFile("doc:/...", ...)` replacement metadata (`Replaced { kept, directory: Arc<std::fs::File>, relative }`, `data/src/documents/kept.rs:569-573`) in a process-global `static REPLACING: Mutex<BTreeMap<u64, (u64, String, ReplacedFiles)>>`. Entries are inserted synchronously by `js/src/storage.js:484` (`host(6, "kept", "replacing " + path)`) before `atomic.apply(null, arguments)` calls `storageCall`, and are removed only by JS `.then(...)` callbacks (`"replaced "` / `"dropped "` at `js/src/storage.js:494-509` -> `js/src/storage.rs:303-311`).

### Reachable triggers
1. **OOM retirement and reload (or hot replacement):** A macOS or Linux TypeScript app with `doc.keep` calls `storage.fs.atomicWriteFile("doc:/...", data)`. `storage.js:484` inserts `(owner, path, Replaced)` into `REPLACING` (holding an open `Arc<std::fs::File>` directory descriptor) before the write finishes. If the Hermes runtime hits OOM before the `.then` settles, `Module::retire_oom` (`js/src/retirement.rs:13-43`) calls `Module::drop_runtime` (`js/src/lib.rs:854-888`) without draining the VM or purging `REPLACING`. On the next answer, `Module::reload_after_oom` (`js/src/retirement.rs:63-69`) reloads with the same `self.document_owner` (`js/src/lib.rs:738`; hot replacement via `Module::replacement` at `js/src/source.rs:312` also preserves `self.document_owner`). Every subsequent `atomicWriteFile` to that same `doc:/` path then hits `earlier()` (`js/src/storage.rs:291-296`), which matches the leaked entry forever instead of calling `kept::before_write(owner, path)`.
2. **Unload / session teardown:** `Module::unload` (`js/src/lib.rs:838-852`) skips `finish_background` when `worker_retired` is set (lines 843-849) or abandons queued/in-flight ops after the 1 s deadline (`js/src/background.rs:188-196`), and session close (`host/apple/src/documents.rs:116-120`) calls `kept::end(owner)` (`data/src/documents/kept.rs:329-331`) and `documents::forget(owner)` (`data/src/documents.rs:182-187`), none of which remove entries from `REPLACING`.
3. **Call inside an overlay:** `files.atomicWriteFile` (`js/src/storage.js:481-494`) calls `host(6, "kept", "replacing " + path)` (which `js/src/door.rs:136-146` dispatches to `storage::kept` because `state.kept` is `Some(self.document_owner)`) before `storageCall` checks `env.overlaying()` (`js/src/storage.js:115`). Because `Module::overlay` (`js/src/lib.rs:1255`) calls `engine.call("__exact_overlay", ...)` without `engine.drain()` and `exact_js_call` (`js/src/shim.cc:508-554`) does not drain microtasks, calling `atomicWriteFile("doc:/...", ...)` in an overlay performs filesystem syscalls (`kept::before_write`) and leaves the entry live in `REPLACING` after `overlay()` returns (and permanently leaked if the engine retires/unloads before a later `drain()`).

### Actual vs. expected
- **Actual:** (1) Abandoned entries and their open `Arc<std::fs::File>` directory FDs leak for the lifetime of the process; (2) after OOM reload or replacement with the same `document_owner`, `earlier()` (`js/src/storage.rs:291-296`) repeatedly clones the leaked stale `Replaced` (old `kept` record IDs and old directory FD) and never calls `kept::before_write`, poisoning `kept::carry` on subsequent saves to that path; and (3) an overlay call performs host/disk side effects and leaves a live entry in `REPLACING`.
- **Expected:** Per `Module::drop_runtime` (`js/src/lib.rs:854-888`), `js/src/prelude.js:1246-1257` (overlays run synchronously with no effects), and `data/src/documents/kept.rs:536-565` (`before_write` must reflect the file being replaced), replacement tokens must be torn down with the runtime/session and never registered before `storageCall` admits the operation.

### Verification & sibling sweep
Verified by reading `js/src/storage.rs`, `js/src/storage.js`, `js/src/door.rs`, `js/src/lib.rs`, `js/src/retirement.rs`, `js/src/background.rs`, `js/src/source.rs`, `js/src/shim.cc`, `data/src/documents.rs`, `data/src/documents/kept.rs`, and `host/apple/src/documents.rs`.
- **Siblings checked:**
  - Rust native storage (`data/host/src/native.rs:472-478`): holds `Replaced` in a local variable across the synchronous write; no global table or leak.
  - Web (`host/web/documents-glue.js`, `host/web-js/ts-data.js:490`): no `REPLACING` table, and `keptDocuments` goes through `ordinary(...)` which refuses overlays.
  - Linux painted host (`host/painted/`): shares `exact-js` (`js/src/storage.rs` + `js/src/storage.js`) and is affected identically to macOS.
  - Related sibling in `js/src/storage.js:585-603`: native `files.recentDocuments()` and `files.forgetDocument(path)` also call `host(6, "kept", ...)` directly without checking `env.overlaying()` (unlike `storageCall` at line 115 and `files.watch` at line 576).

### Suggested fix & regression test
Move the `REPLACING` map from a process-global `static` onto per-`Module` `HostState` (cleared in `Module::drop_runtime`, `js/src/lib.rs:854-888`), and invoke `host(6, "kept", "replacing " + path)` inside `storageCall`'s `accepted` hook (or after all `storageCall` admission checks — `overlaying`, `current`, `refused`, and `full` — pass) rather than before `atomic.apply(...)` in `js/src/storage.js:481-494`. Add unit tests in `js/src/storage.rs` / `js/tests/it/` verifying that (1) dropping/reloading a `Module` clears pending replacement tokens so the next `atomicWriteFile` re-reads `kept::before_write`, and (2) calling `atomicWriteFile("doc:/...", ...)` inside an overlay records no replacement token.

### Siblings checked

Rust native storage (data/host/src/native.rs:472-478 — stack-local Replaced, unaffected); Web (host/web/documents-glue.js, host/web-js/ts-data.js:490 — no REPLACING table and guards overlays); Linux painted host shares exact-js and is also affected; related sibling in js/src/storage.js:585-603 where native recentDocuments/forgetDocument also bypass env.overlaying().
