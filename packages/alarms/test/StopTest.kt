package exact.alarms

import android.app.AlarmManager
import android.content.Context
import android.content.Intent
import android.provider.Settings
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

/** The service stops with the last start it was given, which the system refuses while a newer start is on its way,
 * so a ring started as the last one stops is not taken down with it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@LooperMode(LooperMode.Mode.PAUSED)
class StopTest {
    @Test fun theLastRingStopsWithTheLatestStart() {
        val app = RuntimeEnvironment.getApplication()
        val alarms = app.getSystemService(AlarmManager::class.java)
        ShadowAlarmManager.setCanScheduleExactAlarms(true)
        Settings.Global.putInt(app.contentResolver, Settings.Global.BOOT_COUNT, 1)
        val now = System.currentTimeMillis().toDouble()
        val a = AlarmRecord("a", "Tea", now, null, null, null, "scheduled")
        val b = AlarmRecord("b", "Oven", now, null, null, null, "scheduled")
        val service = Robolectric.buildService(AlarmService::class.java).create()
        try {
            for ((r, start) in listOf(a to 1, b to 2)) {
                AlarmRing.set(app, r)
                alarms.cancel(AlarmRing.action(app, r, AlarmRing.FIRE))
                AlarmRing.due(app, r.tag)
                service.get().onStartCommand(Intent(app, AlarmService::class.java).putExtra("tag", r.tag), 0, start)
            }
            service.get().end(b.tag)
            assertFalse(shadowOf(service.get()).isStoppedBySelf)
            service.get().end(a.tag)
            assertTrue(shadowOf(service.get()).isStoppedBySelf)
            assertEquals("stopped with the latest start, not unconditionally", 2, shadowOf(service.get()).stopSelfId)

            // A start that finds nothing to ring stops with its own.
            val idle = Robolectric.buildService(AlarmService::class.java).create()
            idle.get().onStartCommand(Intent(app, AlarmService::class.java).putExtra("tag", "gone"), 0, 3)
            assertEquals(3, shadowOf(idle.get()).stopSelfId)
            idle.destroy()
        } finally {
            service.destroy()
            listOf(a, b).forEach { AlarmRing.cancel(app, it.tag) }
            app.createDeviceProtectedStorageContext().getSharedPreferences("exact.alarms", Context.MODE_PRIVATE)
                .edit().clear().commit()
        }
    }
}
