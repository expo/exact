# LowerCheck typing misses input handler states

**Status:** Closed
**Resolution:** `Observe.step` and its traced twin dispatch a typed value to an `input` handler where the element has one and to `change` otherwise, as the hosts and `CompSem.cstep` do, so LowerCheck's replay, which calls `step`, reaches the input handler's configurations; `runScript` no longer rewrites the event itself.
**Systems:** semantics/Contract/LowerCheck.lean, semantics/Contract/Observe.lean
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`semantics/Contract/LowerCheck.lean:220` replays typed events with `Observe.step`. Its `.change` case at `semantics/Contract/Observe.lean:263` dispatches only `change`. Unlike the trace replay at `Observe.lean:350–352` and `CompSem.lean:828–831`, this path does not prefer an `input` handler.

## The failure

In `semantics/corpus/edge/input-handler-not-change.contract`, run `type "only-input" "a"`. The action should reach `q == "a"`. LowerCheck's replay refuses the event because that node has no change handler, leaving `q == ""`. The lowering checks consequently omit the reachable input-handler configuration.

## Verification

Generated the existing fixture with `target/debug/contract lean semantics/corpus/edge/input-handler-not-change.contract`, then added temporary Lean evaluations beside it. After `lake build Contract Contract.LowerCheck`, ran `lake env lean corpus/edge/Fix08LowerCheck.lean` from `semantics/`. Evaluating the exact `Observe.step` used by LowerCheck printed `outcome refused` and `only-input has no change handler`, with `q=""`. Dispatching the corresponding `input` event printed `outcome ok`, with `q="a"`. The temporary Lean probe was removed.

## Direction

Give LowerCheck's type replay the same input-first dispatch rule as the trace and component semantics.

## Related

Reported in the f-lowering-corpus review. The input-handler fixture already exists; no issue or QUEUE entry records this replay gap.
