# A save during a dev rebuild is treated as already compiled

**Status:** Open
**Systems:** host/web
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A request that finds a source newer than the build starts one rebuild and stores the start time in `since`. Freshness is `newest(app) <= max(since, receipt())`, and `receipt` is the mtime of `.exact-build.json`, written when that build finishes, so it is later than `since`. A save during the compile is older than the receipt. A second request while the build runs only awaits the same promise.

The process compiled the tree it saw at spawn. The later save is absent from the served files, and every later page request still treats the tree as fresh, until some other file changes.

## Where

`host/web/serve.mjs`, `rebuildOnChange`.

## How to reproduce

Open the dev server, save a source, and save it again before that rebuild exits. Reload. The page is the first save. Further reloads stay there until a different file changes.

## Direction

When a build finishes, if any source mtime is newer than the newest mtime sampled at its start, build once more. Do not treat the output marker's mtime as covering edits that arrived after the build spawned.
