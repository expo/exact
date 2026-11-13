# An old Android alarm timeout stops a repeated ring

**Status:** Closed
**Resolution:** The ringing service keeps each ring's ten-minute stop by tag and cancels it when that ring ends, so a ring started again after a snooze or repeat has only its own limit.
**Systems:** packages/alarms/android/AlarmRing.kt
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

Every ring posts a delayed stop that checks only whether its tag is in `ringing` (`packages/alarms/android/AlarmRing.kt:306`). `end` removes that tag but leaves its callback queued while another alarm keeps the service alive (`:344`). Snooze creates a new record/code (`:133`), but the old callback does not compare that identity. All callbacks are cleared only when the service is destroyed (`:367`).

## The failure

B rings at t=0; A rings at t=1 s. Repeat A at t=2 s with `snooze=60000`. A rings again at t=62 s. B keeps the service alive through the repeat; when B times out at t=600 s, A keeps it alive. A's first callback sees A's tag at t=601 s and stops the new ring, 61 seconds before its own t=662 s timeout. Expected: each ring gets its own ten-minute limit.

## Verification

Verified by reading; Android is unavailable on this machine. `onStartCommand`, `snooze`, `end` and `onDestroy` still form the sequence above. Device check: run the two-alarm sequence, verify A is still alerting at t=601 s, then ends at t=662 s. No Android execution is claimed.

## Direction

Capture the ring's code or another incarnation identity in its timeout and stop only that ring; alternatively cancel the tag's old callback when its ring ends.

## Related

`issues/closed/20261110-android-alarm-snooze-removes-its-countdown-notification.md` gives a repeated countdown a new notification code. The timeout still matches only the tag.
