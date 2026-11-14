package dev.exact.canvas

import android.os.Handler
import android.os.HandlerThread
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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [OwnerBackgroundTest.Host::class])
@LooperMode(LooperMode.Mode.PAUSED)
class OwnerBackgroundTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            var due = 0L
            var ticks = 0
            var frames = 0
            var pending = false
            var delivered = 0
            var applying = false
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun background(applying: Boolean): Double {
                Host.applying = applying
                if (SystemClock.uptimeMillis() >= due) { ticks++; due += 30000 }
                if (pending) { pending = false; delivered++ }
                return (due - SystemClock.uptimeMillis()).toDouble()
            }
            @JvmStatic @Implementation fun frame(): ByteBuffer? { frames++; return null }
            @JvmStatic @Implementation fun requests(): IntArray? = null
            @JvmStatic @Implementation fun scrollers(): IntArray? = null
            @JvmStatic @Implementation fun fields(): IntArray? = null
            @JvmStatic @Implementation fun bars(): IntArray? = null
            @JvmStatic @Implementation fun chrome(): IntArray? = null
            @JvmStatic @Implementation fun timerHeld() = 0
            @JvmStatic @Implementation fun animating() = false
            @JvmStatic @Implementation fun owed() = false
            @JvmStatic @Implementation fun nextDue() = (due - SystemClock.uptimeMillis()).toDouble()
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
    @Test fun stoppedOwnerReportsUnappliedPublicationsAndUnknownReaders() {
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
                reports.publish()
                assertEquals(0, reports.mainWork)
            }
            owner.running(false)
            owner.ask { }
            assertTrue("an unapplied publication blocks background idle", Host.applying)
            owner.call { reports.report(reader, false, false, reports.publication) }
            owner.ask { }
            assertFalse("an acknowledged publication no longer blocks idle", Host.applying)
            owner.call { reports.replace(Any()) }
            owner.ask { }
            assertTrue("an unknown replacement reader blocks idle", Host.applying)
            owner.call {
                reports.report(reports.reader!!, false, false, reports.publication)
                reports.mainWork++
            }
            owner.ask { }
            assertTrue("outstanding main-thread work blocks idle", Host.applying)
        } finally {
            handler.removeCallbacksAndMessages(null)
            thread.quitSafely(); thread.join(5000)
            assertFalse(thread.isAlive)
        }
    }

}
