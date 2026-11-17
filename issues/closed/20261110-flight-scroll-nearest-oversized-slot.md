# A flight's scroll into view on iOS is not `nearest` for a destination larger than its scroller

**Status:** Closed
**Resolution:** The flight brings its slot into view with `IntoView.aligned("nearest", …)`, as `scrollIntoView` does, so a slot taller than its port moves as Chrome moves it: not at all while it covers the port, and to its edge nearer the port otherwise.
**Systems:** host/apple/Sources/ExactKit/IOS/FlightsIOS.swift (Presenter.scrollIntoView), host/apple/Sources/ExactKit/IntoView.swift, LLP 1013.000 D5
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

LLP 1013.000 D5 brings a shared element's destination into view by `scrollIntoView`'s `nearest`. The Apple host has two implementations of `nearest`, at `dd6f5dcd`:

- `IntoView.aligned` (`host/apple/Sources/ExactKit/IntoView.swift:20-25`), used by `scrollIntoView("id")` and fragments. It follows CSSOM View.
- A local `nearest` in the flight's `scrollIntoView(_ slot:)` (`host/apple/Sources/ExactKit/IOS/FlightsIOS.swift:353-357`):

```swift
if lo >= vlo && hi <= vhi { return 0 }
if hi - lo > vhi - vlo { return lo - vlo }
return lo < vlo ? lo - vlo : hi - vhi
```

Its second line aligns the start of any slot larger than the port, wherever the slot is. CSSOM View, "determine the scroll-into-view position", for `nearest` (edge A the start, edge B the end):

> If element edge A and element edge B are both outside scrolling box edge A and scrolling box edge B: Do nothing.
> If element edge A is outside scrolling box edge A and element height is greater than scrolling box height […]: Align element edge B with scrolling box edge B.

The web host calls the browser (`host/web-js/shared.js:148`, `block: 'nearest'`), and the Mac uses `scrollToVisible` (`Mac/FlightsMac.swift:297`).

## The failure

A port showing content 1000 to 1400 (400 points) and a slot 600 points tall:

| slot | CSS, Chrome and `IntoView.aligned` | the flight on iOS |
|---|---|---|
| 900 to 1500, covering the port | no scroll | scrolls up 100 |
| 700 to 1300, its end in view | scrolls up 100, to its end | scrolls up 300, to its start |
| 300 to 900, above the port | scrolls up 500 | scrolls up 700 |

So a card that grows in place into one taller than its scroller, while its top is above the port's top, stays put in Chrome and jumps to its top on iOS before the flight lands. A slot that fits, or one that starts inside the port, is scrolled the same by both.

## Verification

Both functions were copied into one Swift file and run on seven targets (`swift`, no UIKit):

```
SPLIT  taller, covers the port: target 900..<1500  IntoView.aligned 0  flight nearest -100
SPLIT  taller, starts above, ends inside: target 700..<1300  IntoView.aligned -100  flight nearest -300
SPLIT  taller, wholly above: target 300..<900  IntoView.aligned -500  flight nearest -700
3 of 7 cases split
```

The pinned Chromium, asked `scrollIntoView({block: 'nearest'})` for the same three targets in a 400-pixel scroller at `scrollTop` 1000, scrolled by 0, -100 and -500. Not run: a flight on a simulator (none runs on this machine).

## Direction

Delete the local function and call `IntoView.aligned("nearest", …)` on each axis, less the port's start, as `scrollElementIntoView` does (`IntoView.swift:400-401`).

## Related

`issues/20261103-web-history-traversal-forgoes-shared-element-flight.md` is the web's traversal, not this. Nothing in QUEUE.md names the flight's scroll.
