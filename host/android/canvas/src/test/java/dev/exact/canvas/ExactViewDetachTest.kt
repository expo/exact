package dev.exact.canvas

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.RenderNode
import android.os.Looper
import android.view.Choreographer
import android.view.ViewGroup
import java.nio.IntBuffer
import java.time.Duration
import java.util.concurrent.CountDownLatch
import java.util.concurrent.ExecutorService
import java.util.concurrent.TimeUnit
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.LooperMode
import org.robolectric.annotation.RealObject
import org.robolectric.shadow.api.Shadow
import org.robolectric.shadows.ShadowNativeBitmap
import org.robolectric.shadows.ShadowNativeRenderNode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ExactViewDetachTest.Host::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@LooperMode(LooperMode.Mode.PAUSED)
class ExactViewDetachTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun displayFrame() {}
            @JvmStatic @Implementation fun frame(): java.nio.ByteBuffer? = null
            @JvmStatic @Implementation fun requests(): IntArray? = null
            @JvmStatic @Implementation fun timerObservation(a: Boolean, b: Boolean, c: Boolean, d: Boolean) = false
        }
    }

    class HeldWork {
        val entered = CountDownLatch(1)
        val finish = CountDownLatch(1)
        fun hold() {
            entered.countDown()
            check(finish.await(5, TimeUnit.SECONDS)) { "worker was not released" }
        }
    }

    @Implements(RenderNode::class)
    class Recording : ShadowNativeRenderNode() {
        @RealObject private lateinit var node: RenderNode
        @Implementation fun endRecording() {
            held?.hold()
            Shadow.directlyOn<Any, RenderNode>(node, RenderNode::class.java, "endRecording")
        }
        companion object { var held: HeldWork? = null }
    }

    @Implements(Bitmap::class)
    class Copying : ShadowNativeBitmap() {
        @Implementation fun copy(config: Bitmap.Config, mutable: Boolean): Bitmap {
            check(config == Bitmap.Config.HARDWARE && !mutable)
            held.hold()
            return result
        }
        companion object {
            lateinit var held: HeldWork
            lateinit var result: Bitmap
        }
    }

    private fun view(): ExactView {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        return ExactView(activity, false) {}.also {
            it.started = true
            activity.setContentView(it)
            assertTrue(it.isAttachedToWindow)
        }
    }
    private fun ExactView.detach() {
        (parent as ViewGroup).removeView(this)
        assertFalse("the framework cleared attachment state", isAttachedToWindow)
    }
    private fun field(name: String) = ExactView::class.java.getDeclaredField(name).apply { isAccessible = true }
    @Suppress("UNCHECKED_CAST")
    private fun <T> ExactView.get(name: String): T = field(name).get(this) as T
    private fun ExactView.call(name: String, vararg args: Any) = ExactView::class.java.declaredMethods
        .single { it.name == name }.apply { isAccessible = true }.invoke(this, *args)

    @Test fun detachCancelsEveryPostedFrame() {
        for (stopped in listOf(false, true)) detachWithFrames(stopped)
    }

    private fun detachWithFrames(stopped: Boolean) {
        val view = view()
        if (stopped) view.running(false, host = false)
        val names = listOf("laterFrame", "regroupFrame", "turn", "sampler", "holder")
        val calls = names.associateWith { 0 }.toMutableMap()
        val frames = Choreographer.getInstance()
        for (name in names) {
            val original = view.get<Choreographer.FrameCallback>(name)
            val observed = Choreographer.FrameCallback { time ->
                calls[name] = calls.getValue(name) + 1
                original.doFrame(time)
            }
            field(name).set(view, observed)
            frames.postFrameCallback(observed)
        }
        for (flag in listOf("laterPosted", "regrouping", "turnPosted", "samplerPosted")) field(flag).setBoolean(view, true)
        field("holdFrom").setLong(view, System.nanoTime())
        var frameDelivered = false
        frames.postFrameCallback { frameDelivered = true }

        view.detach()
        for (flag in listOf("laterPosted", "regrouping", "turnPosted", "samplerPosted")) assertFalse(flag, view.get(flag))
        assertEquals(0L, view.get<Long>("holdFrom"))
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(32))

        assertTrue("a frame was delivered after detach", frameDelivered)
        for (name in names) assertEquals("$name did not execute", 0, calls.getValue(name))
        assertTrue(view.get<ExecutorService>("worker").isShutdown)
    }

    @Test fun detachStopsAFlingerPostedAfterTheActivityStopped() {
        val view = view()
        view.running(false, host = false)
        val scrolling = view.get<Scrolling>("scrolling")
        val flinger = Scrolling::class.java.getDeclaredField("flinger").apply { isAccessible = true }
        val original = flinger.get(scrolling) as Choreographer.FrameCallback
        var calls = 0
        flinger.set(scrolling, Choreographer.FrameCallback { time -> calls++; original.doFrame(time) })
        // A late snap answer starts a settle after onStop has already set running=false.
        Scrolling::class.java.getDeclaredMethod("settle", Int::class.javaPrimitiveType, Int::class.javaPrimitiveType)
            .apply { isAccessible = true }.invoke(scrolling, 100, 300)
        view.detach()
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(32))
        assertEquals("the late snap's flinger did not execute", 0, calls)
        val fling = Scrolling::class.java.getDeclaredField("fling").apply { isAccessible = true }
            .get(scrolling) as android.widget.OverScroller
        assertTrue(fling.isFinished)
    }

    @Test fun aDetachedViewRecordsNoRows() {
        val view = view()
        view.detach()
        view.call("record", 1, 0, RenderNode("row"), IntBuffer.allocate(0), 0, 0)
        assertTrue(view.get<Map<Int, Int>>("pending").isEmpty())
    }

    @Test fun aDetachedViewMakesNoHardwarePictures() {
        val view = view()
        view.detach()
        val bitmap = Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)
        view.call("harden", 1, bitmap)
        assertFalse(bitmap.isRecycled)
        bitmap.recycle()
    }

    @Test @Config(shadows = [Recording::class])
    fun aRecordingCompletedAfterDetachDiscardsItsDisplayList() {
        val view = view()
        val worker = view.get<ExecutorService>("worker")
        val node = RenderNode("row").apply { setPosition(0, 0, 1, 1) }
        val held = HeldWork()
        Recording.held = held
        try {
            view.call("record", 1, 0, node, IntBuffer.allocate(0), 0, 0)
            assertTrue("the recording started", held.entered.await(5, TimeUnit.SECONDS))
            view.detach()
            held.finish.countDown()
            assertTrue(worker.awaitTermination(5, TimeUnit.SECONDS))
            assertTrue("the worker finished its display list", node.hasDisplayList())
            shadowOf(Looper.getMainLooper()).idle()
            assertFalse("the detached row was discarded on main", node.hasDisplayList())
        } finally {
            held.finish.countDown()
            worker.shutdown()
            assertTrue(worker.awaitTermination(5, TimeUnit.SECONDS))
            Recording.held = null
            node.discardDisplayList()
        }
    }

    @Test @Config(shadows = [Copying::class])
    fun aHardwareCopyCompletedAfterDetachIsRecycled() {
        val view = view()
        val worker = view.get<ExecutorService>("worker")
        val bitmap = Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)
        val hardware = Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)
        val held = HeldWork()
        Copying.held = held
        Copying.result = hardware
        try {
            view.call("harden", 1, bitmap)
            assertTrue("the hardware copy started", held.entered.await(5, TimeUnit.SECONDS))
            view.detach()
            held.finish.countDown()
            assertTrue(worker.awaitTermination(5, TimeUnit.SECONDS))
            assertFalse("cleanup is still queued on main", hardware.isRecycled)
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue("the detached hardware copy was recycled", hardware.isRecycled)
        } finally {
            held.finish.countDown()
            worker.shutdown()
            assertTrue(worker.awaitTermination(5, TimeUnit.SECONDS))
            bitmap.recycle()
            hardware.recycle()
        }
    }

    @Test fun aBitmapFreedAfterDetachSurvivesAnActiveRecording() {
        freeDuringRecording(beforeDetach = false)
    }

    @Test fun aBitmapFreedBeforeDetachIsRecycledAfterQueuedRecordings() {
        freeDuringRecording(beforeDetach = true)
    }

    private fun freeDuringRecording(beforeDetach: Boolean) {
        val view = view()
        val bitmap = Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)
        view.get<MutableMap<Int, Bitmap>>("images")[1] = bitmap
        val worker = view.get<ExecutorService>("worker")
        val entered = CountDownLatch(1)
        val draw = CountDownLatch(1)
        val recording = worker.submit {
            val player = view.get<Player>("background")
            player.image = {
                entered.countDown()
                check(draw.await(5, TimeUnit.SECONDS)) { "recording was not released" }
                bitmap
            }
            val node = RenderNode("row")
            val canvas = node.beginRecording(1, 1)
            try {
                val ops = IntBuffer.wrap(intArrayOf(8, 1, 0, 0, 1f.toBits(), 1f.toBits()))
                player.replay(canvas, ops, 0, ops.limit())
                assertFalse("the recording still owns its bitmap", bitmap.isRecycled)
            } finally {
                node.endRecording()
                node.discardDisplayList()
            }
        }
        try {
            assertTrue("recording entered replay", entered.await(5, TimeUnit.SECONDS))
            if (!beforeDetach) view.detach()
            view.call("ingest", IntBuffer.wrap(intArrayOf(12, 1)), 0, 2)
            if (beforeDetach) view.detach()
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(view.get<Map<Int, Bitmap>>("images").isEmpty())
            assertFalse("pixels remain owned by the held recording", bitmap.isRecycled)
            draw.countDown()
            recording.get(5, TimeUnit.SECONDS)
            assertTrue(worker.awaitTermination(5, TimeUnit.SECONDS))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals("only a release queued before shutdown recycles", beforeDetach, bitmap.isRecycled)
        } finally {
            draw.countDown()
            worker.shutdown()
            if (worker.awaitTermination(5, TimeUnit.SECONDS)) bitmap.recycle()
        }
    }
}
