# A macOS agent `tap <target> hover` delivers no `pointermove` to the target or its ancestors

**Status:** Closed
**Resolution:** Does not reproduce at 036b4141e: under `agent macos`, `tap child hover` on a child of a box with `pointermove` runs the box's handler with the pointer's `clientX` (74, the child's middle), and `tap parent hover` then moves it (174), with or without a `pointerdown` on the box and after a plain tap; the reply says `delivery platform`.
**Systems:** host/apple/Sources/ExactKit/Mac/AgentMac.swift (`tap`, hover branch), host/apple/Sources/ExactKit/Mac/MouseChainMac.swift
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`tap <target> hover` is documented as moving the pointer onto the target (LLP 1005 §3,
`scripts/agent.mjs` usage). On macOS a `pointermove` handler on the target's parent sees
nothing after it. So a drive cannot put the pointer somewhere before `tap … pinch` or a
modified wheel, and an app that follows the pointer from `pointermove` cannot be driven.

The hover branch (`AgentMac.swift:739-753`) builds a `mouseMoved` event at the target's
middle and calls `presenter.trackPointer(event)` and then `presenter.flushHoverMove()`.
`trackPointer` (`MouseChainMac.swift:731`) walks from the hit leaf to the nearest node
with a `pointermove` handler and calls `pointerHovered`, which hit-tests again and queues
the move with `hoverMoved`; `flushHoverMove` (`:641`) sends what was queued. Reading the
code did not show where the move is lost. Candidates to check: `trackPointer` returns
early when `pointerHeld` or `pointerSource` is set (`:748`), `pointerHovered` returns when
its own hit-test reaches a different node first (`:454-466`), and the reply for a
non-canvas target carries no `delivery`, so nothing says whether a move went out. Not
reproduced here (this checkout runs on Linux).

## The failure

On Exact `9e53578f`, `agent macos`: a box with `pointermove=track` (setting state from
`e.clientX`) and a child, then `"tap child hover" "state x"`. `x` did not change. The
same expectation holds for the web, where the driver sends CDP's `mouseMoved`.

## Fix

A host test: a parent with `pointermove`, `tap child hover`, and the parent's handler
runs with the pointer's position, followed by `pointerenter` on the way in. Then find
which guard drops it. The reply should say what was delivered (`delivery`), as the
other tap forms do.
