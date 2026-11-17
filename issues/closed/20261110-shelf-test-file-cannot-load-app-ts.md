# `bun test apps/shelf` fails four of its six tests: `app.ts` imports `@exact/nfc`, whose package entry is its manifest

**Status:** Closed
**Resolution:** Already fixed by b54817eb, which mocks `@exact/nfc` before `app.test.ts` imports the data module; `bun test apps/shelf/app.test.ts` at c2e61991 passes all 7 tests.
**Systems:** apps/shelf/app.test.ts, apps/shelf/app.ts, packages/nfc/package.json
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`apps/shelf/app.test.ts` says how it runs (line 1 at `fe4632ed`: "`bun test apps/shelf`") and loads the data module for four of its tests (`app.test.ts:35`, `await import('./app.ts?case=…')`). `apps/shelf/app.ts:5` imports two functions from a native module:

```ts
import { ndefRecords, write, type NDEFRecord, type NDEFRecordInit } from '@exact/nfc';
```

`packages/nfc/package.json` exports only manifests (`".": "./exact.json"`), as every native module package does: a build resolves the names through the manifest, and plain Bun resolves the package to the JSON file.

## The failure

```sh
bun test apps/shelf/app.test.ts
```

Observed: 2 pass, 4 fail, each with

```
SyntaxError: Export named 'ndefRecords' not found in module '<checkout>/packages/nfc/exact.json'.
```

The four are the store's rules: two edits waiting on the first load, progress and finishing, the goal's bounds and the average, and the widget reading the app's library. Expected: the file passes as its first line says. AGENTS.md names `apps/shelf` as the whole app for a builder to read.

## Verification

Ran the command at `dd6f5dcd` and again at `c047f991`, after `bun install --frozen-lockfile`: the result above both times. No async-lane step names the file.

## Direction

Give the test the two names it needs (`mock.module('@exact/nfc', …)` before the import), or move the store rules these tests exercise into a file that imports no native module.

## Related

None found in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`.
