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

The LLP 1027.005 D3 and D5 amendment (2026-11-13) is built: a kept answer whose differing identifying arguments are all `""`, `none` or read a store reader still on its fallback waits, and their first known match admits it, before or after readiness. The journal says it is waiting, and a known mismatch names each differing argument and the state or resource it reads (`runner/src/runner/kept_wait.rs`, tests in `kept_identity_tests.rs`).

The journal's safety rule is also built. Before this change on `ecdaf06fb`, a
runner test with a password control's value as the identifying argument found
the full value in both `journal()` and agent `logs` at boot, before rendered
control masking could apply. Shown, waiting and known-mismatch lines now name
only the resource, differing argument positions, and the declared states or
resources they read. They never render a kept, current or input identifying
value (LLP 1027.005 D3). Admission and waiting behavior are unchanged.

The same rule now covers eviction and exceptional refusal lines. Eviction names
the declaration and fixed total bound, never the encoded entry length, because
that length includes identifying arguments. A live surface publication says it
replaced the seed; an admitted seed rejected with unchanged invalid arguments
says its request arguments are invalid. Neither case is mislabeled as an
argument-count mismatch.

Six review rounds (Grok 4.7, GPT-6.1 Sol) found gaps in it, now fixed (tests in `kept_identity_tests.rs`):

- A kept module whose source is ready at boot is compared and waits for an unknown identity; admitted, it stands over the bake's answer while it is asked, and its show line is journaled once the commit stands and only if it still shows.
- A store reader on its compiled answer whose first answer is still out (`origin` 0) is an unknown identity, so a module kept signed out is not admitted while the account is unknown.
- A kept answer no fresh one has replaced is not carried as an answer, and a refusal names it as an admitted kept answer, not a fresh one.
- A reload keeps a waiting entry's sleep history, also when the new boot plants the same entry as a seed; it carries an admitted kept answer that slept as a waiting entry; and it keeps the stored entry of a kept module that waited without reading the store.

Remaining, from the last Sol round (each looks present before these fixes too):

- **A reload forgets an admitted kept module's entry that has neither slept nor asked.** An ungated kept module admitted before readiness, `reader` false: `carry()` exports no answer, waiting entry or store-reader record for it, so `compatible_on_reload` forgets its stored entry and the replacement shows the placeholder.
- **A wait that starts awake does not learn a later sleep.** Open gate, unknown identity, no compiled answer: the entry waits with `slept` false. Closing and reopening the gate while the identity is still unknown never sets `waiting.slept`, so the identity's match admits the kept value as `current()` with no fresh answer since the wake (LLP 1161.001 D1).
- **An admitted kept answer whose resource has `with` context is never current.** A seed stores only its identifying arguments, and `is_current` (`in_flight.rs`) subtracts the context count from them again, so `current(r)` stays false until a fresh answer lands.
