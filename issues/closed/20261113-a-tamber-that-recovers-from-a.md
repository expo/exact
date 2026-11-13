# A tamber that recovers from a refused place does not redraw

**Status:** Closed
**Resolution:** After a check takes a new answer the view is invalidated and its observers told whatever the previous inputs were, so a place that was refused and now answers is drawn.
**Systems:** host/android
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`take` stores `inputs` as null when the answer has `error`, which is a refused place. `check` asks again when that string changes, including from null to a real value, and `take` stores the new colours. It then invalidates the view and notifies listeners only when the previous inputs were non-null. A place that was refused and later answers is kept in memory and not drawn, and `observe` is not called. `onDraw` keeps the fallback until something else invalidates the view.

## Where

`host/android/canvas/src/main/java/dev/exact/canvas/Tamber.kt`, `check` and `take`.

## How to reproduce

Bind a token on a view whose place exact2 refuses, then make that place answer. The stored colour changes and the view does not.

## Direction

Invalidate and notify whenever `take` stores a new answer, including a recovery from null inputs.
