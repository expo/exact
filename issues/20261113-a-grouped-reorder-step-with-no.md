# A grouped reorder step with no flags traps the wasm page

**Status:** Open
**Systems:** host/web
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

Decode accepts `flags == 0` for every reorder op. `&&` binds tighter than `||`, so the check that limits a step to flags 1 through 4 does not apply when flags are 0. `group_input` for op 22 then indexes the four-step table at `flags - 1`, which traps.

`motion-glue.js` writes `facts.flags ?? 0`. A step packet that omits flags, during an active grouped session, aborts the wasm page. The JS target treats 0 as a later step. Apple returns an invalid-step error.

## Where

`host/web/src/reorder_drag.rs`, `Input::decode` and the flags word. `host/web/src/group_drag.rs`, op 22. `host/web/motion-glue.js`, the flags field. `host/apple/src/arrange_group.rs`, the step match that returns an error.

## How to reproduce

During a grouped reorder on the wasm page, send `reorder-step` with flags omitted. The page traps. The same packet on Apple is refused and the session stays up.

## Direction

Accept op 22 only for flags 1 through 4, and return an error for any other step, as `arrange_group.rs` does.
