# Fieldnotes' iOS inbox drive deletes shares that predate the test

**Status:** Closed
**Resolution:** Each run makes its own inbox directory (in the group container on the simulator), names it to the app with `EXACT_AGENT_SHARE_INBOX` and removes only that directory, so the installed app's inbox is never read or cleared.
**Systems:** Fieldnotes share-inbox fixture, iOS test isolation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, running `bun apps/fieldnotes/share-inbox.drives.mjs ios`
against a simulator with pending Fieldnotes shares deletes those shares
before the test opens the app. This includes both completed shares and
an extension's incomplete directory. The fixture does not preserve or
restore them, and they are lost even if the subsequent app launch fails.

`apps/fieldnotes/share-inbox.drives.mjs:45` resolves the installed app's
real `group.com.exact.fieldnotes` container and uses its `shares` directory.
Lines 84–87 recursively remove every non-hidden entry in that directory.
The test's unique `storage: fn-<run>` at line 92 does not isolate this
App Group inbox; it is selected and cleared before the session exists.
On macOS the script instead uses a fresh temporary inbox, so that branch
does not have this problem.

Verified with the script's filesystem operations against a controlled
temporary App Group directory. A copy of the actual script changed only
its child-process and agent/checker dependency bindings: fake simulator
queries returned the temporary directory, and `open()` threw immediately
to stop before any app launch. A pre-existing directory containing a
complete `share.json` was present before importing the script and absent
when `open()` was reached:

```json
{"originalPresentBefore":true,"originalPresentAfter":false,"stoppedBeforeAppLaunch":true}
```

The probe is `/tmp/exact-review-20261112/share-inbox-cleanup-probe.mjs`;
run it with the pinned Bun 1.4.2. Its successful run uses only temporary
files and stand-ins; no real simulator inbox or app was used.

Use a dedicated fixture application/App Group or a disposable simulator
whose ownership the test establishes before clearing it. If a real
installed app must be used, refuse an occupied inbox without deleting it
and account for a concurrently running share extension. Remove only test
directories owned by this run. Validate that unrelated completed and
in-progress shares survive both a successful drive and a launch failure.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot. The closed session-registry issue
`20261110-session-registries-keyed-by-released-address.md` concerns inbox
observer registration, not fixture deletion; no matching report was found.
