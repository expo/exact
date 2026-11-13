# A check whose record write fails still reports the bundle as staged

**Status:** Closed
**Resolution:** `finish_check` builds the new record in a copy, writes it, and adopts the record and view only after the write succeeds, as `commit_activation` does.
**Systems:** update/src/store.rs (finish_check)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`Store::finish_check` changes the in-memory record and view (`selected`, `pending`, `failures`, `view`, `seq`, `digest`) and only then writes the record (`update/src/store.rs:859-871`). When the write fails (a full disk, a container that became read-only) it returns the error and the in-memory state stays changed. `commit_activation` does the opposite on purpose: it writes a copy and adopts it only on success (`store.rs:1054-1062`; its doc: "A write failure likewise changes neither the in-memory selection nor the running generation").

After the failure the process reports `staged: true` and the new `selected_seq` through `Status` (`store.rs:1075`), which the hosts copy into `state.delivery`. An app that shows "an update is ready, restart to apply" from that fact is wrong: the next launch boots what this one did. A second check in the same process then answers `current` and writes nothing (`store.rs:838`), because memory already holds the new seq and digest.

## The failure

With `exact_update::Client`: download a head (seq 1) with `begin_check().fetch(..)`, make the store directory unwritable (`chmod 555`), call `finish_check`, restore the directory, read `status()`, then open the store again as the next launch does.

```
outcome: refused: cannot write …/update/.tmp-60552-…-15: Permission denied (os error 13)
this process: staged true selected_seq 1
record.json: (none: No such file or directory (os error 2))
next launch boots an update: false
```

Expected: `staged false` and `selected_seq 0`, matching the refusal and the record on disk.

## Verification

Ran those steps as an integration test of `exact-update` (`cargo test -p exact-update --test <name> -- --nocapture`); it printed the lines above. A full disk was not simulated.

## Direction

Build the new record in a clone, write it, and assign `self.record` and `self.view` after the write succeeds, as `commit_activation` does. The `Current` branch at `store.rs:838-844` has the same order and wants the same change.

## Related

None found: no issue or QUEUE entry names `finish_check`'s write order. `issues/closed/20261102-current-head-leaves-no-rollback-witness.md` and `20261102-current-head-sunset-never-reaches-status.md` concern what `finish_check` records, not a failed write.
