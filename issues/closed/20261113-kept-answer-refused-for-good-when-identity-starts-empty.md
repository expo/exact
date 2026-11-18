# A kept answer whose identifying argument is still empty at launch is refused for the whole launch

**Status:** Closed
**Resolution:** Fixed: a waiting kept answer is admitted at its first known match; a reload keeps every keeper's entry, sleep history survives waiting and fallbacks, and current compares only the identifying arguments (LLP 1027.005 D3, LLP 1161.001 D1)
**Systems:** runner/src/runner/settlement.rs, runner/src/runner/kept.rs, runner/src/runner/kept_wait.rs
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261113-first-data-request-waits-for-module-after-first-pixel.md

## What is wrong

A kept answer (LLP 1027 D4) seeds the first frame only when the resource's identifying arguments, evaluated at boot, equal the ones it was kept for. In `settlement.rs:801-827` (at `49ae6357`) a seed whose arguments differ is refused before readiness: `kept::refuse` journals `<name> did not use its kept answer: kept for […], this launch asks for […]` and the seed's `kept_seed` flag is cleared, so it is never reconsidered. When the argument reaches the kept value a moment later, the resource has no seed left and waits for the network.

There is a check for an argument that is not known yet (`identity_unknown`, `kept.rs:218-250`), but it recognises only a store-reading resource still on its placeholder or compiled value. An argument read from a `state` that an action sets at launch, from a resource that reads no store, or from anything else that starts as `""` and becomes the signed-in user's id, counts as known, and its `""` refuses the seed.

## The failure

In the port of a large React Native app (iOS, exact2 `94b8671f`), screens keyed on the signed-in user lost their kept answer on every launch, with the journal line

```
… did not use its kept answer: kept for ["<id>"], this launch asks for [""]
```

Where the app's `""` came from was not reported; the code above shows which inputs the runner treats as unknown. Not reproduced here. `runner/src/runner/kept_identity_tests.rs` covers the store-reader case (`an_identity_read_from_a_resource_on_its_placeholder_does_not_admit_a_kept_answer`) and pins a compiled value of a resource that reads no store as known (`a_compiled_value_of_a_resource_that_reads_no_store_is_a_known_identity`); no test has an argument that reaches the kept value after the refusal.

## Fix

Before readiness, hold a refused seed instead of consuming it: if the resource's arguments come to equal the kept ones before the source is ready (or before its first answer lands), show the seed then. Failing that, widen `identity_unknown` to the inputs that are on their initial value at boot, and say in the journal line which input the empty argument came from, with the repair (read the identity from a store-reading resource, whose kept answer is known at first frame).

## Fixed

LLP 1027.005 D3 and D5 (amended 2026-11-13): a kept answer whose differing identifying arguments are all `""`, `none` or read a store reader still on its fallback waits, and its first known match admits it, before or after readiness (`runner/src/runner/kept_wait.rs`, tests in `kept_identity_tests.rs`). Journal lines name only the resource, differing argument positions and the declarations they read, never an identifying value.

- A reload keeps the stored entry of every resource whose answers the replaced runner kept (`Carried::keepers`: its store readers and what `kept::keeps_answers` names), whether or not it has read the store, asked or slept.
- Sleep history: going idle marks a waiting entry, and a seed let go after a wake passes it to the fallback that replaces it, so neither the fallback nor an entry admitted later is current before a fresh answer since the wake (LLP 1161.001 D1). A reload carries a waiting entry's history.
- `current` compares the row's identifying prefix (`Runner::identity_len`), so an admitted kept answer under `with`, whose entry holds only the identifying arguments, is current.
