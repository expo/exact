# JS runtime tests fail because their stand-ins omit current runtime bindings

**Status:** Closed
**Resolution:** The stand-ins export every name the runtime and ts-data.js import, as constructible functions or stated values, list.js is evaluated with every name it imports bound, and the tests load the runtime's request-send.js and shape.js, give a queue mutation its numeric mode and expect the store's leased retry.
**Systems:** host/web/tests/js-runtime.test.mjs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `09cb236d64007f51a668e38926fda6f64dfb6fb5`, the JS runtime test harness removes imports from `list.js` and evaluates it with a hand-written parameter list that omits `Before` (`host/web/tests/js-runtime.test.mjs:31` to `:34`). The real module calls `Before.push` at `host/web-js/list.js:1037`.

The same file's hand-written `ts-fetch.js` stand-in (`js-runtime.test.mjs:190`) omits `activeCall`, which `host/web-js/ts-data.js:10` imports. These incomplete stand-ins prevent collection and TypeScript tests from reaching their assertions.

## The failure

Run `bun test host/web/tests/js-runtime.test.mjs`. Six collection tests stop at `ReferenceError: Before is not defined`. Ten TypeScript, overlay and storage tests stop while linking modules because `activeCall` is missing. Expected: the harness loads the real modules and runs the assertions.

## Verification

With `~/.bun/bin` first on PATH and `CARGO_BUILD_JOBS=5`, ran the command above at this commit. Bun 1.4.2 printed:

```
ReferenceError: Before is not defined
SyntaxError: The requested module './ts-fetch.js' does not provide an export named 'activeCall'
52 pass
16 fail
436 expect() calls
Ran 68 tests across 1 file.
```

The first collection failure is at the evaluated module's top-level `Before.push`; the TypeScript failures occur before the tested operations run. No source or test was changed.

## Direction

Supply the current bindings in both stand-ins, using the existing import-derived stand-in helper where possible. Include `useFetch` as well as `activeCall` in the explicit fetch stand-in, since `ts-data.js` imports both.

## Related

`host/web/tests/stand-ins.mjs` already derives no-op exports for other mocked modules. Existing focus-event issues mention older runs of this suite but do not record these missing bindings. No matching issue or QUEUE entry was found.
