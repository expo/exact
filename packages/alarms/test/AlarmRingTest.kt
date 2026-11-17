package exact.alarms

import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.os.Looper
import android.provider.Settings
import java.time.Duration
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import org.robolectric.shadows.ShadowAlarmManager
import org.robolectric.shadows.ShadowSystemClock

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@LooperMode(LooperMode.Mode.PAUSED)
class AlarmRingTest {
    @Test fun repeatKeepsTheCountdownAfterTheLastRingStops() = snooze(duration = 60_000.0)

    @Test fun snoozeKeepsTheCountdownAfterTheLastRingStops() = snooze(duration = null)

    @Test fun repeatKeepsTheCountdownWhenAnotherRingTakesTheForeground() =
        snooze(duration = 60_000.0, otherFirst = false)

    @Test fun repeatKeepsTheCountdownWhileAnotherRingStaysInTheForeground() =
        snooze(duration = 60_000.0, otherFirst = true)

    private fun snooze(duration: Double?, otherFirst: Boolean? = null) {
        val app = RuntimeEnvironment.getApplication()
        val alarms = app.getSystemService(AlarmManager::class.java)
        val notices = app.getSystemService(NotificationManager::class.java)
        ShadowAlarmManager.setCanScheduleExactAlarms(true)
        Settings.Global.putInt(app.contentResolver, Settings.Global.BOOT_COUNT, 1)
        val timer = AlarmRecord("timer", "Tea", System.currentTimeMillis() + 60_000.0,
            duration, 60_000.0, null, if (duration == null) "scheduled" else "countdown")
        val other = AlarmRecord("other", "Oven", timer.at, 60_000.0, null, null, "countdown")
        val records = when (otherFirst) {
            true -> listOf(other, timer)
            false -> listOf(timer, other)
            null -> listOf(timer)
        }
        records.forEach { AlarmRing.set(app, it) }
        ShadowSystemClock.advanceBy(Duration.ofMinutes(1))
        val service = Robolectric.buildService(AlarmService::class.java).create()
        try {
            for (r in records) {
                alarms.cancel(AlarmRing.action(app, r, AlarmRing.FIRE))
                AlarmRing.due(app, r.tag)
                service.get().onStartCommand(Intent(app, AlarmService::class.java).putExtra("tag", r.tag), 0, 1)
            }
            val ring = notices.activeNotifications.single { it.id == timer.code }.notification
            assertEquals(AlarmRing.RING, ring.channelId)
            val action = ring.actions.single { it.title == if (duration == null) "Snooze" else "Repeat" }
            AlarmReceiver().onReceive(app, shadowOf(action.actionIntent).savedIntent)
            shadowOf(Looper.getMainLooper()).idle()

            val next = AlarmRecords.get(app, timer.tag)!!
            assertEquals("countdown", next.state)
            val countdowns = notices.activeNotifications.filter { it.notification.channelId == AlarmRing.COUNTDOWN }
            assertEquals("ring teardown must leave the snoozed countdown in the shade", 1, countdowns.size)
            val countdown = countdowns.single()
            assertEquals(next.code, countdown.id)
            assertEquals("Tea", countdown.notification.extras.getString(Notification.EXTRA_TITLE))
            assertTrue(countdown.notification.flags and Notification.FLAG_ONGOING_EVENT != 0)
            assertTrue(countdown.notification.extras.getBoolean(Notification.EXTRA_SHOW_CHRONOMETER))
            assertTrue(countdown.notification.extras.getBoolean(Notification.EXTRA_CHRONOMETER_COUNT_DOWN))
            assertEquals(next.at!!.toLong(), countdown.notification.`when`)
            assertEquals(next.at!!.toLong(), alarms.nextAlarmClock.triggerTime)
            val rings = notices.activeNotifications.filter { it.notification.channelId == AlarmRing.RING }
            assertEquals(if (otherFirst == null) emptyList<Int>() else listOf(other.code), rings.map { it.id })
            assertEquals(otherFirst == null, shadowOf(service.get()).isStoppedBySelf)
        } finally {
            service.destroy()
            records.forEach { AlarmRing.cancel(app, it.tag) }
            app.createDeviceProtectedStorageContext().getSharedPreferences("exact.alarms", Context.MODE_PRIVATE)
                .edit().clear().commit()
        }
    }

    /** B rings at 0 s and A at 1 s; A is repeated at 2 s and rings again at 62 s. B keeps the service alive through the
     * repeat, so A's first ring's limit, due at 601 s, must not end its second, due at 662 s. */
    @Test fun aRepeatedRingGetsItsOwnTimeLimit() {
        val app = RuntimeEnvironment.getApplication()
        val alarms = app.getSystemService(AlarmManager::class.java)
        val looper = shadowOf(Looper.getMainLooper())
        ShadowAlarmManager.setCanScheduleExactAlarms(true)
        Settings.Global.putInt(app.contentResolver, Settings.Global.BOOT_COUNT, 1)
        val now = System.currentTimeMillis().toDouble()
        val b = AlarmRecord("b", "Oven", now, null, null, null, "scheduled")
        val a = AlarmRecord("a", "Tea", now, 60_000.0, 60_000.0, null, "countdown")
        val service = Robolectric.buildService(AlarmService::class.java).create()
        fun ring(r: AlarmRecord) {
            alarms.cancel(AlarmRing.action(app, r, AlarmRing.FIRE))
            AlarmRing.due(app, r.tag)
            service.get().onStartCommand(Intent(app, AlarmService::class.java).putExtra("tag", r.tag), 0, 1)
        }
        try {
            AlarmRing.set(app, b)
            ring(b)
            looper.idleFor(Duration.ofSeconds(1))
            a.at = System.currentTimeMillis().toDouble()
            AlarmRing.set(app, a)
            ring(a)
            looper.idleFor(Duration.ofSeconds(1))
            AlarmRing.snooze(app, a.tag)
            looper.idle()
            looper.idleFor(Duration.ofSeconds(60))
            ring(AlarmRecords.get(app, a.tag)!!)

            looper.idleFor(Duration.ofSeconds(538))
            assertNull("B's ring ends at its limit", AlarmRecords.get(app, b.tag))
            looper.idleFor(Duration.ofSeconds(1))
            assertEquals("A's second ring outlives its first ring's limit", "alerting", AlarmRecords.get(app, a.tag)?.state)
            looper.idleFor(Duration.ofSeconds(61))
            assertNull("A's second ring ends at its own limit", AlarmRecords.get(app, a.tag))
        } finally {
            service.destroy()
            listOf(a, b).forEach { AlarmRing.cancel(app, it.tag) }
            app.createDeviceProtectedStorageContext().getSharedPreferences("exact.alarms", Context.MODE_PRIVATE)
                .edit().clear().commit()
        }
    }

    /** Exact alarms revoked and granted again with the app never opened between: the grant broadcast is the package's
     * first word of it, and the alarms the revocation ended stay ended; a paused countdown, which held none, stays (D7). */
    @Test fun aGrantAfterARevocationRestoresNoAlarm() {
        val app = RuntimeEnvironment.getApplication()
        val alarms = app.getSystemService(AlarmManager::class.java)
        ShadowAlarmManager.setCanScheduleExactAlarms(true)
        Settings.Global.putInt(app.contentResolver, Settings.Global.BOOT_COUNT, 1)
        val now = System.currentTimeMillis().toDouble()
        val future = AlarmRecord("future", "Wake", now + 3_600_000.0, null, null, null, "scheduled")
        val overdue = AlarmRecord("overdue", "Tea", now + 60_000.0, 60_000.0, null, null, "countdown")
        val paused = AlarmRecord("paused", "Oven", now + 60_000.0, 60_000.0, null, null, "countdown")
        val records = listOf(future, overdue, paused)
        try {
            records.forEach { AlarmRing.set(app, it) }
            AlarmRing.pause(app, AlarmRecords.get(app, paused.tag)!!)
            // The revocation: Android stops the app and drops its alarms, and no session reads the status.
            ShadowAlarmManager.setCanScheduleExactAlarms(false)
            listOf(future, overdue).forEach { alarms.cancel(AlarmRing.action(app, it, AlarmRing.FIRE)) }
            ShadowSystemClock.advanceBy(Duration.ofMinutes(2))
            ShadowAlarmManager.setCanScheduleExactAlarms(true)
            AlarmSystemReceiver().onReceive(app, Intent(AlarmManager.ACTION_SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED))
            shadowOf(Looper.getMainLooper()).idle()

            assertEquals(listOf(paused.tag), AlarmRecords.all(app).map { it.tag })
            assertEquals("paused", AlarmRecords.get(app, paused.tag)!!.state)
            assertNull("no revoked alarm is set again", alarms.nextAlarmClock)
            assertNull("no overdue alarm rings", shadowOf(app).nextStartedService)
        } finally {
            records.forEach { AlarmRing.cancel(app, it.tag) }
            app.createDeviceProtectedStorageContext().getSharedPreferences("exact.alarms", Context.MODE_PRIVATE)
                .edit().clear().commit()
        }
    }

    @Test fun aBootWithNotificationsOffRingsNothing() = bootDenied { notices -> shadowOf(notices).setNotificationsEnabled(false) }

    @Test fun aBootWithTheAlarmChannelBlockedRingsNothing() = bootDenied { notices ->
        notices.createNotificationChannel(NotificationChannel(AlarmRing.RING, "Alarms", NotificationManager.IMPORTANCE_NONE))
    }

    /** A boot with no Stop to show: neither an overdue alarm nor one that was ringing starts its sound, every held alarm
     * goes, and a paused countdown stays (D7). */
    private fun bootDenied(deny: (NotificationManager) -> Unit) {
        val app = RuntimeEnvironment.getApplication()
        val alarms = app.getSystemService(AlarmManager::class.java)
        val notices = app.getSystemService(NotificationManager::class.java)
        ShadowAlarmManager.setCanScheduleExactAlarms(true)
        Settings.Global.putInt(app.contentResolver, Settings.Global.BOOT_COUNT, 1)
        val now = System.currentTimeMillis().toDouble()
        val future = AlarmRecord("future", "Wake", now + 3_600_000.0, null, null, null, "scheduled")
        val overdue = AlarmRecord("overdue", "Tea", now + 60_000.0, null, null, null, "scheduled")
        val paused = AlarmRecord("paused", "Oven", now + 60_000.0, 60_000.0, null, null, "countdown")
        val ringing = AlarmRecord("ringing", "Bread", now, null, null, null, "alerting")
        val records = listOf(future, overdue, paused, ringing)
        try {
            listOf(future, overdue, paused).forEach { AlarmRing.set(app, it) }
            AlarmRing.pause(app, AlarmRecords.get(app, paused.tag)!!)
            ringing.code = AlarmRecords.nextCode(app)
            AlarmRecords.put(app, ringing)
            deny(notices)
            listOf(future, overdue).forEach { alarms.cancel(AlarmRing.action(app, it, AlarmRing.FIRE)) }
            ShadowSystemClock.advanceBy(Duration.ofMinutes(2))
            Settings.Global.putInt(app.contentResolver, Settings.Global.BOOT_COUNT, 2)
            AlarmSystemReceiver().onReceive(app, Intent(Intent.ACTION_BOOT_COMPLETED))
            shadowOf(Looper.getMainLooper()).idle()

            assertNull("no sound starts", shadowOf(app).nextStartedService)
            assertEquals(listOf(paused.tag), AlarmRecords.all(app).map { it.tag })
            assertNull("no alarm is set again", alarms.nextAlarmClock)
        } finally {
            records.forEach { AlarmRing.cancel(app, it.tag) }
            app.createDeviceProtectedStorageContext().getSharedPreferences("exact.alarms", Context.MODE_PRIVATE)
                .edit().clear().commit()
        }
    }

    /** The alarm channel blocked while a ring sounds takes its Stop away: the ring ends and the service with it (D7). */
    @Test fun blockingTheChannelWhileRingingEndsTheRing() {
        val app = RuntimeEnvironment.getApplication()
        val alarms = app.getSystemService(AlarmManager::class.java)
        val notices = app.getSystemService(NotificationManager::class.java)
        ShadowAlarmManager.setCanScheduleExactAlarms(true)
        Settings.Global.putInt(app.contentResolver, Settings.Global.BOOT_COUNT, 1)
        val r = AlarmRecord("tea", "Tea", System.currentTimeMillis().toDouble(), null, null, null, "scheduled")
        val service = Robolectric.buildService(AlarmService::class.java).create()
        try {
            AlarmRing.set(app, r)
            alarms.cancel(AlarmRing.action(app, r, AlarmRing.FIRE))
            AlarmRing.due(app, r.tag)
            service.get().onStartCommand(Intent(app, AlarmService::class.java).putExtra("tag", r.tag), 0, 1)
            assertEquals("alerting", AlarmRecords.get(app, r.tag)?.state)

            notices.deleteNotificationChannel(AlarmRing.RING)
            notices.createNotificationChannel(NotificationChannel(AlarmRing.RING, "Alarms", NotificationManager.IMPORTANCE_NONE))
            app.sendBroadcast(Intent(NotificationManager.ACTION_NOTIFICATION_CHANNEL_BLOCK_STATE_CHANGED))
            shadowOf(Looper.getMainLooper()).idle()

            assertNull("the ring's record goes", AlarmRecords.get(app, r.tag))
            assertTrue("the service ends", shadowOf(service.get()).isStoppedBySelf)
        } finally {
            service.destroy()
            AlarmRing.cancel(app, r.tag)
            notices.deleteNotificationChannel(AlarmRing.RING)
            app.createDeviceProtectedStorageContext().getSharedPreferences("exact.alarms", Context.MODE_PRIVATE)
                .edit().clear().commit()
        }
    }
}
