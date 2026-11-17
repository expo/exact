package exact.push

import android.content.Context
import android.os.HandlerThread
import android.os.Looper
import com.google.firebase.messaging.RemoteMessage
import dev.exact.canvas.Exact
import dev.exact.canvas.Extensions
import dev.exact.canvas.Native
import dev.exact.canvas.Owner
import exact.app.ExactExtensions
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], manifest = Config.NONE, shadows = [PushServiceTest.Session::class, PushServiceTest.Environment::class])
class PushServiceTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Session {
        companion object {
            var boots = 0
            var active = 1
            val wakes = CopyOnWriteArrayList<JSONObject>()
            val expired = CopyOnWriteArrayList<String>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun start(width: Int, height: Int, scale: Float, dir: String, store: String, live: Boolean): Boolean { boots++; return true }
            @JvmStatic @Implementation fun fds() = IntArray(0)
            @JvmStatic @Implementation fun background(applying: Boolean) = -1.0
            @JvmStatic @Implementation fun dataActive(unshown: Boolean) = active
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun storeDrain(ms: Int) = true
            @JvmStatic @Implementation fun changed(topic: String) {
                if (topic.startsWith("exact-wake:")) wakes += JSONObject(topic.removePrefix("exact-wake:"))
                if (topic.startsWith("exact-wake-expired:")) expired += JSONObject(topic.removePrefix("exact-wake-expired:")).getString("id")
            }
        }
    }

    @Implements(android.system.Os::class)
    class Environment {
        companion object { @JvmStatic @Implementation fun environ(): Array<String> = emptyArray() }
    }

    private val app: Context get() = RuntimeEnvironment.getApplication()
    private val main get() = shadowOf(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor()

    @Before fun setup() {
        Session.boots = 0; Session.active = 1; Session.wakes.clear(); Session.expired.clear()
        Exact.owner = null; Exact.live = false; Exact.unshown = false
        ExactExtensions.process(mapOf("push" to ::PushModule), atLoad = listOf("push"))
    }

    @After fun cleanup() {
        worker.shutdownNow()
        worker.awaitTermination(5, TimeUnit.SECONDS)
        Exact.owner?.let { owner ->
            val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }.get(owner) as HandlerThread
            thread.quitSafely(); thread.join(5000)
        }
        Exact.owner = null; Exact.live = false; Exact.unshown = false
        Extensions.stopped(false)
        ExactExtensions.process()
    }

    private fun settle() = repeat(3) { main.idle(); Exact.owner?.ask {}; main.idle() }

    private fun completed(id: String) {
        Extensions.call(1, JSONObject().put("extension", "push").put("woke", id).put("outcome", "completed").toString())
        settle()
    }

    @Test fun aDataMessageBootsAHeadlessSessionAndRaisesPush() {
        val service = Robolectric.buildService(PushService::class.java).get()
        val message = RemoteMessage.Builder("sender").setMessageId("panic-42")
            .addData("title", "Panic").addData("body", "{\"navigate\":\"/urgencies\"}").build()
        val delivery = worker.submit { service.onMessageReceived(message) }
        val limit = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (Session.wakes.isEmpty() && !delivery.isDone && System.nanoTime() < limit) {
            settle()
            Thread.sleep(50)
        }
        assertEquals("a data-only message must reach the app", 1, Session.wakes.size)
        val wake = Session.wakes.single()
        assertEquals(1, Session.boots)
        assertEquals("push", wake.getString("extension"))
        assertEquals("push", wake.getString("event"))
        val data = JSONObject(wake.getJSONArray("args").getJSONObject(0).getString("data"))
        assertEquals("Panic", data.getString("title"))
        assertFalse("FCM keeps custody until the handler ends", delivery.isDone)
        completed(wake.getString("id"))
        delivery.get(5, TimeUnit.SECONDS)
        main.idleFor(Duration.ofSeconds(10))
        settle()
        assertTrue("a completed message is not later expired", Session.expired.isEmpty())
    }

    @Test fun aRunningSessionIsReusedAndItsUnfinishedWakeExpires() {
        var ready = false
        dev.exact.canvas.ExtensionContext.session(app) { ready = it }
        settle()
        assertTrue(ready)
        Extensions.started()
        var endings = 0
        PushMessage(app, emptyMap()) { endings++ }.start()
        settle()
        assertEquals(1, Session.boots)
        assertEquals(1, Session.wakes.size)
        main.idleFor(Duration.ofMillis(9999))
        assertEquals(0, endings)
        main.idleFor(Duration.ofMillis(1))
        settle()
        assertEquals(1, endings)
        assertEquals(listOf(Session.wakes.single().getString("id")), Session.expired)
        completed(Session.wakes.single().getString("id"))
        assertEquals("late completion is ignored", 1, endings)
    }

    @Test fun expiryDuringBootDoesNotDeliverAfterTheDataModuleBecomesReady() {
        Session.active = 0
        var endings = 0
        PushMessage(app, mapOf("panic" to "42")) { endings++ }.start()
        settle()
        assertTrue(Session.wakes.isEmpty())
        main.idleFor(Duration.ofSeconds(10))
        assertEquals(1, endings)
        Session.active = 1
        main.idleFor(Duration.ofMillis(100))
        settle()
        assertTrue(Session.wakes.isEmpty())
    }

    @Test fun aMessageThatExpiredBeforeTheMainThreadRunsDoesNotBoot() {
        var endings = 0
        PushMessage(app, emptyMap()) { endings++ }.start()
        org.robolectric.shadows.ShadowSystemClock.advanceBy(Duration.ofSeconds(10))
        settle()
        assertEquals(1, endings)
        assertEquals(0, Session.boots)
        assertTrue(Session.wakes.isEmpty())
    }
}
