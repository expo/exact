# TOCTOU between `Store::download` and `Store::finish_check` plus unverified `app.plan` in `admit_check` resurrects a refused corrupt staged bundle

**Status:** Closed
**Resolution:** Verify app.plan and entry metadata for staged next-launch entries in admit_check and refuse finish_check adoption when download skipped write_entry.
**Systems:** update store (`exact-update`, `exact-apple-update`, `exact-painted-update`)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Check, then act)

Verification: verified by reading.

At `9c0431fd5`, two coupled defects in `update/src/store.rs` break corruption recovery and allow a concurrent check to resurrect a corrupt staged bundle:

1. **Unverified staged `app.plan` in `admit_check` (`update/src/store.rs:904-912`):**
   `admit_check` verifies `read_card(&self.entry_dir(&view.sha).join("app.plan"), &view.plan).is_ok()` only when `self.record.pending` matches `envelope.digest` (`app-decides`, added for closed issue `20261113-a-held-app-decides-bundle-with.md`). Under the default `next-launch` policy (and `immediate`), `self.hold` is `false` (`update/src/client.rs:172`), so a staged bundle waiting for activation or relaunch lives in `record.selected` and `self.view` while `self.running` is still the prior generation or entry zero. Line 912 matches `Some(view) => view.sha == envelope.digest` without verifying `app.plan`, so a check against a damaged staged `app.plan` returns `Ok(true)` (`Check::Current`), skips `write_entry`, and never repairs it while the process runs.
2. **TOCTOU resurrection between `download` and `finish_check` (`update/src/store.rs:756-763,770-834,989-1003,1114-1141,1205-1219`, `update/src/client.rs:90-124,310-347`):**
   Both `host/apple-update/src/store.rs:463-488` and `host/painted-update/src/store.rs:260-276` release the `Client` mutex around `CheckRequest::fetch` (`Store::download`) and reacquire it for `Client::finish_check` (`Store::finish_check`).
   - `download()` (`store.rs:756-762`) runs `admit_check` on the pre-fetch snapshot, observes `current == true` (either due to #1 above, or because the staged entry was intact at snapshot time and damaged/refused while the fetch was in flight), and skips `write_entry()`.
   - Before `finish_check()` reacquires the mutex, `prepare_activation()` (`store.rs:989-1003`) hashes `entries/<B>/app.plan`, detects the mismatch, calls `clear_entry_references(B)` (`store.rs:1205-1219`, setting `record.selected = None`, `record.pending = None`, `self.view = None` while preserving `record.seq = B.seq`, `record.digest = Some(B)` and leaving corrupt `entries/<B>` on disk), and persists `record.json`.
   - `finish_check()` (`store.rs:777-829`) re-runs `admit_check()` against the live store: `self.view` is now `None`, `floor == B.seq`, and `accepted == Some(&B)`, so `admit_check()` returns `Ok(false)`.
   - `finish_check()` assumes `download()` wrote a verified entry and falls through to `self.validate(Some(B))` (line 801) -> `read_view(B)` (`store.rs:1122-1141`), which checks only `regular_file(&plan_path)`, `exact.json`, and `assets.json` — never hashing `app.plan` or assets (`validate_contents` is never called). `validate()` succeeds on the untouched corrupt `entries/<B>`, and `finish_check()` writes `record.selected = Some(B)` (or `record.pending = Some(B)`) back to `record.json`, sets `self.view = Some(view)`, and returns `Check::Staged`.

### Triggering sequence
Under `next-launch` (or `immediate`), stage bundle B (seq 2) while running entry zero or bundle A (seq 1), and replace `entries/<B>/app.plan` with a regular file of corrupted bytes.
- **Without concurrency:** Run a check (`deliveryCheck` / foreground check). `admit_check` returns `Ok(true)` and reports `Check::Current` without repairing `entries/<B>/app.plan`.
- **With concurrency (also affects `app-decides` if corruption happens after `download`'s `admit_check`):** Start a check so `request.fetch` sees `current = true` and skips `write_entry`; before `finish_check` acquires the `Client` mutex, invoke `deliveryActivate` (`prepare_activation` refuses B and clears `record.selected`/`self.view`). When `finish_check` runs, it re-stages the corrupt B into `record.json` and reports `Check::Staged`.

### Actual vs. expected
- **Actual:** Checks falsely report `Current` on a damaged `next-launch`/`immediate` staged `app.plan`, and an overlapping `prepare_activation` refusal is undone by `finish_check`, resurrecting the corrupt entry as `Staged`.
- **Expected:** Per `update/src/store.rs:766-769` ("Recheck against the live record after downloading: activation, boot marks, and a newer accepted head must never be overwritten"), `:905-906` ("a plan damaged since staging is written again, not reported current"), and `:987-988` ("A candidate whose plan does not match its card is no longer staged, so the next check of its head writes it again"), damaged staged entries should be re-downloaded rather than reported `Current`, and `finish_check` must never stage an entry whose contents were not written/verified.

### Verification & siblings checked
Verified by reading the exact paths above at `9c0431fd5`. Confirmed closed issue `issues/closed/20261113-a-held-app-decides-bundle-with.md` fixed only the `record.pending` (`app-decides`) arm and did not address `Some(view)` or the `download`/`finish_check` TOCTOU.

Sibling sites checked:
- **Both update hosts affected:** `host/apple-update/src/store.rs:463-488` (iOS/macOS) and `host/painted-update/src/store.rs:260-276` (Linux/Android) both release the `Client` mutex around `CheckRequest::fetch`, exposing the resurrection race.
- **Related refusal path:** `Store::refuse_prepared` / `refuse_running` (`update/src/store.rs:624-645,1221-1234`) also calls `clear_entry_references` while leaving the corrupt `entries/<B>` on disk, so an in-flight check that skipped `write_entry` can similarly resurrect a boot-refused entry.

### Suggested fix & regression test
1. In `admit_check` (`update/src/store.rs:912`), verify `read_card(&self.entry_dir(&view.sha).join("app.plan"), &view.plan).is_ok()` (or `self.validate_contents(view).is_ok()`) in the `Some(view) if view.sha == envelope.digest` arm as well as for `record.pending`.
2. In `Store::download` / `DownloadedCheck` / `Store::finish_check`, either record whether `download()` wrote/verified the entry (refusing in `finish_check` if `!wrote` and live `admit_check` now returns `Ok(false)`), or call `self.validate_contents(&view)` in `finish_check` (line 801) before adopting `record.selected`/`record.pending` whenever `download()` did not write the entry.
3. Add two tests in `update/tests/it/durability.rs` / `update/src/client.rs`: (a) rotting a `next-launch` staged `app.plan` causes the next check to re-stage/repair it rather than return `Current`; (b) starting `begin_check().fetch()` while B is staged, calling `prepare_activation()` on corrupt B, and then calling `finish_check()` must not re-stage the corrupt B.

### Siblings checked

Checked both update host wrappers: host/apple-update/src/store.rs:463-488 (iOS/macOS) and host/painted-update/src/store.rs:260-276 (Linux/Android) both release the Client mutex around CheckRequest::fetch. Also checked Store::prepare_selected / refuse_prepared / refuse_running (update/src/store.rs:580-645,1221-1234), which also leave corrupt entries/<B> on disk and can trigger the same finish_check resurrection.
