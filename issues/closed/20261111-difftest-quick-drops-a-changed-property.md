# Difftest quick drops a changed property sidecar

**Status:** Closed
**Resolution:** Quick selects a changed corpus `.test.contract` sidecar's sibling program, which reads the sidecar's properties, instead of the sidecar the collector drops.
**Systems:** semantics/difftest/src/quick.rs, semantics/difftest/src/lib.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`semantics/difftest/src/quick.rs:195–201` passes a changed corpus `.test.contract` file directly to the corpus loader. The shared collector at `semantics/difftest/src/lib.rs:656–661` excludes that path without selecting its sibling program. The fallback counter fixture is unrelated, so a sidecar-only change can leave its properties unchecked.

## The failure

Change only `semantics/corpus/properties/holds.test.contract`, making an `always` property false. Quick selects that sidecar and the counter fixture; collection drops the sidecar, so the false property does not fail the run.

A minimal fixture is `component App` with `state n = 0` and a text view, plus this sibling sidecar:

```contract
always "deliberately false"
  expect state n == 99
```

Selecting the sidecar should select and check its program.

## Verification

Ran `CARGO_BUILD_JOBS=5 cargo test -p contract-difftest --test fix08_sidecar fix08_ -- --nocapture`. A temporary fixture under `semantics/corpus/edge/` was passed to the actual corpus collector and expectation checker. The test printed `sidecar selected: 0 cases, errors=[]`; selecting its program instead printed `program selected: 1 cases, results=[Err("PROPERTY ... is false at step 0: slot n n0000000000000000")]`. The changed-sidecar selection assertion failed. This exercises the collector input produced by quick; the full quick suite was not run. The temporary fixtures and test were removed.

## Direction

Translate a changed property sidecar to its sibling program before collecting quick's corpus cases.

## Related

[Types check reads property sidecars as programs](closed/20261110-types-check-reads-property-sidecars-as-programs.md) correctly excludes sidecars as independent programs. Quick also needs to follow their dependency on a program. Reported in the f-types-check-sidecars review.
