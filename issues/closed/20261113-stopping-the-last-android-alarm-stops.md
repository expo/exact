# Stopping the last Android alarm stops a newer ring

**Status:** Closed
**Resolution:** AlarmService stops with the latest start id it was delivered, so the system keeps it for a start that is still on its way.
**Systems:** packages/alarms
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`AlarmService` stops itself with `stopSelf()`, which ends the service even when a newer start is outstanding. `onStartCommand` receives a `startId` and does not keep it. `end` calls `stopSelf` when the ringing set becomes empty, and the early path calls it when a start finds nothing to ring.

A second alarm can be started, or be queued to start, after that call. The service then stops and takes the newer ring with it. The notification and the player go away while that alarm is still alerting.

## Where

`packages/alarms/android/AlarmRing.kt`, `onStartCommand` and `AlarmService.end`.

## How to reproduce

Ring one alarm and, as it is stopped, start a second. The second alarm's notification and sound end with the service. The record can still say alerting.

## Direction

Remember each tag's `startId`. Stop with `stopSelf(startId)` for the start that is ending, so a newer start keeps the service.
