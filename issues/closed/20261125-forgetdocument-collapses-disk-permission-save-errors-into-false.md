# forgetDocument collapses disk/permission save errors into false ENOENT even though the document remains kept

**Status:** Closed
**Resolution:** Return a distinct ForgetError::NotKept vs ForgetError::Save from kept::forget and propagate save I/O errors with their OS error suffix instead of collapsing them into document ENOENT.
**Systems:** exact-js & exact-data-host (kept document forgetDocument error handling)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Swallowed errors)

Verification: verified by reading.

At `9c0431fd5`, `kept::forget(owner, path)` (`data/src/documents/kept.rs:498-528`) distinguishes unkept paths (`ErrorKind::NotFound` via `gone()`, lines 499-514) from persistence failures in `save(store)` (`data/src/documents/kept.rs:263-289`), rolling back `store.records.insert(at, r)` and returning the `std::io::Error` when `save` fails (lines 515-519). However, both native callers discard the error and unconditionally map every `Err(_)` to `document ENOENT`:
- TypeScript (`js/src/storage.rs:312-315`): `.map_err(|_| format!("document ENOENT: {path} is not a document this app keeps"))`, which `js/src/storage.js:73-76, 588-602` parses into `{ kind: "Unavailable", code: "ENOENT", ... }`.
- Rust data sources (`data/host/src/native.rs:496-500`): `.map_err(|_| format!("document ENOENT: {path} is not a document this app keeps"))`, returned in `Outcome::Storage({"error": ...})` (`data/host/src/native.rs:644`) to Rust sources such as `apps/markdown/data/src/lib.rs:442, 511`.

### Reachable trigger
On macOS or Linux, an app granted `doc.keep` calls `storage.fs.forgetDocument("doc:/1/notes.md")` (TypeScript) or sends an `fs.forgetDocument` storage request (Rust, e.g., Markdown reader's Remove from Recents) while `kept-documents.json`'s parent directory cannot be written (e.g., disk full `ENOSPC`, read-only/unwritable directory `EACCES`/`EROFS`, or `EIO`). `kept::forget` fails in `save(store)`, restores the record into `store.records`, and returns `Err(io_err)`.

### Actual vs. expected
- **Actual:** Both native entry points swallow `io_err` and report `"document ENOENT: <path> is not a document this app keeps"` (`error.code === "ENOENT"` in JS), claiming the document is not kept even though `save` failed, the rollback succeeded, and the document remains kept in memory and on disk. Callers treating `ENOENT` as idempotent ("already forgotten") silently believe removal succeeded.
- **Expected:** Only an unkept document (`NotFound` from lookup) should produce `document ENOENT`. Persistence errors from `save(store)` must propagate the real I/O error (so `storageCode` in `js/src/storage.js:80-82` maps e.g. `EACCES` to `"EACCES"` and `ENOSPC`/`EIO` to `"failed"`), matching `kept::forget`'s rollback semantics (`data/src/documents/kept.rs:515-519`), `kept::carry`'s explicit save-error propagation (`data/src/documents/kept.rs:595-600`), and the web host (`host/web/documents-glue.js:154-174`).

### Verification & sibling sweep
Verified by reading `data/src/documents/kept.rs:263-289, 498-528`, `js/src/storage.rs:312-320`, `js/src/storage.js:67-83, 588-603`, `data/host/src/native.rs:484-502, 644`, and `apps/markdown/data/src/lib.rs:437-443, 511`.
- **Siblings checked:**
  - Web host (`host/web/documents-glue.js:154-174`, shared by `host/web/storage-request.js:79-82`, `host/web/storage.js:167`, and `host/web-js/ts-data.js:530`): `drop()` returns `false` only when the record is absent (throwing `notKept` / `ENOENT`), while IndexedDB transaction commit errors propagate directly rather than collapsing into `ENOENT`.
  - `kept::carry` (`data/src/documents/kept.rs:595-600`): propagates `save(store)` errors with `{e}`.
  - All other callers of `kept::forget` in the repo: only `host/painted/src/kept_tests.rs:324` (test).

### Suggested fix & regression test
Distinguish unkept-document lookup failures from `save(store)` I/O failures (either by checking `err.kind() == std::io::ErrorKind::NotFound` in `js/src/storage.rs:313-315` and `data/host/src/native.rs:497-499` — or better, returning a distinct `NotKept` vs. `Save(io::Error)` error from `kept::forget` so even a `NotFound` inside `save` cannot masquerade as unkept) and formatting non-`NotKept` errors with the underlying `io::Error` (e.g. `format!("storage: could not save kept documents: {e}")`, preserving Rust's `(os error N)` suffix for `js/src/storage.js:80-81`). Add tests in `data/host/src/tests.rs` and `js/src/storage.rs` that make the kept-set directory read-only (`0o555`), call `forgetDocument` / `fs.forgetDocument`, and assert that the error is NOT `document ENOENT` (and yields `EACCES` in JS) while the document remains listed by `recentDocuments`.

### Siblings checked

Web host (host/web/documents-glue.js:154-174 via host/web/storage-request.js:79-82, host/web/storage.js:167, host/web-js/ts-data.js:530 — propagates IndexedDB save failures instead of ENOENT); kept::carry (data/src/documents/kept.rs:595-600 — propagates save errors); no other non-test callers of kept::forget exist.
