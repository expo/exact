# The module preparation test omits the prelude’s overlay probe

**Status:** Closed
**Resolution:** Already fixed by a72f3f0a, which gave the test's window `__exact_has_overlay: () => '0'` and the `Function` that `prepare` reads; at cf5fb747 `bun test host/web/request-refusal.test.mjs` prints 48 pass, 0 fail.
**Systems:** host/web/request-refusal.test.mjs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `09cb236d64007f51a668e38926fda6f64dfb6fb5`, the `win` mock in the normalized-grants preparation test provides `__exact_install_storage` but no `__exact_has_overlay` (`host/web/request-refusal.test.mjs:1104`). Production preparation now calls that prelude export at `host/web/module-glue.js:200`. The test stops before checking the grant formatting it is meant to exercise.

## The failure

Run `bun test host/web/request-refusal.test.mjs`. The test `module-glue prepare accepts normalized formatting and exact-only grants` rejects at `prepare`, before its assertions at lines 1121–1122. Expected: preparation succeeds for the normalized declaration.

## Verification

With `~/.bun/bin` first on PATH and `CARGO_BUILD_JOBS=5`, ran the file at this commit. It printed:

```
TypeError: win.__exact_has_overlay is not a function. (In 'win.__exact_has_overlay()', 'win.__exact_has_overlay' is undefined)
    at prepare (host/web/module-glue.js:200:31)
    at host/web/request-refusal.test.mjs:1120:25
47 pass
1 fail
792 expect() calls
Ran 48 tests across 1 file.
```

No source or test was changed.

## Direction

Give the mock the prelude's `__exact_has_overlay` function, returning `"0"` for this fixture without an overlay, and keep the normalized-grants assertions.

## Related

`issues/closed/20261006-web-tests-trust-stale-exact-web-js.md` concerns this file's compiler freshness. `issues/20261110-bake-finds-optional-exports-by-text.md` concerns finding optional exports during a build. Neither records this missing mock function; no matching QUEUE entry was found.
