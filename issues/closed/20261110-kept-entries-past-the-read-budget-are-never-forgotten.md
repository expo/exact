# A kept answer the host leaves out of its snapshot is never forgotten

**Status:** Closed
**Resolution:** The Linux and Apple hosts name the kept answers they left out of the boot snapshot to the runner (`Runner::forget_unread_kept`), which forgets those a read entry would have been forgotten for: every one in an app that keeps no answers, else one no declared reader seeds, a `transient` reader's included.
**Systems:** runner/src/runner/kept.rs (`obsolete`), runner/src/runner.rs (boot), host/apple/src/store.rs, host/linux/src/system_support.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), from GPT-6 Astra's review of LLP 1156.005 r3 (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

At a cold boot the runner forgets every kept entry that no declared reader seeds
(`kept::obsolete`, `runner/src/runner/kept.rs:100`, called from `runner/src/runner.rs:726`):
a removed or renamed resource, and since LLP 1156.005 D4 one made `transient`. It finds those
entries only in the snapshot the host passes. The hosts stop reading kept values at
`Store::KEPT_TOTAL_BYTES`: Apple leaves the rest out and retries them later
(`host/apple/src/store.rs:811`), and Linux, whose store Windows shares, skips them
(`host/linux/src/system_support.rs:68`). An entry left out is never named to the runner, so it
is never forgotten and stays on disk.

The runner keeps its own entries under that budget (`make_room`), so this needs a store that is
already over budget: two builds of the app writing to one store, or entries left from before a
resource was removed. For an answer made `transient` because it is sensitive, staying on disk
is the failure D4 exists to prevent.

## To see it

Read from the code; not run.

## What a fix is

Give the runner the kept scope's key list separately from the values it reads, and run
`obsolete` (and `kept::transient`) over the keys. Forgetting a key needs no value. Each host
already lists the keys before reading (`KvStore::keys`).
