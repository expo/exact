# The native bake finds `overlay`, `tools`, `draw` and `surfaces` by matching `app.ts`'s text, so it drops ones the web runs and fails on a commented-out one

**Status:** Closed
**Resolution:** Native and web builds resolve drawing exports through the module namespace, ignore comments, and generate the same canvas declarations.
**Systems:** js/bake/src/lib.rs (`exports`, `bake_in`), host/web-js/build.mjs
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd` the bake's generated entry re-exports the module's optional members only when `exports` (`js/bake/src/lib.rs:1251-1275`) finds the name in the text of `app.ts` after `export function `, `export async function `, `export const ` or `export let `, or inside an `export {` list (`lib.rs:1071-1072` for `draw` and `surfaces`, `1106-1111` for `overlay` and `tools`). It reads no syntax and no other file. The web JS target imports `app.ts` itself (`host/web-js/ts-data.js:13`) and reads `source.overlay` and `source.tools` from the module (`ts-data.js:519`, `535`), so it runs whatever the module exports.

Two results follow.

1. An export the match misses is bundled but not exported: Hermes has no `exact.overlay`, and the resource shows no overlay, with nothing said.
2. A match in a comment makes the entry name `app.overlay`, and the build fails in a file the author did not write.

## The failure

`app.ts` with a working `answer` and one more line, baked with `exact_js_bake::bake`, then `DataSource::overlay("message", [0], "answer", [])` on the loaded module:

```
export const overlay: Overlay = ...              Some("overlaid")
const overlay: Overlay = ...; export { overlay } Some("overlaid")
export { overlay } from './overlay'              Some("overlaid")
export * from './overlay'                        None
export var overlay: Overlay = ...                None
export const { overlay } = made                  None
// export const overlay = (...) => 'x';          the TypeScript does not compile (tsc):
   __exact_entry.ts(8,37): error TS2339: Property 'overlay' does not exist on type
   'typeof import(".../.exact-js-bake-62120-8/app")'.
// export const tools = {};                      __exact_entry.ts(7,26): error TS2339:
   Property 'tools' does not exist on type 'typeof import(".../app")'.
```

A block comment holding `export function overlay` fails as the line comment does. Expected: the three `None` rows apply the overlay, since the module exports one; the commented rows build, since it exports none.

## Verification

Ran a temporary test in `js/bake/tests/` that writes each `app.ts` above beside a one-resource `app.contract`, bakes it with `bake(&dir, &Tools::default())`, loads the result through `Paired::decode` and calls `overlay`. It printed the rows above. Read, not run: the web JS target's side (`ts-data.js:13`, `519`, `535`), and the web build's type-check entry, which finds `overlay` with a regex over the same text (`host/web-js/build.mjs:329`). `draw` and `surfaces` go through the same `exports` and were not run.

## Direction

Write the entry without the text match: read each optional member from the namespace (`(app as { overlay?: unknown }).overlay`) and type it with a conditional type on `typeof app`, so a module without one exports `undefined`, which `__exact_has_overlay` (`js/src/prelude.js:1410`) already reads at load. Delete `exports` and the regex in `host/web-js/build.mjs:329`.

## Related

QUEUE "A wasm worker realm still encodes an answer for a module with no overlay" is about cost at run time. No issue or QUEUE entry names the detection, and no guide says how an optional member must be exported.
