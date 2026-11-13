# The native-start dev tests throw before they run

**Status:** Open
**Systems:** host/web-js
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

`dev-native-start.test.mjs` calls `fileURLToPath` while the module is loading and does not import it. `fileURLToPath` is not a Bun global, so loading the file throws `ReferenceError`. The three tests never run: a 503 while the resident starts, a failed start that stays 500 until an edit, and a failed phase that stays alive.

## Where

`host/web-js/dev-native-start.test.mjs`, the `ROOT` line. `host/web-js/dynamic-chunks.test.js` imports `fileURLToPath` from `node:url`.

## How to reproduce

Run `bun test host/web-js/dev-native-start.test.mjs`. The file fails at load with `ReferenceError: fileURLToPath is not defined`. None of the three tests run.

## Direction

Import `{ fileURLToPath }` from `node:url` beside the other node imports.
