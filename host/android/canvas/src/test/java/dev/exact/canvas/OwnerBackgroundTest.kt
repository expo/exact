package dev.exact.canvas

import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.SystemClock
import java.nio.ByteBuffer
import java.time.Duration
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.LooperMode
import org.robolectric.util.ReflectionHelpers.ClassParameter
import org.robolectric.util.ReflectionHelpers.callInstanceMethod

internal fun withMainSyncBarrier(work: () -> Unit) {
    val looper = Looper.getMainLooper()
    val barrier = callInstanceMethod<Int>(looper.queue, "postSyncBarrier")
    try { work() } finally {
        callInstanceMethod<Unit>(looper.queue, "removeSyncBarrier", ClassParameter.from(Int::class.javaPrimitiveType, barrier))
    }
    shadowOf(looper).idle()
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [OwnerBackgroundTest.Host::class])
@LooperMode(LooperMode.Mode.PAUSED)
class OwnerBackgroundTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            val visibility = ArrayList<Boolean>()
            @JvmStatic @Implementation fun hidden(hidden: Boolean, eligible: Boolean) { visibility.add(hidden) }
            var due = 0L
            var ticks = 0
            var frames = 0
            var frameDemand = false
            var pending = false
            var delivered = 0
            var applying = false
            var paint: ByteBuffer? = null
            var onFrame: (() -> Unit)? = null
            var chromeStatement: IntArray? = null
            var request: IntArray? = null
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun start(width: Int, height: Int, scale: Float, dir: String, store: String, live: Boolean) = true
            @JvmStatic @Implementation fun fds() = IntArray(0)
            @JvmStatic @Implementation fun background(applying: Boolean): Double {
                Host.applying = applying
                if (SystemClock.uptimeMillis() >= due) { ticks++; due += 30000 }
                if (pending) { pending = false; delivered++ }
                return (due - SystemClock.uptimeMillis()).toDouble()
            }
            @JvmStatic @Implementation fun frame(vsync: Boolean): ByteBuffer? { frames++; onFrame?.invoke(); return paint.also { paint = null } }
            @JvmStatic @Implementation fun pendingPictures() = IntArray(0)
            @JvmStatic @Implementation fun requests(): IntArray? = request.also { request = null }
            @JvmStatic @Implementation fun scrollers(): IntArray? = null
            @JvmStatic @Implementation fun fields(): IntArray? = null
            @JvmStatic @Implementation fun bars(): IntArray? = null
            @JvmStatic @Implementation fun chrome(): IntArray? = chromeStatement.also { chromeStatement = null }
            @JvmStatic @Implementation fun timerHeld() = 0
            @JvmStatic @Implementation fun animating(vsyncOnly: Boolean) = frameDemand
            @JvmStatic @Implementation fun displayFrame() {}
            var owing = false
            @JvmStatic @Implementation fun owed() = owing
            @JvmStatic @Implementation fun nextDue() = (due - SystemClock.uptimeMillis()).toDouble()
            var moves = false
            @JvmStatic @Implementation fun moved() = moves
        }
    }

    @Test fun aStepThatOnlyMovesMakesNoFrameAndStillDoesWhatFollowsOne() {
        Host.due = SystemClock.uptimeMillis() + 30000
        val owner = Owner(false, false, 0) { "0" }
        val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }.get(owner) as HandlerThread
        val handler = Owner::class.java.getDeclaredField("handler").apply { isAccessible = true }.get(owner) as Handler
        val timer = Owner::class.java.getDeclaredField("timer").apply { isAccessible = true }.get(owner) as Runnable
        val turnPosted = Owner::class.java.getDeclaredField("turnPosted").apply { isAccessible = true }
        val looper = shadowOf(thread.looper)
        val chrome = ArrayList<Int>()
        val requests = ArrayList<Int>()
        owner.chrome = { chrome += it[0] }
        owner.asked = { requests += it[0] }
        try {
            val frames = Host.frames
            owner.ask {
                handler.removeCallbacks(timer)
                Host.moves = true
                Host.owing = true
                Host.frameDemand = true
                Host.chromeStatement = intArrayOf(7)
                Host.request = intArrayOf(Requests.ORIENTATION)
            }
            owner.scrolled(1, 0f, 10f, 0, 1)
            owner.ask { }
            assertEquals("no frame", frames, Host.frames)
            assertTrue("a display turn is posted", turnPosted.getBoolean(owner))
            assertTrue("the paint the move owes is timed", handler.hasCallbacks(timer))
            Host.frameDemand = false
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals("the chrome still goes", listOf(7), chrome)
            assertEquals("platform requests still go", listOf(Requests.ORIENTATION), requests)

            val due = SystemClock.uptimeMillis() + 5
            val timerFrames = ArrayList<Long>()
            owner.ask {
                handler.removeCallbacks(timer)
                Host.due = due
                Host.onFrame = {
                    timerFrames += SystemClock.uptimeMillis()
                    Host.due += 30000
                    Host.owing = false
                }
            }
            owner.scrolled(1, 0f, 10f, 0, 2)
            owner.ask { }
            assertTrue(handler.hasCallbacks(timer))
            looper.idleFor(Duration.ofMillis(4))
            assertTrue(timerFrames.isEmpty())
            looper.idleFor(Duration.ofMillis(1))
            assertEquals(listOf(due), timerFrames)

            val afterTimer = owner.ask { Host.moves = false; Host.onFrame = null; Host.frames }
            owner.scrolled(1, 0f, 10f, 0, 3)
            owner.ask { }
            assertTrue("a step that does more makes its frame", Host.frames > afterTimer)
        } finally {
            owner.ask {
                Host.moves = false; Host.owing = false; Host.frameDemand = false
                Host.chromeStatement = null; Host.request = null; Host.onFrame = null
            }
            handler.removeCallbacksAndMessages(null)
            thread.quitSafely(); thread.join(5000)
            assertFalse(thread.isAlive)
        }
    }

    private fun withOwner(work: (Owner) -> Unit) {
        val owner = Owner(false, false, 0) { "0" }
        val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }.get(owner) as HandlerThread
        try { work(owner) } finally {
            owner.ask { Host.paint = null; Host.chromeStatement = null; Host.request = null }
            thread.quitSafely(); thread.join(5000)
            assertFalse(thread.isAlive)
        }
    }

    private fun turn(word: Int) {
        Host.paint = ByteBuffer.allocate(0)
        Host.chromeStatement = intArrayOf(word)
    }

    @Test fun aColdPlatformRequestPrecedesBootCompletionThroughTheBarrier() = withOwner { owner ->
        val main = shadowOf(Looper.getMainLooper())
        val calls = ArrayList<String>()
        val landscape = intArrayOf(Requests.ORIENTATION, 0, 5, 1, 0b1100, 0, 0, 0)
        owner.chrome = { calls += "chrome ${it[0]}" }
        owner.asked = { assertArrayEquals(landscape, it); calls += "orientation" }
        withMainSyncBarrier {
            assertTrue(owner.boot(64, 64, 1f, "", "", false, started = {
                turn(1)
                Host.request = landscape
            }) { calls += "done $it" })
            owner.ask { }
            main.idle()
            assertEquals(listOf("chrome 1", "orientation", "done true"), calls)
        }
    }

    @Test fun queuedColdChromeDoesNotReachTheAdoptingReader() = withOwner { owner ->
        val main = shadowOf(Looper.getMainLooper())
        val calls = ArrayList<String>()
        owner.chrome = { calls += "cold chrome ${it[0]}" }
        withMainSyncBarrier {
            assertTrue(owner.boot(64, 64, 1f, "", "", false, started = { turn(1) }) { calls += "cold done $it" })
            owner.ask { }
            owner.chrome = { calls += "adopted chrome ${it[0]}" }
            assertFalse(owner.boot(80, 64, 1f, "", "", false, started = { turn(2) }) { calls += "adopted done $it" })
            owner.ask { }
            main.idle()
            assertEquals(listOf("cold done true"), calls)
        }
        assertEquals(listOf("cold done true", "adopted chrome 2", "adopted done true"), calls)
    }

    @Test fun adoptionKeepsOlderChromeAndTheRestoredAlertAheadOfCompletion() = withOwner { owner ->
        val main = shadowOf(Looper.getMainLooper())
        val calls = ArrayList<String>()
        val alert = intArrayOf(Requests.SURFACE, 0, 10, 1, Surfaces.ALERT, 0, 0, 0, 64, 64, 0, 0, 0)
        owner.published = { calls += "published" }
        owner.chrome = { calls += "chrome ${it[0]}" }
        owner.asked = { assertArrayEquals(alert, it); calls += "alert" }
        assertTrue(owner.boot(64, 64, 1f, "", "", false) {})
        owner.ask { }
        main.idle()
        owner.endColdDraw()
        withMainSyncBarrier {
            owner.sync { turn(1) }
            val hold = owner.boot(64, 64, 1f, "", "", false, started = {
                turn(2)
                Host.request = alert
            }) { calls += "done $it" }
            owner.ask { }
            main.idle()
            assertEquals("adoption work waits for the traversal", emptyList<String>(), calls)
            assertFalse("adoption leaves drawing to the platform", hold)
        }
        assertEquals(listOf("published", "chrome 1", "chrome 2", "alert", "done true"), calls)
    }

    @Test fun bootWhileStoppedKeepsCompletionAndResumedChromeInQueueOrder() = withOwner { owner ->
        val main = shadowOf(Looper.getMainLooper())
        val calls = ArrayList<String>()
        owner.published = { calls += "published" }
        owner.chrome = { calls += "chrome ${it[0]}" }
        // First a Headless boot, then another boot after the owner has run and stopped.
        repeat(2) {
            calls.clear()
            owner.running(false)
            withMainSyncBarrier {
                val hold = owner.boot(64, 64, 1f, "", "", false, started = { turn(1) }) { calls += "done $it" }
                owner.ask { }
                assertTrue(owner.queue.isEmpty())
                main.idle()
                assertEquals("completion waits for the traversal even without a frame", emptyList<String>(), calls)
                assertFalse("a stopped boot leaves drawing to the platform", hold)
                owner.running(true)
                owner.ask { }
                main.idle()
                assertEquals("resumed chrome waits for the traversal too", emptyList<String>(), calls)
            }
            assertEquals(listOf("done true", "published", "chrome 1"), calls)
        }
    }

    @Test fun anOverrunningTurnStillRequestsVsyncForFrameTasks() {
        val owner = Owner(false, false, 0) { "0" }
        fun field(name: String) = Owner::class.java.getDeclaredField(name).apply { isAccessible = true }
        val thread = field("thread").get(owner) as HandlerThread
        val handler = field("handler").get(owner) as Handler
        val schedule = Owner::class.java.getDeclaredMethod("schedule").apply { isAccessible = true }
        try {
            owner.ask {
                Host.due = SystemClock.uptimeMillis() + 30000
                Host.frameDemand = true
                field("late").setBoolean(owner, true)
                schedule.invoke(owner)
                assertTrue("frame tasks need a display callback even after a late turn", field("turnPosted").getBoolean(owner))
            }
        } finally {
            Host.frameDemand = false
            owner.running(false)
            owner.ask { }
            handler.removeCallbacksAndMessages(null)
            thread.quitSafely(); thread.join(5000)
            assertFalse(thread.isAlive)
        }
    }

    @Test fun mainThreadVisibilityPublishesBeforeTurningTheReader() {
        val previousOwner = Exact.owner
        val previousTurned = Exact.turned
        Exact.owner = null
        Host.visibility.clear()
        var turns = 0
        try {
            Exact.turned = {}
            Exact.hidden(true)
            Exact.turned = {
                assertEquals("the foreground flag precedes the reader turn", listOf(true, false), Host.visibility)
                turns++
            }
            Exact.hidden(false)
            assertEquals("owner=0 produces after returning to the foreground", 1, turns)
        } finally {
            Exact.turned = {}
            Exact.hidden(false)
            Exact.owner = previousOwner
            Exact.turned = previousTurned
        }
    }

    @Test fun stoppedOwnerRunsTimersAndRepliesWithoutPublishingFramesAndResumes() {
        Host.due = SystemClock.uptimeMillis() + 30000
        Host.ticks = 0; Host.frames = 0; Host.delivered = 0; Host.pending = false
        val owner = Owner(false, false, 0) { "0" }
        val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }.get(owner) as HandlerThread
        val handler = Owner::class.java.getDeclaredField("handler").apply { isAccessible = true }.get(owner) as Handler
        val timer = Owner::class.java.getDeclaredField("timer").apply { isAccessible = true }.get(owner) as Runnable
        try {
            owner.running(false)
            owner.ask { }
            assertTrue("the stopped owner keeps its task deadline", handler.hasCallbacks(timer))
            repeat(3) {
                shadowOf(thread.looper).idleFor(Duration.ofSeconds(30))
                owner.ask { }
                assertEquals(it + 1, Host.ticks)
            }
            owner.call { Host.pending = true }
            owner.ask { }
            assertEquals(1, Host.delivered)
            assertEquals(0, Host.frames)
            assertTrue(owner.queue.isEmpty())
            owner.running(true)
            owner.ask { }
            assertEquals(1, Host.frames)
            assertTrue(handler.hasCallbacks(timer))
        } finally {
            handler.removeCallbacksAndMessages(null)
            thread.quitSafely(); thread.join(5000)
            assertFalse(thread.isAlive)
        }
    }
    @Test fun stoppedOwnerDiscardsPaintButWaitsForOutstandingMainThreadWork() {
        Host.due = SystemClock.uptimeMillis() + 30000
        val owner = Owner(false, false, 0) { "0" }
        val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }.get(owner) as HandlerThread
        val handler = Owner::class.java.getDeclaredField("handler").apply { isAccessible = true }.get(owner) as Handler
        val reports = Owner::class.java.getDeclaredField("timerReports").apply { isAccessible = true }.get(owner) as TimerReports
        val reader = Any()
        try {
            owner.ask {
                reports.replace(reader)
                reports.report(reader, false, false, 0)
                val publication = reports.publish()
                owner.queue.add(Owner.Pub(publication, null, false, 0, 0, null, null, null, null))
                assertEquals(0, reports.mainWork)
            }
            owner.running(false)
            owner.ask { }
            assertFalse("the discarded publication no longer blocks idle", Host.applying)
            assertTrue(owner.queue.isEmpty())
            owner.call { reports.mainWork++ }
            owner.ask { }
            assertTrue("outstanding main-thread work blocks idle", Host.applying)
        } finally {
            handler.removeCallbacksAndMessages(null)
            thread.quitSafely(); thread.join(5000)
            assertFalse(thread.isAlive)
        }
    }

}
