# Android alarms can ring after their notification is blocked

**Status:** Open
**Systems:** Alarms package, Android permission and restore lifecycle
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, restoring an overdue Android alarm can start its ringing
service while notifications or the alarm channel are blocked. An existing
ring also has no listener that ends it when its channel becomes blocked.
Both paths can leave sound running without the notification's Stop
control. The normal alarm-delivery path checks this condition.

`packages/alarms/android/AlarmRing.kt:177-185` restores overdue
`countdown` and `scheduled` records, and every `alerting` record, by
calling `ring` directly. `ring` at lines 207-214 starts the foreground
service without checking notification availability. Its service starts
audio at line 305. By comparison, `due` at lines 198-204 checks both
`areNotificationsEnabled()` and the alarm channel before permitting
the ring. `AlarmSystemReceiver` at line 242 invokes restore after
boot and package replacement, without a live app session that could
first clear denied alarms through `alarmStatus`.

A concrete sequence is:

1. Schedule an alarm with both permissions granted, then close the app.
2. Disable the app's notifications or its alarm channel before the alarm
   is due, leaving exact-alarm permission granted.
3. Power off before the due time and boot after it.
4. The boot receiver restores the persisted record and starts its sound
   without its notification's Stop action being available.

LLP 1133.030 D7 requires the Stop invariant wherever the receiver runs:
an alarm whose notification cannot be shown must be removed, as a
revoked alarm is. This is a permission recheck required after a process
or device lifetime change, independent of ordinary scheduling admission.

The same design requires a running service to observe
`ACTION_APP_BLOCK_STATE_CHANGED` and
`ACTION_NOTIFICATION_CHANNEL_BLOCK_STATE_CHANGED` and stop when its
notification is blocked. `AlarmService` at lines 260-374 registers
neither listener; the package's manifest and `AlarmSystemReceiver` omit
both actions too. Blocking just the alarm channel during an active ring
therefore leaves `MediaPlayer` looping until the ten-minute timeout or a
later app status read. `AlarmsModule.status` can clean up on that later
read, but a background service cannot depend on someone reopening the app.

Verified by tracing the complete receiver, restore, delivery, and service
paths. No Android execution is claimed. The existing Android drive tests
notification denial at ordinary delivery, but never combines denial
with a boot or package replacement.

Apply the same notification and channel checks to restored rings before
starting the service, and observe block-state changes while ringing.
Remove denied records and preserve paused countdowns. Validate overdue
scheduled/countdown records and previously
alerting records after reboot with notifications enabled, notifications
disabled, and just the alarm channel blocked. Also cover package
replacement and blocking the channel during a ring; verify that denied
cases start no audio and active rings stop when their Stop control goes.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot. The open old-timeout report concerns ring identity;
the closed snooze-notification report concerns reused notification IDs.
Neither covers permission revalidation during restore.
