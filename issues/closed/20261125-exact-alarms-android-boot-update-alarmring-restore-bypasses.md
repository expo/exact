# @exact/alarms (Android): boot/update AlarmRing.restore() bypasses revoked SCHEDULE_EXACT_ALARM check and starts systemExempted foreground service

**Status:** Closed
**Resolution:** Checked canScheduleExactAlarms() in Android AlarmRing.restore() and due(), dropping held records instead of starting AlarmService when exact-alarm permission is revoked.
**Systems:** packages/alarms (Android AlarmSystemReceiver, AlarmRing, AlarmService)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Back doors)

Verification: verified by reading.

At `9c0431fd5`, `AlarmRing.restore(c)` (`packages/alarms/android/AlarmRing.kt:182-195`), called by `AlarmSystemReceiver.onReceive` (`AlarmRing.kt:252`) on boot and package replacement, does not check `canScheduleExactAlarms()` before calling `ring(c, r)` (`AlarmRing.kt:190, 192, 217-224`) and starting `AlarmService` as a `FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED` foreground service (`AlarmRing.kt:399-402`).

### Reachable trigger
1. With exact-alarm and notification permissions granted, an app schedules an alarm or countdown (`scheduleAlarm`), persisting an `AlarmRecord` in DeviceProtectedStorage (`Alarms.kt:185-198`).
2. The user revokes "Alarms & reminders" (`SCHEDULE_EXACT_ALARM`) in Android Settings without reopening the app. AOSP `AlarmManagerService` cancels future exact alarms and kills the app's UID (`killUid`, which does not place the app in `FLAG_STOPPED`), sending no revocation broadcast and leaving the persisted alarm record on disk.
3. After the alarm's `at` timestamp has passed (or if the record was persisted in state `"alerting"` when the revocation killed the process), the device reboots (`ACTION_LOCKED_BOOT_COMPLETED` / `ACTION_BOOT_COMPLETED`) or the app updates (`ACTION_MY_PACKAGE_REPLACED`), invoking `AlarmSystemReceiver.onReceive` (`packages/alarms/android/exact.json:30-40`, `AlarmRing.kt:252`) -> `AlarmRing.restore(c)`.

### Actual vs. expected result
- **Actual:** `AlarmRing.restore(c)` checks only `if (!stoppable(c))` (notifications enabled and alarm channel unblocked, lines 183-187). For future alarms (`r.at!! > now`), line 190 calls `arm(c, r)`, which checks `if (Build.VERSION.SDK_INT >= 31 && !alarms(c).canScheduleExactAlarms()) return AlarmRecords.remove(c, r.tag)` (line 84). However, for any overdue `"countdown"` / `"scheduled"` record (`r.at!! <= now`, line 190) or any `"alerting"` record (line 192), `restore(c)` bypasses `arm()` and calls `ring(c, r)` directly -> `c.startForegroundService(AlarmService)` (line 222) -> `startForeground(code, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED)` on API 34+ (line 400).
  - On Android 14+ (API 34+), holding `SCHEDULE_EXACT_ALARM` (or `USE_EXACT_ALARM`) is the qualifying prerequisite for `FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED`; starting `AlarmService` from `BOOT_COMPLETED` / `MY_PACKAGE_REPLACED` without that grant fails with `ForegroundServiceStartNotAllowedException` / `SecurityException` (or `ForegroundServiceTypeNotAllowedException`), crashing the app in the background.
  - On Android 12-13 (API 31-33), it starts `AlarmService` and rings an alarm for which the user explicitly revoked exact-alarm permission.
- **Expected:** Per LLP 1133.030 D7 ("Revocation ends alarms ... the package removes the records of every alarm the system held ... A paused countdown, which held no system alarm, stays"), `Alarms.kt:75, 89-91` (`if (!exact || state == "denied") AlarmRing.dropHeld(app)`), and `AlarmRing.kt:83-84, 256-261`, `restore(c)` must verify `Build.VERSION.SDK_INT < 31 || alarms(c).canScheduleExactAlarms()` alongside `stoppable(c)`, dropping all held records (preserving only `"paused"` countdowns) and starting no foreground service when exact-alarm permission is revoked.

### Verification and sibling sweep
- **Verification:** Verified by reading `AlarmRing.kt`, `Alarms.kt`, `packages/alarms/android/exact.json`, `packages/alarms/test/AlarmRingTest.kt`, and `llp/1133.030-alarms.rfc.md` D7/D9, plus confirming AOSP `AlarmManagerService` revocation (`killUid`) and Android 14 `FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED` requirements. No Android device/Robolectric run was executed. Note that closed issues `20261112-review-android-exact-alarm-grant-restores-revoked-alarms.md` (fixed the grant broadcast) and `20261112-review-android-restored-alarms-ignore-notification-denial.md` (added `stoppable(c)` to `restore()`) did not add the exact-alarm check to `restore()`.
- **Siblings checked:**
  - `AlarmRing.kt:137-143` (`snooze`) and `:165-175` (`timeSet`): both delegate to `arm(c, r)`, which checks `canScheduleExactAlarms()` at line 84.
  - `AlarmRing.kt:198-215` (`due`): checks `stoppable(c)` before `ring(c, r)` (adding the same exact-alarm guard in `due` or `ring` also closes any race where `FIRE` is delivered right at revocation).
  - `packages/alarms/apple/Alarms.swift`: AlarmKit owns scheduling/firing with no boot receiver or foreground service.
  - Other Android boot receivers (`packages/notifications/android/Later.kt:53-58, 81-85`, `packages/background-transfer/android/Jobs.kt:341-351`, `packages/geofencing/android/Receivers.kt:49-58`): none require `SCHEDULE_EXACT_ALARM` or start a foreground service.

### Suggested fix and regression test
- Add `fun exact(c: Context): Boolean = Build.VERSION.SDK_INT < 31 || alarms(c).canScheduleExactAlarms()` on `AlarmRing`, and change the guard at `AlarmRing.kt:183` in `restore(c)` from `if (!stoppable(c))` to `if (!stoppable(c) || !exact(c))` (and check `!exact(c)` alongside `!stoppable(c)` in `due()` as defense in depth).
- Add a Robolectric test in `packages/alarms/test/AlarmRingTest.kt` (beside `bootDenied` at lines 168-208) setting `ShadowAlarmManager.setCanScheduleExactAlarms(false)` with future, overdue, alerting, and paused records, dispatching `ACTION_BOOT_COMPLETED` and `ACTION_MY_PACKAGE_REPLACED`, and asserting `shadowOf(app).nextStartedService == null`, `alarms.nextAlarmClock == null`, and only the paused countdown remains in `AlarmRecords.all(app)`.

### Siblings checked

AlarmRing.kt:137-143 (snooze -> arm, checked), AlarmRing.kt:165-175 (timeSet -> arm, checked), AlarmRing.kt:198-215 (due -> checks stoppable, suggest exact() guard as defense in depth); packages/alarms/apple/Alarms.swift (no boot receiver/FGS); packages/notifications/android/Later.kt, packages/background-transfer/android/Jobs.kt, packages/geofencing/android/Receivers.kt (no SCHEDULE_EXACT_ALARM or FGS)
