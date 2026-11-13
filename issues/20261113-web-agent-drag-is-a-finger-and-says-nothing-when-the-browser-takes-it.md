# A web agent `drag` is a finger, a macOS one a mouse, and the web reply does not say when the browser took the touch

**Status:** Open
**Systems:** scripts/agent-drag.mjs (`dragTap`), scripts/agent.mjs (web carrier's held contact), docs/agent-pitfalls.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`tap <t> drag dx dy` on the web is a touch unless `mouse` is written: `dragTap`
(`agent-drag.mjs:125`, `mouse = false` at `:135`) plays the contact through the web
carrier's `down`, `move` and `up` (`agent.mjs:395-430`), which send CDP
`Input.dispatchTouchEvent`. On macOS and Linux the same drag is the mouse. A touch on a
node with `pointerdown`, `pointermove` and `pointerup` handlers and no
`touch-action="none"` is the browser's to pan with. Pointer Events then sends
`pointercancel` and no more moves, so the node's handlers stop partway or do not run. A
mouse drag has no such step, so the same app passes on macOS and fails on the web.

That is the browser's behaviour and the app's to settle (`touch-action`). The driver's
part is that its reply does not say it happened: the drag reports success with no sign of
a `pointercancel` or a scroll. The journal names this case for a reorder grip
(`agent-pitfalls.md:337`, "A finger on a card's ellipsis title") and for a `pan`
(`:1168`, "A `pan` hears nothing from a finger on the web"), not for a node's own pointer
handlers. The pitfall "A test `drag` is a touch unless `mouse` is set" (`:1160`) covers a
hover cleared on touch-up, not a drag the browser took.

Whether this was the reporter's cause was not confirmed: their app was not available, and
no drive was run here.

## The failure

On Exact `fc149195`: two sibling nodes inside a component, each with `pointerdown`,
`pointermove` and `pointerup` handlers that are the component's actions (the move reads
`frame(id)` of an ancestor). `agent macos tap <t> drag dx dy` moves the node and the up's
action runs. On `agent web` the same drag committed nothing for one node and committed
without effect for its sibling. The reporter dropped the flow from the app's test file.

## Fix

When a web drag's contact ends in `pointercancel`, or the page scrolled under it, say so
in the drag's reply and name the node whose `touch-action` let the browser take it, as
the reorder journal line does. Add a line to the drag pitfall: a node with its own
pointer handlers needs `touch-action="none"` for a finger, and `drag … mouse` tests the
pointer path alone.
