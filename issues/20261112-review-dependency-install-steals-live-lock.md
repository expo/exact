# A dependency install can take the lock from another live install

**Status:** Open
**Systems:** build tooling, dependency installation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2
**Related:** `issues/closed/20260904-origin-lock-stolen-while-alive.md`, `issues/closed/20261102-stale-build-claim-can-remove-live-owner.md`

At `f6be99af`, `installedModules` in `scripts/filesystem.mjs:421-423`
treats a lock older than ten minutes as stale before checking whether its
owner is alive. A slow or suspended `bun install` can therefore lose its lock
while still running. Another caller enters `bun install --frozen-lockfile`
against the same `node_modules`, violating the helper's promised exclusion.
One installer can remove the completion marker while the other still changes
the directory, allowing builds to consume an unfinished install.

The age test also bypasses the ownership guarantees that were fixed for
publisher and build-output locks in the related tickets. Those tickets cover
different helpers; this install lock still uses an expiring PID file.

**Verification.** `/tmp/exact-review-20261112/install-lock-probe.mjs` runs the
unchanged `installedModules` function against a private temporary directory.
It creates an unfinished-install marker and a lock naming the live probe's
PID, with its modification time just over ten minutes in the past. Only the child
installer and lockfile-version check are stand-ins. The helper enters the
second install and removes the original lock while its owner is still alive.
The probe uses Bun 1.4.2 and performs no dependency download. Concurrent
directory corruption is a consequence of the broken exclusion, not an
observed result of this probe.

Use a lock held for the entire operation and released by process death,
without expiring a live owner. Keep all paths that reinstall dependencies
under the same lock, including the version-mismatch repair below the main
loop. Verify long-running owners, abandoned installs, simultaneous recovery,
and completion-marker visibility to waiting builds.

No matching open filesystem issue or GitHub issue was found; the two related
closed reports concern other lock implementations.
