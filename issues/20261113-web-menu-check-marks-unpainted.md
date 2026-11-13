# Web menu checks find a checkable menu's marks unpainted

**Status:** Open
**Systems:** host/web menus, host/web/tests/menu-checks.test.mjs
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

`host/web/tests/menu-checks.test.mjs`'s "JS: a checkable menu's rows lead with their marks in every face layout and direction" and its wasm twin fail at line 148: for one face layout the row's mark reports `"painted": false` where the test expects `true` (one entry of the per-layout table differs; `before` and `centred` hold).

## How to reproduce

At 69668888a on the async lane's Mac, with Caltrain's wasm and JS dists built as `scripts/async.mjs`'s prepare step builds them:

```
EXACT_REQUIRED=1 EXACT_WEB_DIST=<caltrain-wasm> EXACT_WEB_JS_DIST=<caltrain-js> bun test ./host/web/tests/menu-checks.test.mjs
```

Seen in one run at that commit. Not looked into: whether it is QUEUE.md's "A web menu opened by a click shows its first row highlighted" (the focused row's wash), which changed what a menu shows when it opens.
