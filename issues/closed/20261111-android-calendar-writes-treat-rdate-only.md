# Android calendar writes treat RDATE-only series as single events

**Status:** Closed
**Resolution:** The write guard reads `RDATE` beside `RRULE` and `ORIGINAL_ID`, so `saveEvent` and `removeEvent` refuse a date-list series with `NotSupportedError`, as they refuse a ruled one.
**Systems:** packages/calendar/android/EventStore.kt
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`row` queries `CALENDAR_ID`, `RRULE` and `ORIGINAL_ID`, and considers a row recurring only when one of the last two is non-null (`packages/calendar/android/EventStore.kt:281`). It omits `RDATE`. `save` and `remove` use that Boolean as their recurrence guard (`:291`, `:322`), then update or delete the event row. The occurrence reader already recognizes `RDATE` (`:224`).

## The failure

An event row has `RDATE` with two dates, no `RRULE`, and no `ORIGINAL_ID`. Call `removeEvent` with its id, or `saveEvent` with that id and no `recurrenceId`. The guard passes and the provider receives a delete or update of the series row. Expected: `NotSupportedError`, as for an RRULE series; recurrence writes are unsupported. Passing an occurrence's non-null `recurrenceId` to save is already refused and is not the failing input.

## Verification

Verified by reading; Android is unavailable. `row` cannot distinguish this series from a single event using its three-column projection. Device check: create an RDATE-only series in a test calendar, call both operations separately, and confirm refusal with every occurrence unchanged.

## Direction

Include `Events.RDATE` in the row query and in its recurring test, matching the occurrence reader. Check save and remove against both recurrence representations.

## Related

The a-calendar-all-day lane changes all-day conversion and occurrence identity; it leaves this write guard unchanged. No matching issue or QUEUE entry was found.
