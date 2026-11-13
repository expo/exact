# A failed keyboard reorder leaves the collection interaction pin

**Status:** Open
**Systems:** host/linux, runner
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

A keyboard lift of a grouped row sets the collection's interaction pin, then can return without a session. `refine_collections` can fail, the reorder geometry or the row's base can be missing, the facts can be stale, or `group_begin` can refuse. Each of those returns leaves the pin set. `finish_group` is what clears a pin the session owns, and there is no session.

A pointer lift clears the pin on those same failures. The stuck pin is the list's interaction. The next collection report carries it, and the runner then releases every other list's interaction pin.

## Where

`host/linux/src/presenter/group.rs`, `group_lift` (the pin, then the returns) and `group_key` (Space). `host/linux/src/presenter/contact.rs`, the pointer lift that clears the pin on `Ok(false)` and `Err`. `host/linux/src/presenter/collection.rs`, `set_collection_interaction`. `runner/src/instance/collection/traversal.rs`, `release_other_pins` when an interaction is reported.

## How to reproduce

Focus a grouped grip and press Space before the list has reorder geometry. The lift is refused and no ghost appears. The list's interaction pin stays on that row. A drag already held on another list loses its pin.

## Direction

Clear the interaction pin on every `group_lift` return that does not store a session, as the pointer path does.
