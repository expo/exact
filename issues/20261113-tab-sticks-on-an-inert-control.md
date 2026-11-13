# Tab sticks on an inert control when a vertical list exists

**Status:** Open
**Systems:** host/terminal
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

With any vertical collection on the page, Tab and the arrows use `next_in`. That walk skips `display: none` and `disabled`, and returns the next control even when it is `inert`, under an inert ancestor, or not visible. `focus` then returns without moving. `step_collection_focus` still returns true, so the key is consumed and `focusables` never runs.

The same tree with no collection uses `focusables`, which skips those nodes.

## Where

`host/terminal/src/collection/focus.rs`, `focus_entries`, `next_in`, and `step_collection_focus`. `host/terminal/src/collection.rs`, the cursor installed for every vertical list. `host/terminal/src/host.rs`, `input_eligible`, `focus`, and `step_focus`.

## How to reproduce

Put a vertical `list` anywhere on a terminal page, and an `inert` or hidden control in the tab order. Tab onto that control. Focus does not move, and further Tabs stay there.

## Direction

Have `next_in` return only controls `input_eligible` accepts. Have `step_collection_focus` return false when focus did not change, so `step_focus` can use `focusables`.
