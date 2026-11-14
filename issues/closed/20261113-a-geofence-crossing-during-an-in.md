# A geofence crossing during an in-flight wake is dropped

**Status:** Closed
**Resolution:** `delivered` names the transition the package raised and the carrier clears only that one; a region crossed while in a wake is offered again when the wake ends.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`crossed` writes the latest enter or leave into `pending` and calls `offer`. `offer` skips a name already in `offering`. The in-flight handler then calls `delivered`, which sets `pending` to nil. That clears the newer transition. When the wake finishes, `offering` is cleared and nothing offers the name again. `ready` runs when a module becomes ready, not after each delivery.

The store's `inside` flag already matches the lost event, so `list` looks current and the app never hears the crossing. The next opposite crossing can still fire.

## Where

`host/apple/Sources/ExactKit/Regions.swift`, `crossed`, `offer`, and the `delivered` op. `packages/geofencing/apple/Geofencing.swift`, `deliver`, which calls `delivered` after a completed or unheard raise.

## How to reproduce

Enter a region, and leave it before that enter's wake handler returns. The app hears the enter and never hears the leave.

## Direction

Clear `pending` only when it is still the event that was raised. When the wake finishes, offer any name whose `pending` was written while that name was in `offering`.
