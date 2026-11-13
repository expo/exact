# Apple reads an exclusive-midnight all-day end as one extra day

**Status:** Open
**Systems:** packages/calendar
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`instants` sets an all-day `utcEnd` to the start of `endDate`'s day plus one day. `fill` stores the exclusive end minus one second, so an event this module wrote round-trips: `startOfDay` of that instant is the last included day.

EventKit stores an all-day event's `endDate` as the exclusive next midnight. That midnight has no separate case. `startOfDay` of it is that midnight, and adding a day makes the span one day longer. A one-day Calendar.app event is answered as `P2D` and overlaps the next day. Android uses the provider's exclusive end as the span end.

## Where

`packages/calendar/apple/EventStore.swift`, `instants` and `fill`. `packages/calendar/android/EventStore.kt` reads the exclusive end as the span end.

## How to reproduce

Read a one-day all-day event created in Calendar, whose `endDate` is the next local midnight. The answered duration is two days. An event this module saved as one day still reads back as one day.

## Direction

If an all-day `endDate` is exactly local midnight, treat that midnight as the exclusive end and do not add another day. Keep the minus-one-second value only as a last-included-day encoding.
