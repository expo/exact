package dev.exact.canvas

import android.app.Activity
import android.view.MotionEvent
import android.widget.FrameLayout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ScrollingTest {
    private class Fixture(val axis: Int) {
        val activity = Robolectric.buildActivity(Activity::class.java).get()
        val view = ExactView(activity, false) {}
        val parent = FrameLayout(activity).also { it.addView(view) }
        val scrolling = Scrolling(view)
        val scrollers = Scrolling::class.java.getDeclaredField("scrollers").apply { isAccessible = true }
        val indicate = Scrolling::class.java.getDeclaredMethod("indicate", Scrolling.Scroller::class.java).apply { isAccessible = true }
        fun scroller() = (scrollers.get(scrolling) as Map<*, *>)[1] as Scrolling.Scroller
        fun told(flags: Int) = scrolling.told(intArrayOf(1, flags, 0, 0, 0, 0, 0, 500f.toBits(), 500f.toBits()))
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

    @Test fun aTouchsAnswerTellsEachScrollersSnap() {
        val f = Fixture(1)
        f.down()
        val module = IntArray(10)
        // Sideways, mandatory: a 200 snapport, two areas, the second an `always` one; nothing up and down.
        val snap = intArrayOf(1 or 4, 200f.toBits(), 2, 0, 0, 0, 300f.toBits(), 300f.toBits(), 1, 0, 0)
        f.under(f.reply(0) + module + snap)
        val s = f.scroller()
        assertTrue(s.snapMandatory)
        assertEquals(200f, s.snapPort[0], 0f)
        assertArrayEquals(floatArrayOf(0f, 0f, 300f, 300f), s.snaps[0], 0f)
        assertArrayEquals(booleanArrayOf(false, true), s.stops[0])
        assertEquals(0f, s.snapPort[1], 0f)
        assertEquals(0, s.snaps[1].size)
    }

    @Test fun aSnapRestsAtTheNearestPositionShortOfAnAlwaysOne() {
        val snaps = floatArrayOf(0f, 0f, 250f, 250f, 550f, 550f, 800f, 800f)
        val normal = BooleanArray(4)
        assertEquals(550f, snapRest(snaps, normal, 600f, 250f, 250f, 0, true, 400f)!!, 0f)
        assertEquals("the smaller of two as near", 250f, snapRest(snaps, normal, 400f, 250f, 250f, 0, true, 400f)!!, 0f)
        val always = booleanArrayOf(true, true, true, true)
        assertEquals("not past the next `always` position", 550f, snapRest(snaps, always, 780f, 250f, 250f, 0, true, 400f)!!, 0f)
        assertEquals("nor the previous one going back", 250f, snapRest(snaps, always, 10f, 550f, 550f, 0, true, 400f)!!, 0f)
        assertEquals(800f, snapRest(snaps, booleanArrayOf(false, false, false, true), 780f, 250f, 250f, 0, true, 400f)!!, 0f)
        // An area longer than the snapport rests anywhere it covers it, and an `always` one is left at its end.
        val long = floatArrayOf(0f, 0f, 300f, 900f, 1200f, 1200f)
        assertEquals(640f, snapRest(long, BooleanArray(3), 640f, 400f, 400f, 0, true, 400f)!!, 0f)
        assertEquals(900f, snapRest(long, booleanArrayOf(false, true, false), 1150f, 400f, 400f, 0, true, 400f)!!, 0f)
        // `proximity`: only within a third of the snapport of where it would rest.
        assertEquals(250f, snapRest(snaps, normal, 380f, 250f, 250f, 0, false, 400f)!!, 0f)
        assertNull(snapRest(snaps, normal, 400f, 250f, 250f, 0, false, 400f))
        assertNull(snapRest(FloatArray(0), BooleanArray(0), 400f, 250f, 250f, 0, true, 400f))
    }

    @Test fun aFlingReachesAtLeastTheNextPositionFromWhereItsDragBegan() {
        val snaps = floatArrayOf(0f, 0f, 300f, 300f, 600f, 600f, 900f, 900f)
        val normal = BooleanArray(4)
        // A short quick fling projected under half a card: on to the next card, not back.
        assertEquals(300f, snapRest(snaps, normal, 120f, 80f, 0f, 1, true, 300f)!!, 0f)
        assertEquals(0f, snapRest(snaps, normal, 120f, 80f, 0f, 0, true, 300f)!!, 0f)
        // Counted from where the drag began: a drag that already passed a card adds none.
        assertEquals(300f, snapRest(snaps, normal, 420f, 400f, 0f, 1, true, 300f)!!, 0f)
        // Back the other way, and a long fling still rests where it projects.
        assertEquals(300f, snapRest(snaps, normal, 560f, 580f, 600f, -1, true, 300f)!!, 0f)
        assertEquals(900f, snapRest(snaps, normal, 1500f, 80f, 0f, 1, true, 300f)!!, 0f)
        // `always` still holds a long one at the next card.
        assertEquals(300f, snapRest(snaps, BooleanArray(4) { true }, 1500f, 80f, 0f, 1, true, 300f)!!, 0f)
    }

    @Test fun aListsAreasAroundTheProjectionJoinThoseTheTouchTold() {
        val s = Scrolling.Scroller(1)
        s.snaps[0] = floatArrayOf(0f, 0f, 300f, 300f, 600f, 600f, 900f, 900f)
        s.stops[0] = BooleanArray(4)
        // Past the areas the touch told, the last of them is the nearest: behind the projection.
        assertEquals(900f, snapRest(s.snaps[0], s.stops[0], 6100f, 800f, 800f, 1, true, 300f)!!, 0f)
        val b = { v: Float -> v.toBits() }
        mergeSnaps(s, 0, intArrayOf(2, b(6000f), b(6000f), 0, b(6300f), b(6300f), 0))
        assertEquals(6000f, snapRest(s.snaps[0], s.stops[0], 6100f, 800f, 800f, 1, true, 300f)!!, 0f)
        // And an `always` row on the way, which the answer names too, holds it.
        mergeSnaps(s, 0, intArrayOf(1, b(3000f), b(3000f), 1))
        assertEquals(3000f, snapRest(s.snaps[0], s.stops[0], 6100f, 800f, 800f, 1, true, 300f)!!, 0f)
        mergeSnaps(s, 0, intArrayOf(0))
        assertEquals(7, s.stops[0].size)
    }

    /** A list's release, sideways from [at] at [v] px/s, with the snap the touch told; what it asked for comes back. */
    private class ListRelease(mandatory: Boolean, val at: Float = 300f) {
        val f = Fixture(1)
        val field = { name: String -> Scrolling::class.java.getDeclaredField(name).apply { isAccessible = true } }
        val s: Scrolling.Scroller
        var asked: Pair<Float, (IntArray) -> Unit>? = null
        init {
            f.told(1)
            s = f.scroller()
            s.maxX = 100000f; s.nowX = at
            s.snapList = true; s.snapMandatory = mandatory; s.snapPort[0] = 300f
            s.snaps[0] = floatArrayOf(0f, 0f, 300f, 300f, 600f, 600f); s.stops[0] = BooleanArray(3)
            field("active").set(f.scrolling, listOf(s))
            field("sideways").setBoolean(f.scrolling, true)
            field("dragFrom").setFloat(f.scrolling, at)
            f.scrolling.snapsAround = { _, _, _, projected, then -> asked = projected to then }
        }
        val mode get() = field("mode").getInt(f.scrolling)
        fun snap(v: Float) = Scrolling::class.java.getDeclaredMethod("snap", Float::class.java).apply { isAccessible = true }
            .invoke(f.scrolling, v) as Boolean
        /** The list's areas around the projection, at [positions]. */
        fun answer(vararg positions: Float) {
            val b = { v: Float -> v.toBits() }
            asked!!.second(intArrayOf(positions.size) + positions.flatMap { listOf(b(it), b(it), 0) })
        }
    }

    @Test fun aProximityListFlungPastItsToldAreasAsksForThoseAroundTheProjection() {
        val r = ListRelease(mandatory = false)
        assertFalse("nothing told is near: the fling is the platform's", r.snap(8000f))
        val projected = r.asked!!.first
        assertTrue(projected > 1000f)
        // The fling that release starts is under way when the answer comes: one near its projection takes it.
        r.field("mode").setInt(r.f.scrolling, 4)
        r.answer(projected + 40f)
        val fling = r.field("fling").get(r.f.scrolling) as android.widget.OverScroller
        assertEquals((projected + 40f - r.at).toInt().toFloat(), fling.finalX.toFloat(), 1f)
    }

    @Test fun anAnswerForAListTheAppScrolledSinceIsDropped() {
        val r = ListRelease(mandatory = true)
        assertTrue(r.snap(8000f))
        // The app scrolls the list (a new revision): the settle ends, and the late answer moves nothing.
        r.f.scrolling.told(intArrayOf(1, 1, 1, 0, 0, 0, 0, 100000f.toBits(), 500f.toBits()))
        assertEquals(0, r.mode)
        r.answer(r.asked!!.first)
        assertEquals("idle", 0, r.mode)
    }

    @Test fun aFingerThatCatchesAFlingBeginsItsDragWhereItCaughtIt() {
        val f = Fixture(1)
        f.told(1)
        f.down()
        f.under(f.reply(0))
        val s = f.scroller()
        val field = { name: String -> Scrolling::class.java.getDeclaredField(name).apply { isAccessible = true } }
        // A fling of the last gesture, which began its drag at 0, has carried the scroller to 450.
        field("mode").setInt(f.scrolling, 4)
        field("active").set(f.scrolling, listOf(s))
        field("sideways").setBoolean(f.scrolling, true)
        field("dragFrom").setFloat(f.scrolling, 0f)
        s.nowX = 450f
        val e = MotionEvent.obtain(0, 10, MotionEvent.ACTION_DOWN, 100f, 100f, 0)
        assertTrue("the catch takes the touch", f.scrolling.touch(e)); e.recycle()
        assertEquals("dragging", 3, field("mode").getInt(f.scrolling))
        assertEquals("the next flick counts from where it was caught", 450f, field("dragFrom").getFloat(f.scrolling), 0f)
    }
}
