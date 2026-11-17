package dev.exact.canvas

import android.app.Activity
import android.view.MotionEvent
import android.view.ViewConfiguration
import android.widget.FrameLayout
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.LooperMode
import org.robolectric.shadow.api.Shadow

/**
 * Fingers on a zoomable scroller (LLP 1057.001.000 §5, [Zooming]), through [ExactView.onTouchEvent] as the platform
 * delivers them: what exact2 is told of the pinch, the double tap and the pan, and which touches it hears.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ZoomTouchTest.Thread::class, ZoomTouchTest.Host::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@LooperMode(LooperMode.Mode.PAUSED)
class ZoomTouchTest {
    @Implements(Owner::class, isInAndroidSdk = false)
    class Thread {
        /** Each touch exact2 heard: 0 down, 1 up, 2 move, 3 cancel. */
        val touches = ArrayList<Int>()
        /** How far the reader moved each scroller, by id, along each axis. */
        val moved = HashMap<Int, FloatArray>()
        val answers = ArrayDeque<(IntArray) -> Unit>()
        @Implementation fun __constructor__(hwbuf: Boolean, fast: Boolean, priority: Int, intent: (String) -> String?) {}
        @Implementation fun under(x: Float, y: Float, done: (IntArray) -> Unit) { answers.addLast(done) }
        @Implementation fun touch(action: Int, x: Float, y: Float, tool: Int, waitMs: Long): Boolean { touches.add(action); return false }
        @Implementation fun finger(action: Int, id: Int, x: Float, y: Float, waitMs: Long) = false
        @Implementation fun scrolled(id: Int, dx: Float, dy: Float, rev: Int, seq: Int) {
            val m = moved.getOrPut(id) { FloatArray(2) }
            m[0] += dx; m[1] += dy
        }
        @Implementation fun answer(waitMs: Long, f: () -> Unit) = false
        @Implementation fun call(f: () -> Unit) = f()
        @Implementation fun input() {}
        @Implementation fun held(on: Boolean) {}
        @Implementation fun running(on: Boolean) {}
        @Implementation fun timerReader(view: Any) {}
        @Implementation fun timerReport(view: Any, scrolling: Boolean, sampling: Boolean, applied: Long, began: Long, ended: Long) {}
    }

    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            /** What exact2's thread was told, in order. */
            val told = ArrayList<String>()
            val factors = ArrayList<Float>()
            @JvmStatic @Implementation fun scrollDrag(id: Int, ended: Boolean, vx: Float, vy: Float, rest: Boolean, restX: Float, restY: Float, flings: Boolean) {
                told.add(if (!ended) "start $id" else if (flings) "end $id flings" else "end $id")
            }
            @JvmStatic @Implementation fun flingEnded(id: Int) { told.add("rest $id") }
            @JvmStatic @Implementation fun zoom(id: Int, phase: Int, factor: Float, x0: Float, y0: Float, x1: Float, y1: Float) {
                if (phase == 1) factors.add(factor) else told.add(if (phase == 0) "pinch $id" else "pinched $id")
            }
            @JvmStatic @Implementation fun zoomTap(phase: Int, x: Float, y: Float) { told.add(if (phase == 0) "second tap" else "double tap") }
        }
    }

    private lateinit var activity: Activity
    private lateinit var view: ExactView
    private lateinit var thread: Thread
    private val slop get() = ViewConfiguration.get(activity).scaledTouchSlop
    private var time = 0L

    @Before fun setUp() {
        activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        activity.intent.putExtra("warm", "0")
        view = ExactView(activity, false) {}
        activity.setContentView(FrameLayout(activity).also { it.addView(view, FrameLayout.LayoutParams(400, 800)) })
        val owner = Owner(false, false, 0) { null }
        thread = Shadow.extract(owner)
        Host.told.clear(); Host.factors.clear()
        view.owner = owner
        view.ownerStarted(true)
        time = 0L
    }

    /** Scroller [id], both axes ([flags] adds the zoom's bits), at the origin of [max] travel, its port the view's
     * top 400 by 800. */
    private fun scroller(id: Int, flags: Int, max: Float, axes: Int = 3) =
        intArrayOf(id, axes or flags, 0, 0, 0, max.toBits(), max.toBits(), 0, 0, 400f.toBits(), 800f.toBits())

    private fun reply(vararg scrollers: IntArray) = intArrayOf(0, scrollers.size, 0) + scrollers.fold(IntArray(0)) { a, s -> a + s }

    private fun answer(words: IntArray) = thread.answers.removeFirst()(words)

    private fun touch(action: Int, vararg fingers: Pair<Float, Float>, index: Int = 0, step: Long = 10) {
        val props = Array(fingers.size) { k -> MotionEvent.PointerProperties().apply { id = k; toolType = MotionEvent.TOOL_TYPE_FINGER } }
        val coords = Array(fingers.size) { k -> MotionEvent.PointerCoords().apply { x = fingers[k].first; y = fingers[k].second } }
        val e = MotionEvent.obtain(0, time, action or (index shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), fingers.size,
            props, coords, 0, 0, 1f, 1f, 0, 0, android.view.InputDevice.SOURCE_TOUCHSCREEN, 0)
        time += step
        view.dispatchTouchEvent(e)
        e.recycle()
    }

    private val zoomable = scroller(1, Scrolling.ZOOM_FLAG or Scrolling.DOUBLE_TAP_FLAG, 0f)

    @Test fun aPinchIsTheZoomsAndExact2LetsGoOfTheTouch() {
        touch(MotionEvent.ACTION_DOWN, 100f to 400f)
        answer(reply(zoomable))
        touch(MotionEvent.ACTION_POINTER_DOWN, 100f to 400f, 300f to 400f, index = 1)
        for (k in 1..6) touch(MotionEvent.ACTION_MOVE, 100f - 10f * k to 400f, 300f + 10f * k to 400f)
        touch(MotionEvent.ACTION_POINTER_UP, 40f to 400f, 360f to 400f, index = 1)
        touch(MotionEvent.ACTION_UP, 40f to 400f)
        assertEquals(listOf("pinch 1", "pinched 1"), Host.told)
        // The fingers went from 200 apart to 320: the factors multiply to the ratio from where the detector's span
        // slop let the pinch begin.
        val total = Host.factors.fold(1f) { a, f -> a * f }
        assertTrue("$total", total > 1.2f && total <= 1.6f + 1e-3f)
        assertEquals("exact2's touch is cancelled when the pinch begins", 3, thread.touches.last())
        assertFalse(thread.touches.contains(1))
    }

    @Test fun aDoubleTapZoomsAndItsSecondTapPressesNothing() {
        for (k in 0..1) {
            touch(MotionEvent.ACTION_DOWN, 200f to 400f)
            answer(reply(zoomable))
            touch(MotionEvent.ACTION_UP, 200f to 400f, step = 80)
        }
        assertEquals(listOf("second tap", "double tap"), Host.told)
        // The first tap is exact2's (its press waits there for a second); the second's down is cancelled.
        assertEquals(listOf(0, 1, 0, 3), thread.touches)
    }

    @Test fun aDoubleTapWhereTheAnswerSaysNoneIsTwoTaps() {
        val plain = scroller(1, Scrolling.ZOOM_FLAG, 0f)
        for (k in 0..1) {
            touch(MotionEvent.ACTION_DOWN, 200f to 400f)
            answer(reply(plain))
            touch(MotionEvent.ACTION_UP, 200f to 400f, step = 80)
        }
        assertTrue(Host.told.isEmpty())
        assertEquals(listOf(0, 1, 0, 1), thread.touches)
    }

    @Test fun aDoubleTapHeldAndDraggedScalesAndDoesNotTapZoom() {
        touch(MotionEvent.ACTION_DOWN, 200f to 400f)
        answer(reply(zoomable))
        touch(MotionEvent.ACTION_UP, 200f to 400f, step = 80)
        touch(MotionEvent.ACTION_DOWN, 200f to 400f)
        answer(reply(zoomable))
        for (k in 1..8) touch(MotionEvent.ACTION_MOVE, 200f to 400f + 15f * k)
        touch(MotionEvent.ACTION_UP, 200f to 520f)
        assertEquals(listOf("second tap", "pinch 1", "pinched 1"), Host.told)
        assertTrue("dragging down zooms in", Host.factors.fold(1f) { a, f -> a * f } > 1f)
    }

    @Test fun aDragOfAZoomedScrollerPansItAlongBothAxes() {
        val zoomed = scroller(1, Scrolling.ZOOM_FLAG, 500f)
        touch(MotionEvent.ACTION_DOWN, 200f to 400f)
        answer(reply(zoomed))
        for (k in 1..5) touch(MotionEvent.ACTION_MOVE, 200f - 12f * k to 400f - 8f * k, step = 100)
        touch(MotionEvent.ACTION_UP, 140f to 360f, step = 100)
        val m = thread.moved[1]!!
        assertTrue("x ${m[0]}", m[0] > 0f)
        assertTrue("y ${m[1]}", m[1] > 0f)
        assertEquals("start 1", Host.told.first())
        assertTrue(Host.told[1], Host.told[1].startsWith("end 1"))
        assertEquals("exact2 hears the move within the slop, then is cancelled", listOf(0, 2, 3), thread.touches)
    }

    @Test fun aDragOfAScrollerThatCannotMoveScrollsTheOneOutside() {
        // The photo at its fit: the page outside it scrolls, as it would over anything else.
        val page = intArrayOf(2, 2, 0, 0, 0, 0, 900f.toBits(), 0, 0, 400f.toBits(), 800f.toBits())
        touch(MotionEvent.ACTION_DOWN, 200f to 400f)
        answer(reply(zoomable, page))
        for (k in 1..5) touch(MotionEvent.ACTION_MOVE, 200f to 400f - 20f * k)
        touch(MotionEvent.ACTION_UP, 200f to 300f, step = 500)
        assertNull(thread.moved[1])
        assertEquals(100f - slop, thread.moved[2]!![1], 0.01f)
    }
}
