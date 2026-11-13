# Finishing one update check deletes another check's downloaded entry

**Status:** Open
**Systems:** update/src/client.rs, update/src/store.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At reviewed commit `f6be99af`, two outstanding `Client` checks interfere when
the older result is finished first. The older finish collects the newer
check's complete entry before its result is adopted. The newer finish then
reports corruption and leaves the older bundle selected, although both
downloads verified successfully.

`Client::begin_check(&self)` returns independently owned snapshots
(`update/src/client.rs:303-312`). The existing test
`a_download_cannot_overwrite_a_newer_check_or_concurrent_activation`
(`:497-522`) explicitly exercises two completed downloads awaiting adoption,
but only finishes the newer one first.

`Store::finish_check` calls `collect()` after staging
(`update/src/store.rs:804`). Collection preserves the durable selected,
pending, last-good, running and floor entries (`:1399-1408`). An outstanding
`DownloadedCheck` holds none of those roles and takes no entry lock, so its
entry is removed at `:1418`. The next finish cannot validate it.

## Verification

An external Rust probe used the current `exact-update` through a path dependency
and an in-memory fetch closure. It made two verified downloads for one client,
seq 1 with plan `older` and seq 2 with plan `newer`, then finished them in that
order. No transport or host was started.

```text
finish older: Staged { ..., seq: 1 }
finish newer: Refused("entry ... was written but does not validate")
selected seq: 1
retry newer: Staged { ..., seq: 2 }
```

Command: `cargo run --offline --manifest-path
/tmp/exact-review-20261112/update-overlap/Cargo.toml`. A repeat check repairs the
failure by downloading the collected files again.

Current Apple and Linux host entry points serialize checks with their
`checking` guards; this probe establishes a `Client` API defect, not an observed
failure of their normal foreground polling. The API and its existing overlap
test permit the sequence above without an error at `begin_check`.

## Direction and validation

Keep downloaded entries and their blobs alive until the corresponding check is
adopted or dropped, or explicitly enforce one outstanding check at the client
boundary. Cover both finish orders and a check dropped without adoption; the
newest valid result should stage without refetching, while older results must
still obey the accepted sequence floor.

## Duplicate check

Searched open and closed filesystem issues and all 275 GitHub issues retrieved
on 2026-11-12. The closed `20260904-check-holds-store-lock-across-network.md`
introduced these independent snapshots. The closed
`20261110-update-store-collects-unfinished-download.md` concerns collection at
relaunch, whereas this failure happens when one live check collects another's
completed entry.
