# On the web a finger outside the scroller keeps its scroll drag open, so `scrolldragend` is lost and the next `scrolldragstart` with it

**Status:** Closed
**Resolution:** `scrollDrag` (now `host/web/navigation/scroll.js`, which both web targets use) counts and follows only the touches whose target is inside the scroller, so a finger elsewhere neither holds a drag open nor makes a later scroll one.
**Systems:** host/web/navigation.js (`scrollDrag`), host/web/glue.js, host/web-js/rt.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`scrollDrag` (`host/web/navigation.js:1500`) counts the fingers down as `e.touches.length`, and ends a drag when a lift leaves none. `TouchEvent.touches` is every contact on the surface, wherever it started ([Touch Events, `touches`](https://www.w3.org/TR/touch-events/#dom-touchevent-touches)), while the scroller's listeners hear only events whose target is inside it.

So when the scroller's finger lifts while another finger is down elsewhere on the page, `lift` sees `e.touches.length === 1` and takes `if (e.touches.length) return follow(e)`. The tracker now waits for a lift the scroller is never sent: `down` stays 1, and `dragging` stays set if a drag was open. Two results:

- A drag in progress gets no `scrolldragend`. The next drag fires no `scrolldragstart` (the tracker is already dragging), and its lift fires the `scrolldragend`.
- With no drag in progress, the next scroll of any kind fires `scrolldragstart`, because `scroll` begins a drag whenever `down` is not 0. The comment on `scrollDrag` says "a programmatic scroll is no drag".

`docs/contract-grammar.md:1796-1800` says the two events "bracket a drag" and that the end "comes as the finger lifts".

## The failure

A page with a 100 px header above a scroller, and `scrollDrag` from the source on the scroller. Touches sent with CDP's `Input.dispatchTouchEvent` in the pinned Chromium (`hasTouch`), two touch points where a row names two fingers:

| drive | calls | the scroller's `touchend` |
|---|---|---|
| one finger drags and lifts | `start`, `end 0.00 0.41` | `touches=0` |
| a finger lands on the header during the drag; the scroller's finger lifts, then the header's | `start` | `touches=1 targetTouches=0` |
| one finger drags and lifts | `end 0.00 0.52` | `touches=0` |
| one finger drags and lifts | `start`, `end 0.00 0.52` | `touches=0` |

Expected: rows two and three each give `start` then `end`.

On a fresh page: a finger rests on the header, another taps the scroller and lifts, then the header's lifts. No calls, which is right. Then, with no finger down:

| drive | calls |
|---|---|
| `scroller.scrollTop += 100` | `start` |
| one finger drags and lifts | `end 0.00 0.50` |

Expected: nothing for the programmatic scroll (as before any touch, where the same line gives no call), then `start` and `end`.

## Verification

Ran both sequences in Playwright's pinned Chromium and read back the calls and the `touches` and `targetTouches` counts of each `touchend` the scroller heard; the tables are the output. Safari and a phone were not run. `host/web/tests/scroll-drag.test.mjs` covers a second finger only as events delivered to the scroller's own listeners.

## Direction

In `follow` and `lift`, count only the touches whose `target` is inside `el`, so a contact elsewhere neither holds the drag open nor becomes the finger whose velocity is read.

## Related

QUEUE's two 2026-11-10 entries on scroll drags are Android's and the iOS simulator's. No issue names the web's tracker.
