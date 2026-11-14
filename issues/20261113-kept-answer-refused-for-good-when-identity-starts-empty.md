# A kept answer whose identifying argument is still empty at launch is refused for the whole launch

**Status:** Open
**Systems:** runner/src/runner/settlement.rs, runner/src/runner/kept.rs, runner/src/runner/kept_wait.rs
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-first-data-request-waits-for-module-after-first-pixel.md

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

## Landed; remaining

The LLP 1027.005 D3 and D5 amendment (2026-11-13) is built: a kept answer whose differing identifying arguments are all `""`, `none` or read a store reader still on its fallback waits, and their first known match admits it, before or after readiness. The journal says it is waiting, and a known mismatch names each differing argument and the state or resource it reads (`runner/src/runner/kept_wait.rs`, tests in `kept_identity_tests.rs`). Two reviews (Grok 4.7, GPT-6.1 Sol) found these gaps in what landed:

- **A kept module whose source is ready at boot is not held.** Its seed is compared once and refused or shown; it never enters the wait, so an empty identity at boot still refuses it for the launch.
- **A reload loses a waiting entry's sleep history.** The new boot plants a fresh seed (`seed_at_boot`) before the carried waiting entries are restored, and the restore skips a row that already holds a seed, so the carried `slept` bit is dropped. When the identity then matches, the kept value is admitted as `current()` without a fresh answer after the wake (LLP 1161.001 D1). The existing reload test uses a store reader whose source is already ready, which plants no seed.
- **The refusal line mislabels a fresh answer** as "no admitted device answer".

A fix for all three is on branch `fix/kept-answer-waits-for-identity` (`77f28f3cc`), held back because its third review round found problems of its own, which a finished fix has to avoid:

- A store-reading account ready at boot whose ask is still pending shows its compiled `""` with `placeholder` and `stale` both false, so it counts as known, and a module kept for `""` (signed out) is admitted while the real account is unknown.
- The ready-at-boot admission sets `kept_admitted` without `stale`; if the module's request then fails, `carry()` exports the kept seed as an ordinary answer and the next runner skips the module's launch request (D5).
- The same admission leaves `kept_seed` false, so `journal_kept_seeds` never records that the kept answer is shown.
- Its new label calls an admitted kept answer whose ask is still in flight "holding a fresh answer"; `kept_admitted && stale` off the fallback is the standing kept answer.
- Reload with the replacement source unready still loses the sleep bit: restoring the waiting entry has to win over planting the seed.
