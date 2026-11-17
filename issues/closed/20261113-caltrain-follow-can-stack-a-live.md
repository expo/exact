# Caltrain Follow can stack a Live Activity, and Delay counts time the host drops

**Status:** Closed
**Resolution:** Follow shows and acts only when no departure is followed or being started, Delay and End only once an id names the activity (so the delay grows only with an update that carries one), and the service alert's Start hides and refuses while a start is out.
**Systems:** apps/caltrain, Apple activity arm
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13
**Related:** https://github.com/expo/exact/issues/127

## What is wrong

Follow, Delay, and End stay enabled together whenever activities are linked. `startTracking` always sends `startActivity`. A second Follow before the first reply creates another Live Activity. `trackedId` is the first active departure, or else the latest reply while `following` is set, or else empty, so Delay and End never address the activity that `first()` did not pick.

Delay and End send that id and still commit `trainDelay` and `following`. Before the reply the id is empty. The arm rejects `updateActivity` and `endActivity` unless the id is already in `Live.activities` (`no alive activity`). `trainDelay` has already grown, and `departureState.at` includes it, so the next update that does land adds the dropped ten minutes again.

Service-alert Start stays on the screen until `alertId` is non-empty. `alertStarted` is set immediately and does not hide the button, so a second tap starts another alert the same way.

## Where

- `apps/caltrain/app.contract` `startTracking`, `delayTrain`, `endTracking`, `trackedId`, and the Follow / Delay / End / alert buttons
- `host/apple/activityarm/ActivityArm.swift` `send`: `startActivity` always calls `Live.request`; update and end throw when the id is not in `Live.activities`

## How to reproduce

On a host with the activity arm linked, tap Follow train twice before the first reply. Two departure activities exist and End reaches one of them. Tap Delay train before the reply: the host returns 400, the on-screen delay still advances, and the next Delay moves the activity by twenty minutes.

## Direction

Show Follow only when no departure is pending or listed, and Delay and End only when `trackedId` is non-empty. Change `trainDelay` only after an update that carried a real id. Hide service-alert Start while a start is in flight, before the reply.
