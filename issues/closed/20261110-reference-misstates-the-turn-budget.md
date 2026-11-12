# docs/reference.md says a Hermes turn has 100 ms of wall clock and is refused as `Unavailable`; it is CPU time, refused as an overrun and asked again

**Status:** Closed
**Resolution:** The reference says a turn's budget is its own CPU time with a ten-budget wall backstop, refused as `Overrun` with the message as now printed, and that a refused resource is asked again at 250 ms and 1 s before it fails (a mutation or stream message never); the agent guide and `set_budget_ms`'s doc comment say the same.
**Systems:** docs/reference.md, docs/contract-for-agents.md, js/src/lib.rs (doc comment)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, `docs/reference.md:1048-1051` tells an app author:

> On Hermes each turn of an answer has 100 ms of wall clock: its call, or its resumption after a `fetch`, storage step or yield it awaited, until it awaits the next. A turn over it is refused as `Unavailable` (`` `walk` took 401.8 ms, over the 100 ms budget for one turn; … ``)

Since the 2026-10-27 amendments to LLP 1027 D10 the executor differs in three ways:

- The budget is the turn's own time on the calling thread's CPU clock, with a wall-clock backstop of ten budgets (`js/src/turn.rs:1-17`, `159-189`). A turn that waits 900 ms on a blocked host call and works for 20 ms is within it.
- The refusal is `DataError::Overrun` (`js/src/lib.rs:227-232`), envelope kind `Overrun`, not `Unavailable`.
- A resource so refused keeps its value and is asked again at 250 ms, then at 1 s, and fails only on the third overrun (`runner/src/runner/overrun.rs:34`). A mutation's or a stream message's overrun is not asked again.

Neither guide mentions the re-ask: `docs/contract-for-agents.md:1076-1077` says only "A turn over that is refused". An author reading "refused" expects a failed resource and finds one that recovers, or reads `failure(x)` for a refusal that has not happened yet. The doc comment on `set_budget_ms` (`js/src/lib.rs:785-786`) carries the same stale word, `Unavailable`.

## The failure

A resource whose first turn does about 600 ms of work, on macOS with the 100 ms budget, journals:

```
resource m: `memo` took 587.2 ms of its own (CPU) time (588.3 ms of wall), over the 100 ms
budget for one turn; a long loop can `await scheduler.yield()` every few thousand items to go
on in a new turn; asked again in 250 ms (1 of 2)
```

The reference's example line has neither the CPU and wall figures nor the re-ask.

## Verification

Ran a temporary `exact-js` test that boots a `Runner` over a module with `set_budget_ms(100.0)` and a source whose first turn spins; the journal line above is its output. Read `turn.rs`, `lib.rs` and `overrun.rs` for the rest.

## Direction

Rewrite the paragraph at `docs/reference.md:1048` from LLP 1027 D10's text: the turn's own time, the backstop, `Overrun`, the two re-asks for a resource and none for a mutation or a stream's message, with the message as now printed. Add the re-ask to the sentence in `contract-for-agents.md` and fix the doc comment in `lib.rs`.

## Related

`issues/closed/20261006-js-budgets-check-after-work.md` is about evaluation limits checked before work. No open entry covers the documents.
