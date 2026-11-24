package dev.exact.canvas

import android.app.KeyguardManager
import java.time.Duration
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE)
class ControlPressTest {
    @Test fun aQueuedClickDoesNotWaitThroughAnotherScreenLock() {
        val app = RuntimeEnvironment.getApplication()
        for (unlocked in listOf(false, true)) {
            lateinit var current: () -> Boolean
            lateinit var ready: (Boolean) -> Unit
            Controls.press(app, "tool", "digest", android.os.SystemClock.elapsedRealtime(), unlocked = unlocked) { _, live, done ->
                current = live
                ready = done
            }
            assertTrue(current())
            app.sendBroadcast(android.content.Intent(android.content.Intent.ACTION_SCREEN_OFF))
            shadowOf(android.os.Looper.getMainLooper()).idle()
            assertFalse(current())
            ready(true)
        }
    }

    @Test fun supersedingARequestedDismissalFinishesItsActivity() {
        val app = RuntimeEnvironment.getApplication()
        val lock = shadowOf(app.getSystemService(KeyguardManager::class.java))
        lock.setKeyguardLocked(true)
        lock.setIsDeviceLocked(true)
        val intent = ControlPress.intent(app, "open", "digest", url = "/detail")
        val activity = Robolectric.buildActivity(ControlPress::class.java, intent).create()
        shadowOf(android.os.Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
        assertFalse(activity.get().isFinishing)
        Controls.superseded()
        assertTrue(activity.get().isFinishing)
        lock.setIsDeviceLocked(false)
        lock.setKeyguardLocked(false)
        assertNull(shadowOf(app).nextStartedActivity)
        activity.destroy()
    }

    @Test fun aPlainClickChecksTheScreenAtSessionReadiness() {
        val app = RuntimeEnvironment.getApplication()
        val power = shadowOf(app.getSystemService(android.os.PowerManager::class.java))
        Controls.press(app, "tool", "digest", android.os.SystemClock.elapsedRealtime(), unlocked = false) { _, live, done ->
            assertTrue(live())
            power.setIsInteractive(false)
            assertFalse(live())
            done(true)
        }
    }

    @Test fun screenOffDuringKeyguardDismissalDropsTheUrl() {
        val app = RuntimeEnvironment.getApplication()
        val lock = shadowOf(app.getSystemService(KeyguardManager::class.java))
        lock.setKeyguardLocked(true)
        lock.setIsDeviceLocked(true)
        val intent = ControlPress.intent(app, "open", "digest", url = "/detail")
        val controller = Robolectric.buildActivity(ControlPress::class.java, intent).create()
        shadowOf(android.os.Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
        app.sendBroadcast(android.content.Intent(android.content.Intent.ACTION_SCREEN_OFF))
        shadowOf(android.os.Looper.getMainLooper()).idle()
        lock.setIsDeviceLocked(false)
        lock.setKeyguardLocked(false)
        assertTrue(controller.get().isFinishing)
        assertNull(shadowOf(app).nextStartedActivity)
        controller.destroy()
    }

    @Test fun viewReportsKeepOnlyTheActionBehindAPress() {
        val call = JSONObject().put("origin", "press").put("press", 7).put("send", "getUserMedia")
        assertEquals(7L, Cause.of(call).press)
        val shown = Cause.of(call.put("view", "wave").put("displayed", true))
        val bound = Cause.of(call.put("press", 8).put("bound", "track"))
        assertEquals("action", shown.lower(bound).origin)
        assertNull(shown.lower(bound).press)
    }

    @Test fun aUrlWaitsForTheDismissedKeyguardToLeaveTheScreen() {
        val app = RuntimeEnvironment.getApplication()
        val lock = shadowOf(app.getSystemService(KeyguardManager::class.java))
        lock.setKeyguardLocked(true)
        lock.setIsDeviceLocked(false)
        val intent = ControlPress.intent(app, "open", "digest", url = "/detail")
        val activity = Robolectric.buildActivity(ControlPress::class.java, intent).create()
        assertNull(shadowOf(app).nextStartedActivity)
        lock.setKeyguardLocked(false)
        shadowOf(android.os.Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
        assertEquals("/detail", shadowOf(app).nextStartedActivity?.dataString)
        activity.destroy()
    }

    @Test fun dismissalOpensTheUrlOnlyIfTheDeviceIsStillUnlocked() {
        val app = RuntimeEnvironment.getApplication()
        val lock = shadowOf(app.getSystemService(KeyguardManager::class.java))
        for (unlocked in listOf(false, true)) {
            lock.setKeyguardLocked(true)
            lock.setIsDeviceLocked(true)
            val intent = ControlPress.intent(app, "open", "digest", url = "/detail")
            val activity = Robolectric.buildActivity(ControlPress::class.java, intent).create()
            shadowOf(android.os.Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
            assertFalse(activity.get().isFinishing)
            lock.setKeyguardLocked(false)
            assertFalse(activity.get().isFinishing)
            lock.setIsDeviceLocked(!unlocked)
            shadowOf(android.os.Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
            assertTrue(activity.get().isFinishing)
            assertEquals(if (unlocked) "/detail" else null, shadowOf(app).nextStartedActivity?.dataString)
            activity.destroy()
        }
    }

    @Test fun onlyTheActivityThatTookTheLockedUrlCanOpenItAfterUnlock() {
        val app = RuntimeEnvironment.getApplication()
        val lock = shadowOf(app.getSystemService(KeyguardManager::class.java))
        fun intent() = ControlPress.intent(app, "open", "digest", url = "/detail")
        val launches = shadowOf(app)
        lock.setIsDeviceLocked(true)
        val first = intent()
        Robolectric.buildActivity(ControlPress::class.java, first).create().destroy()
        lock.setIsDeviceLocked(false)
        val replay = Robolectric.buildActivity(ControlPress::class.java, first).create()
        assertTrue(replay.get().isFinishing)
        assertNull(launches.nextStartedActivity)
        replay.destroy()
        lock.setIsDeviceLocked(true)
        val second = Robolectric.buildActivity(ControlPress::class.java, intent()).create()
        Controls.superseded()
        shadowOf(android.os.Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
        assertTrue(second.get().isFinishing)
        val last = Robolectric.buildActivity(ControlPress::class.java, intent()).create()
        assertNull(launches.nextStartedActivity)
        lock.setIsDeviceLocked(false)
        shadowOf(android.os.Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
        assertEquals("/detail", launches.nextStartedActivity?.dataString)
        assertNull(launches.nextStartedActivity)
        second.destroy()
        last.destroy()
    }

    @Test fun recreatingTheWaitingActivityDropsTheClick() {
        val app = RuntimeEnvironment.getApplication()
        val lock = shadowOf(app.getSystemService(KeyguardManager::class.java))
        lock.setIsDeviceLocked(true)
        val intent = ControlPress.intent(app, "open", "digest", url = "/detail")
        val controller = Robolectric.buildActivity(ControlPress::class.java, intent).setup()
        controller.recreate()
        lock.setIsDeviceLocked(false)
        shadowOf(android.os.Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
        assertTrue(controller.get().isFinishing)
        assertNull(shadowOf(app).nextStartedActivity)
        controller.pause().stop().destroy()
    }
}
