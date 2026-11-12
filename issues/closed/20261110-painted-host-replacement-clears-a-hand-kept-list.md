# The painted host replaces its runner by clearing a hand-kept list, and five filed defects are state the list lacks

**Status:** Closed
**Resolution:** `Presenter::replaced` (`presenter/replace.rs`) destructures every field of the presenter without `..`, so each is said to be the tree's (made anew there, which now covers the callers' lists and the per-tree fields no list named) or kept and why, and a field added later does not compile until it is placed; of the five defects named, four were closed by their own fixes, and the bars' equal-epoch read is `bars-stale-after-reload-at-equal-epoch`.
**Systems:** host/linux/src/presenter.rs (`Presenter::replaced`), host/linux/src/presenter/delivery.rs, runner/src/instance.rs (`Ids`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

A development reload (`reload_module`, `host/linux/src/presenter.rs:612`, at `dd6f5dcd`) and an
activated update (`activate_update`, `presenter/delivery.rs:284`) put a new host, kernel and runner
under the same `Presenter` and call `Presenter::replaced` (`presenter.rs:1386-1413`). The new
runner numbers its views from 1 again (`Ids`, `runner/src/instance.rs:233-252`: "never reusing one
within a runner's life"), so whatever the presenter keeps by view id, node key, kernel epoch or
plan index now names another node, or none.

`replaced()` deals with this by naming what to clear: fourteen fields, and six calls (among them
`hatch_reset` and `top_replaced`). `Presenter` (`presenter.rs:162-348`) has 109
fields. Of the 22 whose type names a `ViewId` or a `NodeKey`, `replaced()` clears nine (`chosen`,
`controls`, `edited`, `fields`, `window_focus`, `menu`, `hovered`, `pointer_held`, `pointer_path`)
and leaves thirteen, among them `scroll`, `focus`, `autofocus_processed`, `deferred_collections`
and `control_contact`. Which of those are kept on purpose is written nowhere. The state inside
`surfaces`, `requests`, `bars`, `hosted` and the statics beside them is not in the list either, and
what a new runner must be told again is in three more places (`restore_time`,
`presenter/preferences.rs:22`; `top_replaced`, `presenter/top.rs:283`; `read_ranges`).

## What it has cost

Five issues filed in this review are one entry each that the list lacks:

- `reload-leaves-new-runner-without-module-handler`: the native handler and bound extensions.
- `reload-keeps-gpu-surface-on-reused-view-id`: a GPU surface, known by view id.
- `picker-answer-after-reload-sets-another-node`: a waiting picker's node.
- `bars-stale-after-reload-at-equal-epoch`: `bars.read`, keyed by the kernel's epoch.
- `hosted-field-face-cache-outlives-its-plan`: a static keyed by a plan's font stack index.

Four of them propose one more entry in `replaced()`; the fifth clears its map at boot.

## Verification

Read `replaced()`, its two callers and the struct; counted the fields and matched each id-typed
field's name against the function's body with a script.
Read the five issues: four name `replaced()` as where the state survives, the fifth the same
reload. Nothing was run here; the failures are those issues'.

## Direction

Turn the default around. Move what belongs to one tree (everything keyed by view id, node key,
epoch or plan index, with the id-keyed state of `surfaces`, `requests`, `bars` and `hosted`) into
one struct that `replaced()` drops and builds anew, and carry over by name only what belongs to the
window or the session (viewport, scheme, clipboard, the display). A field added later is then
cleared unless someone says to keep it.

`QUEUE.md`, "A restart keeps per-node state that a new node with the same id then picks up", says
the same of the Apple host's `Presenter.reset` and adds a second measure that would serve every
host: number views for the session, not the runner, so a carried id cannot match a new node.

## Related

The five issues above and the QUEUE entry, which names `PresenterIOS.swift` and
`PresenterMac.swift` only. No issue or QUEUE entry names `Presenter::replaced` as the common cause.
