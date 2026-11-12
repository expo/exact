# The difftest JS driver writes no `ext-*.js` and has no arms sheet, so `explore --js-only` cannot compare 21 of its 111 programs

**Status:** Closed
**Resolution:** The driver writes an `ext-<name>.js` without a web implementation for each extension in `modules.flag`, gives a `<style>` the sheet the arms rules need, and puts `structuredClone` in its context, so `explore --js-only` drives every program.
**Systems:** semantics/difftest/js/drive.mjs, host/web-js/dom.js, scripts/async.mjs (the `difftest-js` check)
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at `dd6f5dcd`.

The async lane's `difftest-js` check runs `difftest explore contract/corpus apps/*/app.contract --js-only` (`scripts/async.mjs:317`). The driver compiles each program, bundles it over a copy of the runtime and runs it on the render DOM (`host/web-js/dom.js`). Two things the compiler emits have no counterpart there:

1. **Extension chunks.** A plan with native modules or package views gets a loader `()=>import("./ext-<name>.js")` for each extension (`host/web-js/src/facts.rs:133-142`). `host/web-js/build.mjs:223-235` writes those files. The driver's `build` (`semantics/difftest/js/drive.mjs:124-132`) does not, so the bundler cannot resolve them. `Bun.build` throws before the driver's own `r.logs` line (`:130`), so the error says only `Bundle failed`.
2. **The arms sheet.** A node whose style rows are lowered to classes calls `Pc` (`host/web-js/src/lower.rs:270`), which builds its rules through `Arms.sheet` of a `<style>` it makes (`host/web-js/lower.js:22-23`). The render DOM's elements have no `sheet`, and the driver does not set `__exactRender`, under which `Pc` writes inline (`lower.js:50`).

## The failure

```
$ cargo run -q -p contract-difftest -- explore contract/corpus apps/*/app.contract --js-only
ERROR-JS apps/auth-fixture/app.contract: explored: AggregateError: Bundle failed
DIVERGE-JS apps/llp/app.contract: explored at line 1:
  rust: outcome ok
  js:   outcome refused
  js # boot refused while building the view: TypeError: undefined is not an object (evaluating 'Arms.sheet.cssRules')
difftest js: 111 cases: 78 agree, 9 diverge, 5 outside the JS target, 1 refused by the compiler, 18 driver errors   (exit 1)
```

Seventeen of the driver errors are `Bundle failed`: the seventeen apps whose compiled module holds an `ext-` loader. Four of the divergences are the `Arms.sheet` one (`apps/llp`, `apps/messages-stress`, `apps/native-fixture`, `contract/corpus/tabs.contract`). Expected: each program is driven and compared.

The errors hide what the comparison would find: once `apps/agents` bundles, its exploration reaches the defect of `issues/20261110-web-js-path-refusal-in-a-binding-poisons-the-app.md`.

## Verification

The command printed the above. Each cause was confirmed with a temporary edit to `drive.mjs`, reverted afterwards:

- Writing `export const implementation = null;` to `ext-<name>.js` for each extension in the compiler's `modules.flag`: no `Bundle failed` remains (`86 agree, 15 diverge, 7 outside the JS target, 2 driver errors`).
- Also compiling with `EXACT_JS_NO_LOWER=1`: no `Arms.sheet` line remains (`89 agree, 12 diverge, 7 outside the JS target, 2 driver errors`).

The run still fails after both, for causes not diagnosed here: a module call refused for a missing grant (`contacts-calendar-fixture`, `geofence-fixture`, `photo-editor`, `recorder`), `root.contains is not a function` (`shelf`, `unsaid`), `measure.getBoundingClientRect is not a function` (`native-fixture`), the `agents` poisoning above, and `metronome`, `orientation-fixture`, `delivery`, `grouped-touch`, `inline-image` and `share`.

`difftest corpus --js-only` reaches neither cause.

## Direction

In the driver's `build`, write a chunk for each extension `modules.flag` names, as `build.mjs` does for an extension with no web implementation. For the sheet, compile the driver's programs with lowering off (`EXACT_JS_NO_LOWER`, which `lower-equal.test.mjs` uses) or give the driver's style element a sheet.

## Related

`issues/20261110-js-driver-loses-linked-commands-and-reads-no-select-value.md` covers two other gaps of this driver, in the corpus half of the check. `QUEUE.md`'s "Eight apps' explore runs diverge at their first host operation outside the oracle" is about the runner against the semantics, not this driver.
