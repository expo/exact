package exact.background

import android.content.Context
import android.net.ConnectivityManager
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.work.WorkManager
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.LooperMode
import org.robolectric.util.ReflectionHelpers

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, instrumentedPackages = ["exact.background.BackgroundModule"],
    shadows = [BackgroundTest.Session::class])
@LooperMode(LooperMode.Mode.PAUSED)
class BackgroundTest {
    /** Only the JNI boundary is replaced; ExtensionContext carries wakes and their outcomes. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Session {
        companion object {
            var elapsed = 0.0
            val wakes = ArrayList<JSONObject>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun now() = elapsed
            @JvmStatic @Implementation fun changed(topic: String) {
                if (topic.startsWith("exact-wake:")) wakes += JSONObject(topic.removePrefix("exact-wake:"))
            }
        }
    }

    private val epoch = 1_800_000_000_000L
    private val minute = 60_000L
    private val app: Context get() = RuntimeEnvironment.getApplication()
    private val prefs get() = app.getSharedPreferences("exact.background", Context.MODE_PRIVATE)
    private val main get() = shadowOf(Looper.getMainLooper())
    private val connectivity get() = app.getSystemService(ConnectivityManager::class.java)

    @Before fun clear() {
        prefs.edit().clear().commit()
        androidx.work.testing.WorkManagerTestInitHelper.initializeTestWorkManager(app)
    }

    @After fun closeDatabase() { androidx.work.testing.WorkManagerTestInitHelper.closeWorkDatabase() }

    @After fun stop() {
        BackgroundModule.live?.get()?.let { module ->
            ReflectionHelpers.getField<Handler>(module, "main").removeCallbacksAndMessages(null)
            module.context.ended.clear()
        }
        shadowOf(connectivity).networkCallbacks.toList().forEach { connectivity.unregisterNetworkCallback(it) }
        BackgroundModule.live = null
    }

    private fun scheduledDelay(): Long {
        val work = WorkManager.getInstance(app) as androidx.work.impl.WorkManagerImpl
        val info = work.getWorkInfosForUniqueWork(BackgroundModule.work("sync", "outbox")).get().single { !it.state.isFinished }
        return work.workDatabase.workSpecDao().getWorkSpec(info.id.toString())!!.initialDelay
    }

    private fun launch(elapsed: Long, wall: Long): BackgroundModule {
        SystemClock.setCurrentTimeMillis(wall)
        Session.elapsed = elapsed.toDouble()
        Session.wakes.clear()
        return BackgroundModule(ExtensionContext(app, "background"))
    }

    private fun finish(module: BackgroundModule, outcome: String) {
        val wake = Session.wakes.single()
        assertEquals("background", wake.getString("extension"))
        requireNotNull(module.context.ended.remove(wake.getString("id")))(outcome)
    }

    private fun call(module: BackgroundModule, name: String, tag: String = "outbox"): JSONObject {
        var answer: JSONObject? = null
        module.later(name, JSONObject().put("tag", tag).put("minInterval", 15 * minute)) { answer = it }
        return requireNotNull(answer).also { assertFalse(it.toString(), it.has("error")) }
    }

    private fun online() {
        val network = requireNotNull(connectivity.activeNetwork)
        shadowOf(connectivity).networkCallbacks.forEach { it.onAvailable(network) }
        main.idle()
    }

    private fun failTwoHoursIntoLaunch(): BackgroundModule {
        val module = launch(120 * minute, epoch + 120 * minute)
        online()
        call(module, "registerSync")
        assertEquals(1, Session.wakes.size)
        call(module, "registerSync")
        finish(module, "expired")
        main.idle()
        assertEquals(5 * minute, scheduledDelay())
        return module
    }

    @Test fun anExpiredBackoffFiresAtTheNextOnlineLaunch() {
        failTwoHoursIntoLaunch()
        stop()
        launch(0, epoch + 130 * minute)
        online()
        assertEquals("the five-minute backoff elapsed while the process was gone", 1, Session.wakes.size)
        assertEquals("sync", Session.wakes.single().getString("event"))
        assertEquals("outbox", Session.wakes.single().getJSONArray("args").getString(0))
    }

    @Test fun aWorkerCanDeliverAnExpiredBackoffAfterRelaunch() {
        failTwoHoursIntoLaunch()
        stop()
        val module = launch(0, epoch + 130 * minute)
        var completed: Boolean? = null
        module.ran("sync", "outbox") { completed = it }
        main.idle()
        assertEquals("the worker must raise the tag before acknowledging it", 1, Session.wakes.size)
        assertEquals("sync", Session.wakes.single().getString("event"))
        assertEquals("outbox", Session.wakes.single().getJSONArray("args").getString(0))
        assertNull(completed)
        finish(module, "completed")
        main.idle()
        assertEquals(true, completed)
        assertEquals(0, call(module, "syncTags").getJSONArray("exact:answer").length())
    }

    @Test fun anUnexpiredBackoffWaitsOnlyItsRemainingTimeAfterRelaunch() {
        failTwoHoursIntoLaunch()
        stop()
        val module = launch(0, epoch + 122 * minute)
        online()
        assertTrue(Session.wakes.isEmpty())
        var completed: Boolean? = null
        module.ran("sync", "outbox") { completed = it }
        main.idle()
        assertEquals(true, completed)
        assertTrue(Session.wakes.isEmpty())
        call(module, "registerSync")
        assertEquals(3 * minute, scheduledDelay())
        Session.elapsed = (3 * minute - 1).toDouble()
        main.idleFor(java.time.Duration.ofMillis(3 * minute - 1))
        assertTrue(Session.wakes.isEmpty())
        Session.elapsed += 1
        main.idleFor(java.time.Duration.ofMillis(1))
        assertEquals(1, Session.wakes.size)
    }

    @Test fun registrationAndLastRunRemainEpochDatesAcrossLaunches() {
        val module = launch(120 * minute, epoch + 120 * minute)
        call(module, "registerPeriodicSync", "refresh")
        fun stored() = JSONObject(prefs.getString("tags", "{}")!!).getJSONObject("periodicsync").getJSONObject("refresh")
        assertEquals((epoch + 120 * minute).toDouble(), stored().getDouble("registered"), 0.0)
        module.ran("periodicsync", "refresh") {}
        main.idle()
        assertEquals("periodicsync", Session.wakes.single().getString("event"))
        assertEquals("refresh", Session.wakes.single().getJSONArray("args").getString(0))
        Session.elapsed += minute
        SystemClock.setCurrentTimeMillis(epoch + 121 * minute)
        finish(module, "completed")
        main.idle()
        assertEquals((epoch + 121 * minute).toDouble(), stored().getDouble("lastRun"), 0.0)
        stop()
        val reopened = launch(0, epoch + 130 * minute)
        val row = call(reopened, "periodicSyncTags").getJSONArray("exact:answer").getJSONObject(0)
        assertEquals((epoch + 121 * minute).toDouble(), row.getDouble("lastRun"), 0.0)
    }

    @Test fun aSyncRetryDueAfterDeviceSleepRunsInsteadOfBeingDropped() {
        val module = failTwoHoursIntoLaunch()
        Session.wakes.clear()
        // During real device sleep, uptimeMillis() pauses (so Handler.postDelayed does not fire) and CLOCK_MONOTONIC
        // pauses (Session.elapsed stays frozen), while live wall time advances past the 5m backoff before WorkManager wakes.
        ReflectionHelpers.getField<Handler>(module, "main").removeCallbacksAndMessages(null)
        SystemClock.setCurrentTimeMillis(epoch + 126 * minute)
        var completed: Boolean? = null
        module.ran("sync", "outbox") { completed = it }
        main.idle()
        assertEquals("a sync retry due after device sleep must run", 1, Session.wakes.size)
        assertEquals("sync", Session.wakes.single().getString("event"))
        assertEquals("outbox", Session.wakes.single().getJSONArray("args").getString(0))
        assertNull(completed)
        finish(module, "completed")
        main.idle()
        assertEquals(true, completed)
        assertEquals(0, call(module, "syncTags").getJSONArray("exact:answer").length())
    }

    @Test fun anEarlyWorkManagerWakeReEnqueuesRemainingDelay() {
        val module = failTwoHoursIntoLaunch()
        Session.wakes.clear()
        // Wall time advanced 2 minutes; 3 minutes remain before notBefore.
        SystemClock.setCurrentTimeMillis(epoch + 122 * minute)
        var completed: Boolean? = null
        module.ran("sync", "outbox") { completed = it }
        main.idle()
        assertEquals(true, completed)
        assertTrue(Session.wakes.isEmpty())
        assertEquals("early wake must re-enqueue the remaining backoff", 3 * minute, scheduledDelay())
    }
}
