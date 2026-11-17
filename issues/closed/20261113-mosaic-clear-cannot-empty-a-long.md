# Mosaic Clear cannot empty a long stroke log, and the failure is silent

**Status:** Closed
**Resolution:** Clear deletes the board at once and its strokes in batches of 1,000, each batch scheduling the next, `recent` lists only strokes of the board as it is now, and the view shows `cleared.error`.
**Systems:** apps/mosaic
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`clear` loads every stroke for the board and deletes each one in the same mutation. `paint` inserts one stroke per tap and keeps the log. Convex rejects a mutation once it passes the write cap, and the mutation rolls back. The view shows `painted.error` and never `cleared.error`, so Clear fails with no message and the board stays. The shared lobby keeps every tap, and there is no other way in the app to empty it.

## Where

`apps/mosaic/convex/board.ts`, `clear`. `apps/mosaic/app.contract`, the `cleared` mutation and `problem`.

## How to reproduce

Paint until the stroke log is past Convex's per-mutation write cap, then press Clear. The board is unchanged and no error is shown.

## Direction

Delete strokes in bounded batches. Show `cleared.error` the way `painted.error` is shown.
