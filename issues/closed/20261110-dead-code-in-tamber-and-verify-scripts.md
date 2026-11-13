# Dead code in the design server's and verify's scripts: a function only its test calls, three unused imports, a branch that cannot run

**Status:** Closed
**Resolution:** `mapFor`, its two assertions, the three unused imports and the branch that cannot run are deleted; the selection test passes its fixture map to `join` directly.
**Systems:** scripts/tamber-inspect.mjs, scripts/tamber-server.mjs, scripts/tamber-pairing.mjs, scripts/verify.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Four pieces of code in these scripts have no use (`dd6f5dcd`).

1. **`mapFor`** (`scripts/tamber-inspect.mjs:81-88`) parses a source map's bytes and checks its digest. No script calls it: the server does the same work in its own `mapOf` (`scripts/tamber-server.mjs:218-225`), which also caches the parse. Its only callers are `scripts/tamber-inspect.test.mjs:24-25`, a test of `mapFor` itself, and line 592, which uses it to parse a fixture for `join`. The test at lines 24-25 therefore covers a check the server does not run.
2. **`existsSync`** is imported and never used in `scripts/tamber-server.mjs:85`.
3. **`declaredSettings` and `settingValues`** are imported and never used in `scripts/verify.mjs:56`. Line 58 imports `inlineSettings`, the one name used, from the same module.
4. **`if (b.tokens >= rate) buckets.delete(key);`** in `limiter.take` (`scripts/tamber-pairing.mjs:196`) cannot be true. `refill` caps `b.tokens` at `rate` and the line before subtracts 1, so the value is at most `rate - 1`. Every caller passes a finite rate (5, 6 or 10). A full bucket is forgotten by `sweep`, as the function's comment says.

## Verification

At `dd6f5dcd`:

- `git grep -n 'mapFor\b'` prints the definition and the test's import and three calls only.
- `git grep -n existsSync -- scripts/tamber-server.mjs` prints the import alone; `git grep -n 'declaredSettings\|settingValues' -- scripts/verify.mjs` prints line 56 alone.
- With all four deleted, `mapFor`'s two assertions removed and line 592 passing `report.selected.planDigest === map.digest ? map : null` to `join`, these pass as they did before: `bun test` on `scripts/tamber-inspect.test.mjs` (25 tests), `tamber-pairing.test.mjs` (15), `tamber-pairing-server.test.mjs` (18), `tamber-body-limit.test.mjs` (6), `tamber-canvas.test.mjs` (32), `tamber-relay.test.mjs` (32), `tamber-inspect-pages.test.mjs` (1), `verify.test.mjs` (40) and `tamber-server.test.mjs` (30).

## Direction

Delete the three imports and the branch. Delete `mapFor` and its two assertions, and have the test at line 592 pass the fixture to `join`; or make `mapOf` call `mapFor`, so one function holds the rule and the test covers what the server runs.

## Related

No issue or QUEUE entry names these.
