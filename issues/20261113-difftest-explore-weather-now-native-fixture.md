# difftest explore diverges from the Lean semantics on weather-now and native-fixture

**Status:** Open
**Systems:** contract-difftest, semantics
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

`cargo run -q -p contract-difftest -- explore contract/corpus apps/*/app.contract` (the async lane's `difftest` step) finds 2 of 112 cases diverging at 69668888a:

- `apps/weather-now/app.contract`: the Lean side refuses at line 2, "the oracle has no answer for current", where the runner goes on (its next event answers `NoHandler { view: 2, event: "press" }`).
- `apps/native-fixture/app.contract`: at line 110, Lean refuses "`navigation` has no press handler" and the runner answers `NoHandler { view: 1, event: "press" }`.

At afde7b25e fourteen other apps diverged the same way as weather-now, each on a module's status source the oracle had no answer for (`getSupportedFormats`, `cameraStatus`, `purchasesStatus`, …); by 69668888a those are reported outside the semantics (`UNSUPPORTED … a native module's, which the runner answers from the module's substitute`). weather-now's `current` is a data source, not a module's, so it is not covered by that change. The `--js-only` divergences are QUEUE.md's "`difftest explore … --js-only` drives every program and 18 diverge".

## How to reproduce

```
cargo run -q -p contract-difftest -- explore apps/weather-now/app.contract apps/native-fixture/app.contract
```
