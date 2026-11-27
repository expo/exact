# `Store::write_entry` deletes existing `entries/<digest>` before network download, destroying the signed rollback-floor witness on failure and bypassing `disk::remove_entry`'s reader lock

**Status:** Closed
**Resolution:** Download and verify entries into a temporary directory before touching an existing target, acquire a non-blocking exclusive advisory lock on exact.json before replacing it, and keep valid signed floor witnesses intact during in-place replacement.
**Systems:** update store (`exact-update`)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Halfway failure)

Verification: verified by reading.

At `9c0431fd5`, `Store::write_entry` (`update/src/store.rs:1269-1323`) deletes an existing `entries/<digest>` directory before downloading its replacement and without taking the entry reader lock:

```rust
// update/src/store.rs:1285-1296
let witness_only =
    !regular_file(&target.join("app.plan")) && self.read_envelope(&envelope.digest).is_ok();
if target.exists() && !witness_only {
    std::fs::remove_dir_all(&target)
        .map_err(|e| format!("cannot clear {}: {e}", target.display()))?;
}
let tmp = self.entries_dir().join(temporary_name());
let filled = self.fill(&tmp, envelope, head_url, fetch, embedded_asset);
```

This causes two failures when repairing an existing entry whose `app.plan` is still a regular file (so `validate_contents` fails at line 1281 and `witness_only` at line 1285 is `false`):

1. **Pre-download deletion destroys the signed anti-rollback witness on failure:** `std::fs::remove_dir_all(&target)` at line 1288 deletes `entries/<digest>` — including its verified `exact.json` witness — before `self.fill(&tmp, ...)` at line 1292 fetches replacement files over the network. If `self.fill()` fails (offline, timeout, HTTP error, truncated/corrupt response, or crash), `tmp` is removed and `entries/<digest>/exact.json` is permanently gone. If `record.json` is subsequently lost or truncated (for example, after power loss following an unsynced `write_boot_mark` at `update/src/store.rs:1255-1267`), `signed_floor()` (`update/src/store.rs:1182-1201`, called during `Store::open` at lines 455-461) can no longer recover this sequence's floor, allowing rollback to an earlier sequence.
2. **Bypasses `disk::remove_entry`'s reader lock:** Calling `std::fs::remove_dir_all(&target)` directly ignores the shared advisory lock (`hold_entry`, `update/src/store/disk.rs:86-98`) that secondary reader processes hold on `entries/<digest>/exact.json` (`update/src/store.rs:515-534`) so the owner preserves their running entry's directory. Unlike `collect_entries` (`update/src/store.rs:1418`), which uses `disk::remove_entry` (`update/src/store/disk.rs:103-112`) to acquire an exclusive non-blocking lock before deletion, `write_entry` unlinks `entries/<digest>` out from under an active reader, breaking subsequent lazy `AssetSet::resolve` reads (`update/src/store.rs:212-225`).

### Triggering sequence
1. Stage or boot update entry `entries/<digest>` at seq N (either under `app-decides` as a held entry, or under `next-launch` after `prepare_activation`/`prepare_selected`/`refuse_prepared` clears references while keeping `record.seq = N` and `record.digest = Some(digest)`).
2. Corrupt `entries/<digest>/app.plan` (leaving it a regular file) or any asset under `entries/<digest>/assets/`.
3. Optionally have a second reader process open the store (`Store::open` acquires `hold_entry` on `entries/<digest>/exact.json`).
4. Trigger an update check whose blob fetch fails (e.g., offline or HTTP 500). `admit_check` admits re-downloading the accepted floor bundle (`update/src/store.rs:907-911,944-953`), and `write_entry` removes `entries/<digest>` (including `exact.json`) before `self.fill()` fails.

### Actual vs. expected
- **Actual:** A failed repair download permanently deletes `entries/<digest>` and its signed `exact.json` witness, and deletes the directory even while a reader process holds `hold_entry` on it.
- **Expected:** Per `update/src/store.rs:1269-1271` ("any failure removes the temporary directory and leaves the store exactly as it was"), `update/src/store.rs:451-461,1180-1201` / LLP 1026 D11 (signed `exact.json` witnesses preserve the anti-rollback floor across record loss), and `update/src/store/disk.rs:84-112` (`hold_entry`/`remove_entry` protect active reader processes), `write_entry` must complete and verify `self.fill(&tmp, ...)` before mutating `target`, preserve the signed `exact.json` floor witness across failure, and refuse removing an entry held by a reader process.

### Verification & siblings checked
Verified by reading the exact paths above at `9c0431fd5` (note: `hold_entry` and `remove_entry` are at `update/src/store/disk.rs:86-112`, not `124-135`). Confirmed closed issue `issues/closed/20261102-current-head-leaves-no-rollback-witness.md` only added witnesses for `Current` heads and the `witness_only` branch, leaving the `!witness_only` branch in `write_entry` deleting `target` pre-download.

Sibling sites checked:
- `update/src/store.rs:1418` (`collect_entries`): properly uses `disk::remove_entry(&item.path())` to respect reader locks and keeps `self.record.digest` in `keep` (line 1404) so the floor witness is not collected.
- `update/src/store.rs:1297-1317` (`witness_only` branch of `write_entry`): properly calls `self.fill` before touching `target` and keeps `exact.json` in place, publishing `app.plan` last; no reader can hold a `witness_only` entry because `read_view` requires `regular_file(&plan_path)`.
- `update/src/store/disk.rs:203-224` (`write_atomic`) and `update/src/store/disk.rs:333-337` (`Blobs::put`): both write and sync temporary files before renaming into place.

### Suggested fix & regression test
Move `self.fill(&tmp, ...)` ahead of any mutation to `target` in `Store::write_entry`. When `target.exists()` after `fill` succeeds:
- First verify no reader holds `entries/<digest>/exact.json` (by taking a non-blocking exclusive lock via `disk::try_lock`, returning an error and cleaning up `tmp` if held, so active readers are not disrupted).
- Keep the signed `exact.json` floor witness intact across replacement (either via the same in-place file replacement pattern used by `witness_only` when `read_envelope(&envelope.digest).is_ok()`, or by moving `target` aside only after `fill` succeeds and restoring it if rename fails).

Add integration tests in `update/tests/it/durability.rs` and `update/tests/it/instances.rs`: (a) corrupt a staged entry's `app.plan`, fail the replacement fetch, verify `entries/<digest>/exact.json` still exists and `signed_floor()` refuses rollback after `record.json` loss; (b) hold a reader instance on an entry whose asset rotted and verify a check does not delete the reader's entry directory underneath it.

### Siblings checked

Checked update/src/store.rs:1418 (collect_entries properly uses disk::remove_entry and keeps self.record.digest), update/src/store.rs:1297-1317 (witness_only branch fills tmp first and keeps exact.json intact), and update/src/store/disk.rs:203-224,333-337 (write_atomic and Blobs::put write/sync temporaries before rename).
