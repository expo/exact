package dev.exact.canvas

import android.app.Activity
import android.view.MotionEvent
import android.os.Looper
import android.widget.FrameLayout
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
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ScrollingTest.Host::class, ScrollingTest.Stretch::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ScrollingTest {
    /** Replace only the Rust transport; MotionEvents and the reader's scroll dispatch run normally. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            internal var rows = emptyList<Scrolling.Scroller>()
            var axis = 2
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun touch(action: Int, x: Float, y: Float, tool: Int) {}
            @JvmStatic @Implementation fun timerObservation(scrolling: Boolean, sampling: Boolean, applying: Boolean, idle: Boolean) = false
            @JvmStatic @Implementation fun frame(): java.nio.ByteBuffer? = null
            @JvmStatic @Implementation fun fields(): IntArray? = null
            @JvmStatic @Implementation fun bars(): IntArray? = null
            @JvmStatic @Implementation fun requests(): IntArray? = null
            @JvmStatic @Implementation fun scrollers(): IntArray? = null
            @JvmStatic @Implementation fun scrolled(id: Int, dx: Float, dy: Float, rev: Int) {
                rows.first { it.id == id }.let { it.nowX += dx; it.nowY += dy }
            }
            @JvmStatic @Implementation fun under(x: Float, y: Float): IntArray =
                intArrayOf(0, rows.size, 0) + rows.flatMap { s -> listOf(s.id, axis or (s.flags and 4), s.rev,
                    s.nowX.toBits(), s.nowY.toBits(), s.maxX.toBits(), s.maxY.toBits(),
                    0, 0, 1000f.toBits(), 1000f.toBits()) }.toIntArray()
        }
    }

    // Robolectric disables EdgeEffect even in native graphics mode. Enable Android's stretch
    // implementation; onPullDistance, distance and onRelease remain the framework's.
    @Implements(android.widget.EdgeEffect::class)
    class Stretch {
        @Implementation fun getCurrentEdgeEffectBehavior(): Int = 1
    }

    private class DragFixture(val axis: Int, innerSnaps: Boolean) : AutoCloseable {
        val activity = Robolectric.buildActivity(Activity::class.java).get()
        val view = ExactView(activity, false) {}
        val scrolling = ExactView::class.java.getDeclaredField("scrolling").apply { isAccessible = true }.get(view) as Scrolling
        val inner: Scrolling.Scroller
        val outer: Scrolling.Scroller
        init {
            view.running(false, host = false)
            view.ownerStarted(true)
            fun statement(id: Int, maximum: Float, snap: Boolean) = intArrayOf(id, axis, 0, 0, 0, 0, 0,
                maximum.toBits(), maximum.toBits(), if (snap) axis else 0, if (snap) 2 else 0) +
                if (snap) intArrayOf(0, 0, 200f.toBits(), 200f.toBits()) else intArrayOf()
            scrolling.told(statement(1, 200f, innerSnaps) + statement(2, 500f, true))
            val all = Scrolling::class.java.getDeclaredField("scrollers").apply { isAccessible = true }.get(scrolling) as Map<*, *>
            inner = all[1] as Scrolling.Scroller; outer = all[2] as Scrolling.Scroller
            Host.axis = axis; Host.rows = listOf(inner, outer)
        }
        fun event(action: Int, time: Long, position: Float) {
            val e = MotionEvent.obtain(0, time, action, if (axis == 1) position else 100f, if (axis == 2) position else 100f, 0)
            assertTrue(view.dispatchTouchEvent(e))
            e.recycle()
        }
        fun start() {
            event(MotionEvent.ACTION_DOWN, 0, 800f)
            event(MotionEvent.ACTION_MOVE, 20, 700f) // Recognition consumes the slop.
        }
        fun settle() {
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(1000))
            assertFalse("the snap finished", scrolling.timerActive)
        }
        override fun close() { scrolling.halt(); Host.rows = emptyList() }
    }

    @Test fun aPausedDragSettlesTheParentItReachedThroughAnExhaustedInner() {
        for (axis in listOf(1, 2)) for (parentOffset in listOf(60f, 140f)) DragFixture(axis, false).use { f ->
            val position = 700f - 200f - parentOffset
            f.start()
            f.event(MotionEvent.ACTION_MOVE, 40, position)
            assertEquals(200f, f.inner.at(axis == 1), 0f)
            assertEquals(parentOffset, f.outer.at(axis == 1), 0f)
            f.event(MotionEvent.ACTION_MOVE, 900, position)
            f.event(MotionEvent.ACTION_UP, 1000, position)
            f.settle()
            assertEquals(if (parentOffset < 100f) 0f else 200f, f.outer.at(axis == 1), 0f)
            assertEquals("settling the parent must not move the inner", 200f, f.inner.at(axis == 1), 0f)
        }
    }

    @Test fun interruptingASnapKeepsTheNestedGestureChain() {
        for (axis in listOf(1, 2)) DragFixture(axis, true).use { f ->
            f.start()
            f.event(MotionEvent.ACTION_MOVE, 40, 560f)
            f.event(MotionEvent.ACTION_MOVE, 900, 560f)
            f.event(MotionEvent.ACTION_UP, 1000, 560f)
            assertTrue("the inner snap is animating", f.scrolling.timerActive)
            f.event(MotionEvent.ACTION_DOWN, 1100, 560f)
            f.event(MotionEvent.ACTION_MOVE, 1120, 360f)
            assertEquals(200f, f.inner.at(axis == 1), 0f)
            assertEquals("the remaining drag reaches the parent", 140f, f.outer.at(axis == 1), 0f)
            f.event(MotionEvent.ACTION_MOVE, 1900, 360f)
            f.event(MotionEvent.ACTION_UP, 2000, 360f)
            f.settle()
            assertEquals(200f, f.outer.at(axis == 1), 0f)
        }
    }

    @Test fun aFlingSettlesAParentThatTheFingerNeverMoved() {
        for (axis in listOf(1, 2)) DragFixture(axis, false).use { f ->
            f.start()
            f.event(MotionEvent.ACTION_MOVE, 40, 650f)
            assertEquals(50f, f.inner.at(axis == 1), 0f)
            assertEquals(0f, f.outer.at(axis == 1), 0f)
            f.event(MotionEvent.ACTION_UP, 50, 625f)
            assertTrue(f.scrolling.timerActive)
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(10))
            assertFalse(f.scrolling.timerActive)
            assertEquals(200f, f.inner.at(axis == 1), 0f)
            assertEquals("the fling reached the parent and it settled", 200f, f.outer.at(axis == 1), 0f)
        }
    }

    @Test fun aFastReleaseUsesTheSnapProjectionThroughTouchDispatch() {
        for (axis in listOf(1, 2)) DragFixture(axis, true).use { f ->
            f.start()
            f.event(MotionEvent.ACTION_MOVE, 40, 650f)
            f.event(MotionEvent.ACTION_UP, 50, 625f)
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(10))
            assertFalse(f.scrolling.timerActive)
            assertEquals(200f, f.inner.at(axis == 1), 0f)
            assertEquals("the inner snap consumes the projected fling", 0f, f.outer.at(axis == 1), 0f)
        }
    }

    @Test fun aCurrentOffsetOnlyTargetDoesNotConsumeTheFling() {
        for (axis in listOf(1, 2)) DragFixture(axis, true).use { f ->
            f.start()
            f.event(MotionEvent.ACTION_MOVE, 40, 650f)
            f.inner.snaps = floatArrayOf(50f, 50f)
            f.event(MotionEvent.ACTION_UP, 50, 625f)
            assertTrue("a zero-distance snap leaves the ordinary fling running", f.scrolling.timerActive)
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(10))
            assertFalse(f.scrolling.timerActive)
            assertTrue("the fling is handed to the parent", f.outer.at(axis == 1) > 0f)
        }
    }

    @Test fun aZeroDistanceInnerSnapPreservesVelocityAfterTheParentMoved() {
        for (axis in listOf(1, 2)) DragFixture(axis, true).use { f ->
            f.inner.maxX = 800f; f.inner.maxY = 800f
            f.inner.snaps = floatArrayOf(0f, 0f, 200f, 200f, 400f, 400f)
            f.start()
            f.event(MotionEvent.ACTION_MOVE, 40, -160f)
            f.event(MotionEvent.ACTION_MOVE, 60, 240f)
            assertEquals(400f, f.inner.at(axis == 1), 0f)
            assertEquals(60f, f.outer.at(axis == 1), 0f)
            f.event(MotionEvent.ACTION_MOVE, 900, 290f)
            f.event(MotionEvent.ACTION_MOVE, 920, 240f)
            assertEquals(400f, f.inner.at(axis == 1), 0f)
            f.event(MotionEvent.ACTION_UP, 930, 215f)
            val fling = Scrolling::class.java.getDeclaredField("fling").apply { isAccessible = true }.get(f.scrolling) as android.widget.OverScroller
            val distance = if (axis == 1) fling.finalX else fling.finalY
            assertTrue("the forward fling survives instead of settling the parent backward: $distance", distance > 0)
            assertTrue(f.scrolling.timerActive)
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(10))
            assertFalse(f.scrolling.timerActive)
        }
    }

    @Test fun releasingAStretchedEdgeSettlesTheMandatorySnap() {
        for (axis in listOf(1, 2)) DragFixture(axis, true).use { f ->
            f.inner.flags = axis or 4
            f.inner.maxX = 500f; f.inner.maxY = 500f
            f.outer.maxX = 0f; f.outer.maxY = 0f
            f.start()
            f.event(MotionEvent.ACTION_MOVE, 40, 100f)
            assertEquals(500f, f.inner.at(axis == 1), 0f)
            assertTrue("the render-node edge is stretched", f.inner.edges.any { it != null && it.distance > 0f })
            f.event(MotionEvent.ACTION_MOVE, 900, 100f)
            f.event(MotionEvent.ACTION_UP, 1000, 100f)
            f.settle()
            assertEquals("a boundary beyond the last area must still settle", 200f, f.inner.at(axis == 1), 0f)
        }
    }

    private class Fixture(val axis: Int) {
        val activity = Robolectric.buildActivity(Activity::class.java).get()
        val view = ExactView(activity, false) {}
        val parent = FrameLayout(activity).also { it.addView(view) }
        val scrolling = Scrolling(view)
        val scrollers = Scrolling::class.java.getDeclaredField("scrollers").apply { isAccessible = true }
        val indicate = Scrolling::class.java.getDeclaredMethod("indicate", Scrolling.Scroller::class.java).apply { isAccessible = true }
        fun scroller() = (scrollers.get(scrolling) as Map<*, *>)[1] as Scrolling.Scroller
        fun told(flags: Int) = scrolling.told(intArrayOf(1, flags, 0, 0, 0, 0, 0, 500f.toBits(), 500f.toBits(), 0, 0))
        fun down() {
            val e = MotionEvent.obtain(0, 0, MotionEvent.ACTION_DOWN, 100f, 100f, 0)
            scrolling.touch(e); e.recycle()
        }
        fun reply(policy: Int) = intArrayOf(0, 1, 0, 1, axis or policy, 0, 0, 0,
            500f.toBits(), 500f.toBits(), 0, 0, 200f.toBits(), 200f.toBits())
        fun under(reply: IntArray) = scrolling.under(scrolling.serial, reply)
        fun indicate() = indicate.invoke(scrolling, scroller())
    }

    @Test fun hitPolicyIsAFallbackUntilStatementsArrive() {
        for (axis in listOf(1, 2)) {
            val f = Fixture(axis)
            f.down()
            f.under(f.reply(0))
            f.indicate()
            assertEquals(2, f.parent.childCount)
            f.down()
            f.under(f.reply(8))
            assertEquals("none in a hit reply removes an indicator before any statement", 1, f.parent.childCount)
            f.indicate()
            assertEquals(1, f.parent.childCount)
            f.told(axis)
            f.indicate()
            assertEquals(2, f.parent.childCount)
            f.told(axis or 8)
            assertEquals("a live none removes the existing indicator", 1, f.parent.childCount)
            f.told(axis)
            f.indicate()
            assertEquals(2, f.parent.childCount)
        }
    }

    @Test fun anOldAutoReplyCannotOverrideANewerNoneStatement() = oldReply(0, 8)

    @Test fun anOldNoneReplyCannotOverrideANewerAutoStatement() = oldReply(8, 0)

    @Test fun aZeroVelocityReleaseSettlesEverySnappingContainerTheDragMoved() {
        for (axis in listOf(1, 2)) DragFixture(axis, true).use { f ->
            f.start()
            f.event(MotionEvent.ACTION_MOVE, 40, 360f)
            f.event(MotionEvent.ACTION_MOVE, 60, 420f)
            assertEquals(140f, f.inner.at(axis == 1), 0f)
            assertEquals(140f, f.outer.at(axis == 1), 0f)
            f.event(MotionEvent.ACTION_MOVE, 900, 420f)
            f.event(MotionEvent.ACTION_UP, 1000, 420f)
            f.settle()
            assertEquals(200f, f.inner.at(axis == 1), 0f)
            assertEquals(200f, f.outer.at(axis == 1), 0f)
        }
    }

    @Test fun projectionUsesNearestRangeAndKeepsOversizedAreasFree() {
        val ranges = floatArrayOf(500f, 800f, 200f, 200f, 0f, 0f)
        assertEquals(0f, nearestSnap(-30f, ranges)!!, 0f)
        assertEquals(0f, nearestSnap(100f, ranges)!!, 0f)
        assertEquals(200f, nearestSnap(350f, ranges)!!, 0f)
        assertEquals(650f, nearestSnap(650f, ranges)!!, 0f)
        assertEquals(800f, nearestSnap(1200f, ranges)!!, 0f)
        assertNull(nearestSnap(100f, floatArrayOf()))
    }

    @Test fun flingUsesAndroidProjectionAndLiveStyles() {
        for (axis in listOf(1, 2)) {
            val f = Fixture(axis)
            val ranges = floatArrayOf(0f, 0f, 250f, 250f, 550f, 550f, 800f, 800f)
            fun statement(snapAxis: Int, rev: Int = 0) = intArrayOf(1, axis, rev, 250f.toBits(), 250f.toBits(), 0, 0,
                800f.toBits(), 800f.toBits(), snapAxis, if (snapAxis == 0) 0 else 4) +
                (if (snapAxis == 0) intArrayOf() else ranges.map { it.toBits() }.toIntArray())
            f.scrolling.told(statement(axis))
            val snap = Scrolling::class.java.getDeclaredMethod("snap", Float::class.javaPrimitiveType).apply { isAccessible = true }
            val fling = Scrolling::class.java.getDeclaredField("fling").apply { isAccessible = true }.get(f.scrolling) as android.widget.OverScroller
            for (velocity in listOf(-1800f, 1800f)) {
                for ((name, value) in listOf("sideways" to (axis == 1), "active" to listOf(f.scroller()))) {
                    Scrolling::class.java.getDeclaredField(name).apply { isAccessible = true }.set(f.scrolling, value)
                }
                val oracle = android.widget.OverScroller(f.activity)
                if (axis == 1) oracle.fling(0, 0, velocity.toInt(), 0, Int.MIN_VALUE, Int.MAX_VALUE, 0, 0)
                else oracle.fling(0, 0, 0, velocity.toInt(), 0, 0, Int.MIN_VALUE, Int.MAX_VALUE)
                val projected = 250f + if (axis == 1) oracle.finalX else oracle.finalY
                assertEquals(true, snap.invoke(f.scrolling, velocity))
                val expected = nearestSnap(projected, ranges)!! - 250f
                assertEquals(expected.toInt(), if (axis == 1) fling.finalX else fling.finalY)
                f.scrolling.halt()
            }
            f.scrolling.told(statement(0))
            assertFalse(f.scrolling.timerActive)
            assertEquals(false, snap.invoke(f.scrolling, 1800f))
            f.scrolling.told(statement(axis))
            assertEquals(true, snap.invoke(f.scrolling, 1800f))
            f.scrolling.told(statement(axis, 1))
            assertFalse("an app scroll cancels the old snap", f.scrolling.timerActive)
            f.scrolling.told(intArrayOf())
            val scrollers = f.scrollers.get(f.scrolling) as Map<*, *>
            assertTrue(scrollers.isEmpty())
        }
    }

    private fun oldReply(oldPolicy: Int, newPolicy: Int) {
        for (axis in listOf(1, 2)) {
            val f = Fixture(axis)
            f.told(axis or oldPolicy)
            f.down()
            val old = f.reply(oldPolicy)
            f.told(axis or newPolicy)
            f.under(old)
            f.indicate()
            assertEquals("the statement's policy survives an older hit reply", newPolicy != 0, f.scroller().barless)
            assertEquals(if (newPolicy == 0) 2 else 1, f.parent.childCount)
        }
    }
}
