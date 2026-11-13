# Fieldnotes' web drive is refused its own SQLite grant on the wasm target

**Status:** Open
**Systems:** apps/fieldnotes, host/web storage
**Severity:** P2
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

`apps/fieldnotes/web/tests/worker.rs`'s `fieldnotes_web_drives_on_its_placement_and_reports_frame_gaps_under_a_backup` fails on a development wasm build of Fieldnotes. The app boots in the worker placement and reports its idle and backup frame gaps, then the drive's page throws:

```
denied: sqlite.open app:/data/fieldnotes.db: no grant covers it; grant `sqlite.open app:/data/fieldnotes.db`, or `sqlite.open app:/data` for every file there
    at authorize (storage-fs.js)
```

`apps/fieldnotes/app.ts` exports that grant (`sqlite.open app:/data/fieldnotes.db`), so the web host's admission does not see the app's grants on this path.

## How to reproduce

At 69668888a on the async lane's Mac (macOS 27, Chrome for Testing from Playwright):

```
EXACT_WEB_LINK=all EXACT_WEB_DIST=/tmp/fn bun host/web/build.mjs fieldnotes-web --wasm
EXACT_REQUIRED=1 EXACT_FIELDNOTES_DIST=/tmp/fn cargo test -p fieldnotes-web --test worker
```

Fails twice of two. The async lane did not reach this assertion before: it built the dist as the JS target, which the test refuses as "not a development Fieldnotes build" (fixed in `scripts/async.mjs`'s prepare step with this issue).
