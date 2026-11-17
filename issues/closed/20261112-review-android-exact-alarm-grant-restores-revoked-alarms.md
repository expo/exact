# Granting exact-alarm permission restores previously revoked alarms

**Status:** Closed
**Resolution:** The grant broadcast now removes every record but a paused countdown's, as a status read during the revocation would have, and restores nothing; boot restoration is unchanged.
**Systems:** Alarms package, Android permission lifecycle
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, revoking and then restoring exact-alarm permission without
reopening the app between those actions brings back its old alarms.
LLP 1133.030 D7 specifies that revocation ends those alarms and that
granting permission again does not restore them.

`packages/alarms/android/AlarmRing.kt:246-248` handles
`ACTION_SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED` by calling
`restore` when permission is granted. `restore` at lines 177-185 rearms
every persisted scheduled/countdown record and immediately rings overdue
ones. Android clears the platform alarms and stops the app when exact
alarm permission is revoked, but the package's durable records remain.

`packages/alarms/android/Alarms.kt:82-88` removes those records if an
app status read observes the denied interval. That read is absent when
the person switches permission off and back on entirely in Settings.
The grant broadcast then sees permission already granted and restores
the records that revocation was meant to retire.

Concrete sequence: schedule an alarm, close the app, revoke its exact-alarm
permission, then grant it again without launching the app between the
two changes. The receiver rearms the old alarm, or rings it immediately
if it fell due meanwhile. Expected: the old alarm remains gone; a new
schedule requires an app action. Paused countdowns should remain paused.

Verified by tracing the full grant receiver, durable record, restore,
and status-read paths. No Android execution is claimed. The existing
Android drive relaunches the app after revocation before granting again,
so its status read clears the records and misses this sequence.

Retire previously held records on the grant broadcast as D7 specifies,
while retaining paused countdowns. Keep boot restoration as a separate
transition. Validate revoke/grant entirely in Settings, with one future
alarm, one overdue alarm, and a paused countdown; confirm that neither
revoked alarm is scheduled or sounded after the grant.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot. The related notification-blocking review report concerns
whether sound has a usable Stop control. This report concerns restoration
of alarms the person already revoked, even with notifications enabled.
