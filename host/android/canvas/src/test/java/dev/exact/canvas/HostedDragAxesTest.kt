package dev.exact.canvas

import android.app.Activity
import android.content.Context
import android.view.MotionEvent
import android.view.VelocityTracker
import android.view.View
import android.widget.FrameLayout
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.util.ReflectionHelpers
import java.io.File

/**
 * Axis-specific hosted drag claims (`NativeModule.dragAxes` / `ExactExtension.dragAxes`,
 * LLP 1133.008 D8 item 6, expo/exact#1983).
 */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [31],
    manifest = Config.NONE,
    shadows = [HostedTouchTest.Engine::class, HostedTouchTest.Reader::class],
    instrumentedPackages = ["dev.exact.canvas.Native", "dev.exact.canvas.ExactView"],
)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HostedDragAxesTest {
    private class Fixture(
        val accepts: Boolean = true,
        val scroller: Boolean = true,
        val claims: Int = 1 or 128, // horizontal yieldable ancestor pan
        val axis: Int = 2, // vertical scroll
        var dragAxes: Int = NativeModule.DRAG_HORIZONTAL,
        val contextPending: Boolean = false,
        val viewEnabled: Boolean = true,
        val propDisabled: Boolean = false,
        val nestedSheetWords: IntArray = intArrayOf(),
    ) {
        init {
            val pool = ReflectionHelpers.getStaticField<Any>(VelocityTracker::class.java, "sPool")
            while (ReflectionHelpers.callInstanceMethod<VelocityTracker?>(pool, "acquire") != null) {}
        }

        val activity: Activity = Robolectric.buildActivity(Activity::class.java).get()
        val events = ArrayList<Int>()
        val xs = ArrayList<Float>()
        val ys = ArrayList<Float>()
        var scrubbing = false
        var scrubCleared = false
        var hostedView: View? = null

        val module = object : NativeModule {
            override fun create(context: Context, props: JSONObject): View = object : View(context) {
                init { isEnabled = viewEnabled }
                override fun onTouchEvent(e: MotionEvent): Boolean {
                    events.add(e.actionMasked); xs.add(e.x); ys.add(e.y)
                    when (e.actionMasked) {
                        MotionEvent.ACTION_MOVE -> if (kotlin.math.abs(e.x - xs.first()) >= 20f) scrubbing = true
                        MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                            if (scrubbing) scrubCleared = true
                            scrubbing = false
                        }
                    }
                    return accepts
                }
            }.also { hostedView = it }

            override fun update(view: View, props: JSONObject) {}
            override fun dragAxes(view: View): Int = dragAxes
        }

        val natives = Natives(activity, { File(".") }, mapOf("chart-scrubber" to module))
        val view = ExactView(activity, false) {}.also { it.started = true; it.natives = natives }
        val parent = FrameLayout(activity).also { it.addView(natives); it.addView(view) }

        init {
            val E = HostedTouchTest.Engine
            E.samples.clear(); E.touches.clear(); E.scrolls.clear(); E.deltas.clear()
            E.fingers.clear(); E.fingerPoints.clear(); E.takesLongPress = false
            E.asks = 0; E.beforeReply = null; E.deferReplies = false
            E.replies.clear(); E.asksPending.clear(); E.flushing = false
            val disabledJson = if (propDisabled) ",\"disabled\":true" else ""
            natives.ensure(7, 2, 200, 300, "{\"module\":\"chart-scrubber\",\"slot\":2,\"generation\":3$disabledJson}", 0)
            E.reply = intArrayOf(claims, if (scroller) 1 else 0, 0) +
                (if (scroller) intArrayOf(9, axis, 0, 0, 0, 500f.toBits(), 500f.toBits(), 0, 0, 300f.toBits(), 600f.toBits()) else intArrayOf()) +
                nestedSheetWords +
                intArrayOf(
                    7, 10f.toBits(), 20f.toBits(), 200f.toBits(), 300f.toBits(),
                    2, 3, 0, 0, 0, 10f.toBits(), 20f.toBits(),
                    if (contextPending) 1 else 0,
                )
        }

        fun scrolling(): Scrolling =
            ExactView::class.java.getDeclaredField("scrolling").apply { isAccessible = true }.get(view) as Scrolling

        fun touch(action: Int, x: Float = 100f, y: Float = 100f, time: Long = 100L) {
            val e = MotionEvent.obtain(100L, time, action, x, y, 0)
            assertTrue(view.dispatchTouchEvent(e)); e.recycle()
            HostedTouchTest.Engine.flush()
        }

        fun batchedMove(firstX: Float, firstY: Float, firstTime: Long, secondX: Float, secondY: Float, secondTime: Long) {
            val e = MotionEvent.obtain(100L, firstTime, MotionEvent.ACTION_MOVE, firstX, firstY, 0)
            e.addBatch(secondTime, secondX, secondY, 1f, 1f, 0)
            assertTrue(view.dispatchTouchEvent(e)); e.recycle()
            HostedTouchTest.Engine.flush()
        }

        fun fingers(action: Int, ids: IntArray, xs: FloatArray, ys: FloatArray, time: Long) {
            val props = ids.map { id -> MotionEvent.PointerProperties().also { it.id = id; it.toolType = MotionEvent.TOOL_TYPE_FINGER } }.toTypedArray()
            val coords = ids.indices.map { i -> MotionEvent.PointerCoords().also { it.x = xs[i]; it.y = ys[i]; it.pressure = 1f; it.size = 1f } }.toTypedArray()
            val e = MotionEvent.obtain(100L, time, action, ids.size, props, coords, 0, 0, 1f, 1f, 0, 0, 0, 0)
            assertTrue(view.dispatchTouchEvent(e)); e.recycle()
            HostedTouchTest.Engine.flush()
        }
    }

    /** Both horizontal directions, single-MOVE jump crossing both slop and 20 dp as well as stepped moves,
     * early and delayed hit replies, with and without a pending context menu, and UP/CANCEL clearing scrub. */
    @Test fun horizontalDragInBothDirectionsScrubsChartUnderDrawerPan() {
        for (deltaX in listOf(40f, -40f)) for (singleJump in listOf(false, true))
            for (late in listOf(false, true)) for (context in listOf(false, true))
                for (end in listOf(MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL)) {
                    val f = Fixture(contextPending = context)
                    val moves = {
                        if (!singleJump) f.touch(MotionEvent.ACTION_MOVE, x = 100f + deltaX / 2f, y = 100f, time = 110)
                        f.touch(MotionEvent.ACTION_MOVE, x = 100f + deltaX, y = 100f, time = 120)
                        f.touch(end, x = 100f + deltaX, y = 100f, time = 140)
                    }
                    if (late) HostedTouchTest.Engine.beforeReply = moves
                    f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
                    if (!late) moves()
                    val label = "dx=$deltaX single=$singleJump late=$late ctx=$context end=$end"
                    val expectedMoves = if (singleJump) listOf(0, 2, end) else listOf(0, 2, 2, end)
                    assertEquals(label, expectedMoves, f.events)
                    assertEquals(label, 90f + deltaX, f.xs[f.xs.lastIndex - 1], 0.01f)
                    assertTrue(label, f.scrubCleared)
                    assertFalse(label, f.scrubbing)
                    assertEquals(label, listOf(0, 3), HostedTouchTest.Engine.touches)
                    assertTrue(label, HostedTouchTest.Engine.scrolls.isEmpty())
                    f.view.running(false, false)
                }
    }

    /** Mid-scrub diagonal vertical drift after crossing horizontal slop stays with the chart. */
    @Test fun diagonalVerticalDriftAfterHorizontalSlopLatchStaysWithChart() {
        val f = Fixture()
        f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
        f.touch(MotionEvent.ACTION_MOVE, x = 125f, y = 100f, time = 110)
        f.touch(MotionEvent.ACTION_MOVE, x = 135f, y = 160f, time = 120)
        f.touch(MotionEvent.ACTION_UP, x = 135f, y = 160f, time = 140)
        assertEquals(listOf(0, 2, 2, 1), f.events)
        assertEquals(listOf(0, 3), HostedTouchTest.Engine.touches)
        assertTrue(HostedTouchTest.Engine.scrolls.isEmpty())
    }

    /** Vertical movement starting on the chart scrolls the vertical scroller and cancels the chart without scrubbing. */
    @Test fun verticalDragOnHorizontalChartScrollsPageAndCancelsScrub() {
        for (late in listOf(false, true)) for (context in listOf(false, true)) {
            val f = Fixture(contextPending = context)
            val moves = {
                f.touch(MotionEvent.ACTION_MOVE, x = 100f, y = 60f, time = 120)
                f.touch(MotionEvent.ACTION_MOVE, x = 100f, y = 20f, time = 140)
                f.touch(MotionEvent.ACTION_UP, x = 100f, y = 20f, time = 160)
            }
            if (late) HostedTouchTest.Engine.beforeReply = moves
            f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
            if (!late) moves()
            assertEquals("late=$late ctx=$context", listOf(0, 3), f.events)
            assertFalse(f.scrubbing)
            assertEquals(listOf(0, 3), HostedTouchTest.Engine.touches)
            assertFalse("vertical scroller must scroll", HostedTouchTest.Engine.scrolls.isEmpty())
            f.view.running(false, false)
        }
    }

    /** Outside the chart, disabled chart (view or prop), declined DOWN, or undeclared/wrong dragAxes fall through to the drawer. */
    @Test fun outsideDisabledDeclinedAndUndeclaredContactsStillOpenDrawer() {
        // 1. Horizontal drag outside the chart box (chart is [10..210, 20..320]).
        val outside = Fixture()
        outside.touch(MotionEvent.ACTION_DOWN, x = 250f, y = 100f, time = 100)
        outside.touch(MotionEvent.ACTION_MOVE, x = 290f, y = 100f, time = 120)
        outside.touch(MotionEvent.ACTION_UP, x = 290f, y = 100f, time = 140)
        assertTrue(outside.events.isEmpty())
        assertEquals(listOf(0, 2, 1), HostedTouchTest.Engine.touches)

        // 2. Disabled view (`view.isEnabled = false`) and disabled element (`disabled=true`).
        for (prop in listOf(false, true)) {
            val disabled = Fixture(viewEnabled = prop, propDisabled = prop)
            disabled.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
            disabled.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 120)
            disabled.touch(MotionEvent.ACTION_UP, x = 140f, y = 100f, time = 140)
            assertTrue("prop=$prop", disabled.events.isEmpty())
            assertEquals("prop=$prop", listOf(0, 2, 1), HostedTouchTest.Engine.touches)
        }

        // 3. Declined ACTION_DOWN (`onTouchEvent` returns false).
        val declined = Fixture(accepts = false)
        declined.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
        declined.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 120)
        declined.touch(MotionEvent.ACTION_UP, x = 140f, y = 100f, time = 140)
        assertEquals(listOf(0), declined.events)
        assertEquals(listOf(0, 2, 1), HostedTouchTest.Engine.touches)

        // 4. Undeclared (`0`) or vertical-only (`DRAG_VERTICAL`) on horizontal yieldable pan.
        for (axes in listOf(0, NativeModule.DRAG_VERTICAL)) {
            val undeclared = Fixture(dragAxes = axes)
            undeclared.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
            undeclared.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 120)
            undeclared.touch(MotionEvent.ACTION_UP, x = 140f, y = 100f, time = 140)
            assertEquals("axes=$axes", listOf(0, 3), undeclared.events)
            assertEquals("axes=$axes", listOf(0, 2, 1), HostedTouchTest.Engine.touches)
        }
    }

    /** An exclusive Exact horizontal claim (e.g., swipe row or direct self-pan, bit 7 clear) still wins over DRAG_HORIZONTAL. */
    @Test fun exclusiveHorizontalGestureStillBeatsHostedDragAxes() {
        for (context in listOf(false, true)) {
            val f = Fixture(claims = 1, contextPending = context) // bit 0 set, bit 7 clear
            f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
            f.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 120)
            f.touch(MotionEvent.ACTION_UP, x = 140f, y = 100f, time = 140)
            assertEquals(if (context) emptyList() else listOf(0, 3), f.events)
            assertEquals(listOf(0, 2, 1), HostedTouchTest.Engine.touches)
        }
    }

    /** Batched MotionEvents and separate MOVEs latch the same axis at the first sample crossing slop. */
    @Test fun batchedMotionEventMatchesSeparateMovesForEarlyLateAndContextPending() {
        for (batched in listOf(false, true)) for (late in listOf(false, true)) for (context in listOf(false, true)) {
            val f = Fixture(contextPending = context)
            val moves = {
                if (batched) f.batchedMove(125f, 100f, 110, 135f, 160f, 120)
                else {
                    f.touch(MotionEvent.ACTION_MOVE, x = 125f, y = 100f, time = 110)
                    f.touch(MotionEvent.ACTION_MOVE, x = 135f, y = 160f, time = 120)
                }
                f.touch(MotionEvent.ACTION_UP, x = 135f, y = 160f, time = 140)
            }
            if (late) HostedTouchTest.Engine.beforeReply = moves
            f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
            if (!late) moves()
            val label = "batched=$batched late=$late ctx=$context"
            val expected = if (batched) listOf(0, 2, 1) else listOf(0, 2, 2, 1)
            assertEquals(label, expected, f.events)
            assertEquals(label, listOf(0, 3), HostedTouchTest.Engine.touches)
            assertTrue(label, HostedTouchTest.Engine.scrolls.isEmpty())
            f.view.running(false, false)
        }
    }

    /** Catching a shared fling still lets a declared drag finish with UP, while a tap without drag stays suppressed. */
    @Test fun sharedFlingCatchAllowsDeclaredScrubUpWhileSuppressingTap() {
        for (claims in listOf(0, 1 or 128)) {
            // 1. Declared horizontal scrub after catching a shared fling receives DOWN, MOVE, UP (no CANCEL).
            val scrub = Fixture(claims = claims, axis = 2, dragAxes = NativeModule.DRAG_HORIZONTAL)
            val s = scrub.scrolling()
            // Start a shared list fling on scroller 9.
            scrub.touch(MotionEvent.ACTION_DOWN, x = 250f, y = 200f, time = 10)
            scrub.touch(MotionEvent.ACTION_UP, x = 250f, y = 200f, time = 20)
            s.heightReleased(s.serial, intArrayOf(9, 1200f.toBits(), 0, 1f.toBits()))
            assertTrue(s.timerActive)
            scrub.events.clear(); HostedTouchTest.Engine.touches.clear(); HostedTouchTest.Engine.scrolls.clear()
            scrub.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
            assertTrue("claims=$claims", s.caughtSharedFling)
            scrub.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 120)
            scrub.touch(MotionEvent.ACTION_UP, x = 140f, y = 100f, time = 140)
            assertEquals("claims=$claims", listOf(0, 2, 1), scrub.events)
            assertEquals("claims=$claims", listOf(0, 3), HostedTouchTest.Engine.touches)
            scrub.view.running(false, false)

            // 2. Lifting without crossing slop still treats the contact as a catch-only tap (cancels hosted view).
            val tap = Fixture(claims = claims, axis = 2, dragAxes = NativeModule.DRAG_HORIZONTAL)
            val ts = tap.scrolling()
            tap.touch(MotionEvent.ACTION_DOWN, x = 250f, y = 200f, time = 10)
            tap.touch(MotionEvent.ACTION_UP, x = 250f, y = 200f, time = 20)
            ts.heightReleased(ts.serial, intArrayOf(9, 1200f.toBits(), 0, 1f.toBits()))
            tap.events.clear(); HostedTouchTest.Engine.touches.clear()
            tap.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
            assertTrue("tap claims=$claims", ts.caughtSharedFling)
            tap.touch(MotionEvent.ACTION_UP, x = 100f, y = 100f, time = 120)
            assertEquals("tap claims=$claims", listOf(0, 3), tap.events)
            tap.view.running(false, false)
        }
    }

    /** With claims = 0, a horizontal chart inside a same-axis or zoomable scroller keeps horizontal drags,
     * including a sub-slop Euclidean move that later resolves horizontally, while letting vertical pan the scroller. */
    @Test fun horizontalDragAxesKeepsDragFromSameAxisAndZoomableScrollerWithoutPan() {
        for (flags in listOf(1, 3 or Scrolling.ZOOM_FLAG)) {
            val f = Fixture(claims = 0, axis = flags, dragAxes = NativeModule.DRAG_HORIZONTAL)
            f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
            f.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 120)
            f.touch(MotionEvent.ACTION_UP, x = 140f, y = 100f, time = 140)
            assertEquals("flags=$flags", listOf(0, 2, 1), f.events)
            assertTrue("flags=$flags", HostedTouchTest.Engine.deltas.isEmpty())
            f.view.running(false, false)
        }

        // Sub-slop Euclidean move (7, 4): hypot > 8 (default slop) while max <= 8. Resolving horizontal keeps the chart.
        val horiz = Fixture(claims = 0, axis = 3 or Scrolling.ZOOM_FLAG, dragAxes = NativeModule.DRAG_HORIZONTAL)
        horiz.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
        horiz.touch(MotionEvent.ACTION_MOVE, x = 107f, y = 104f, time = 110)
        horiz.touch(MotionEvent.ACTION_MOVE, x = 125f, y = 104f, time = 120)
        horiz.touch(MotionEvent.ACTION_UP, x = 125f, y = 104f, time = 140)
        assertEquals(listOf(0, 2, 2, 1), horiz.events)
        assertTrue(HostedTouchTest.Engine.deltas.isEmpty())
        horiz.view.running(false, false)

        // Same sub-slop start (7, -4) turning vertical upward (7, -30) hands off to the zoomable scroller.
        val vert = Fixture(claims = 0, axis = 3 or Scrolling.ZOOM_FLAG, dragAxes = NativeModule.DRAG_HORIZONTAL)
        vert.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
        vert.touch(MotionEvent.ACTION_MOVE, x = 107f, y = 96f, time = 110)
        vert.touch(MotionEvent.ACTION_MOVE, x = 107f, y = 70f, time = 120)
        vert.touch(MotionEvent.ACTION_MOVE, x = 107f, y = 40f, time = 130)
        vert.touch(MotionEvent.ACTION_UP, x = 107f, y = 40f, time = 140)
        assertEquals(listOf(0, 2, 3), vert.events)
        assertFalse("zoomable scroller must pan vertically", HostedTouchTest.Engine.deltas.isEmpty())
        vert.view.running(false, false)
    }

    /** Vertical dragAxes works over vertical ancestor pan (bit 8), modal sheet (bit 2), and nested-sheet directions
     * (bits 3/4 with and without the 5-word bit-5 header), while horizontal movement still scrolls a horizontal scroller. */
    @Test fun verticalDragAxesKeepsVerticalDragOverAncestorPanAndSheetWithoutBlockingHorizontalScroll() {
        val sheetPayload = intArrayOf(1, 1, 200f.toBits(), 80f.toBits(), 400f.toBits())
        val cases = listOf(
            Triple(2 or 256, -40f, intArrayOf()),
            Triple(6, -40f, intArrayOf()),
            Triple(8, -40f, intArrayOf()),
            Triple(16, 40f, intArrayOf()),
            Triple(8 or 32, -40f, sheetPayload),
            Triple(16 or 32, 40f, sheetPayload),
        )
        for ((claims, dy, payload) in cases) {
            val f = Fixture(claims = claims, axis = 1, dragAxes = NativeModule.DRAG_VERTICAL, nestedSheetWords = payload)
            f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
            f.touch(MotionEvent.ACTION_MOVE, x = 100f, y = 100f + dy, time = 120)
            f.touch(MotionEvent.ACTION_UP, x = 100f, y = 100f + dy, time = 140)
            assertEquals("claims=$claims", listOf(0, 2, 1), f.events)
            assertEquals("claims=$claims", listOf(0, 3), HostedTouchTest.Engine.touches)
            assertTrue("claims=$claims", HostedTouchTest.Engine.deltas.isEmpty())
            f.view.running(false, false)
        }

        // Horizontal movement (dragging left from offset 0) on the same vertical-axis view scrolls the horizontal scroller.
        val horiz = Fixture(claims = 2 or 256, axis = 1, dragAxes = NativeModule.DRAG_VERTICAL)
        horiz.touch(MotionEvent.ACTION_DOWN, x = 180f, y = 100f, time = 100)
        horiz.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 120)
        horiz.touch(MotionEvent.ACTION_MOVE, x = 100f, y = 100f, time = 140)
        horiz.touch(MotionEvent.ACTION_UP, x = 100f, y = 100f, time = 160)
        assertEquals(listOf(0, 3), horiz.events)
        assertTrue("horizontal scroller must scroll", HostedTouchTest.Engine.deltas.any { it.second != 0f })
        horiz.view.running(false, false)
    }

    /** Second pointer, mid-scrub retirement, direct Natives.dragAxes disabled guard, and a fresh subsequent contact. */
    @Test fun secondPointerRetirementAndSubsequentContactStayBalanced() {
        val f = Fixture()
        // Direct Natives.dragAxes guard for view.isEnabled and prop disabled.
        val hosted = f.hostedView!!
        assertEquals(NativeModule.DRAG_HORIZONTAL, f.natives.dragAxes(hosted))
        hosted.isEnabled = false
        assertEquals(0, f.natives.dragAxes(hosted))
        hosted.isEnabled = true
        assertEquals(NativeModule.DRAG_HORIZONTAL, f.natives.dragAxes(hosted))

        // First contact: horizontal scrub + second pointer + release.
        f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 100)
        f.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 105)
        f.fingers(MotionEvent.ACTION_POINTER_DOWN or (1 shl 8), intArrayOf(0, 1), floatArrayOf(140f, 160f), floatArrayOf(100f, 100f), 110)
        f.fingers(MotionEvent.ACTION_POINTER_UP or (1 shl 8), intArrayOf(0, 1), floatArrayOf(140f, 160f), floatArrayOf(100f, 100f), 120)
        f.touch(MotionEvent.ACTION_UP, x = 140f, y = 100f, time = 130)
        assertEquals(listOf(0, 2, 5, 6, 1), f.events)
        assertEquals(listOf(0, 3), HostedTouchTest.Engine.touches)

        // Second contact: retire the hosted view mid-scrub; view gets CANCEL and swallows the rest of the contact.
        f.events.clear(); HostedTouchTest.Engine.touches.clear()
        f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 160)
        f.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 170)
        f.natives.unrowed(emptySet())
        f.touch(MotionEvent.ACTION_MOVE, x = 180f, y = 100f, time = 180)
        f.touch(MotionEvent.ACTION_UP, x = 180f, y = 100f, time = 190)
        assertEquals(listOf(0, 2, 3), f.events)
        assertEquals(listOf(0, 3), HostedTouchTest.Engine.touches)

        // Third contact after remounting with dragAxes = 0: drawer wins cleanly with no stale hostedAxes.
        f.natives.ensure(7, 2, 200, 300, "{\"module\":\"chart-scrubber\",\"slot\":2,\"generation\":3}", 0)
        f.events.clear(); HostedTouchTest.Engine.touches.clear()
        f.dragAxes = 0
        f.touch(MotionEvent.ACTION_DOWN, x = 100f, y = 100f, time = 200)
        f.touch(MotionEvent.ACTION_MOVE, x = 140f, y = 100f, time = 220)
        f.touch(MotionEvent.ACTION_UP, x = 140f, y = 100f, time = 240)
        assertEquals(listOf(0, 3), f.events)
        assertEquals(listOf(0, 2, 1), HostedTouchTest.Engine.touches)
        f.view.running(false, false)
    }

    /** ExactExtension.dragAxes forwards cleanly through PackageView. */
    @Test fun exactExtensionDragAxesForwardsThroughPackageView() {
        val activity = Robolectric.buildActivity(Activity::class.java).get()
        val ext = object : ExactExtension(ExtensionContext(activity.application, "pkg")) {
            override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {}
            override fun view(tag: String, session: String, id: String, context: Context, props: JSONObject, emit: (String, String) -> Unit): View =
                View(context)
            override fun dragAxes(view: View): Int = NativeModule.DRAG_HORIZONTAL
        }
        val made = ReflectionHelpers.getField<MutableMap<String, ExactExtension>>(Extensions, "made")
        val prev = made["pkg"]
        try {
            made["pkg"] = ext
            val pkgViewClass = Class.forName("dev.exact.canvas.Extensions\$PackageView")
            val ctor = pkgViewClass.getDeclaredConstructor(String::class.java, String::class.java)
            ctor.isAccessible = true
            val module = ctor.newInstance("pkg-scrubber", "pkg") as NativeModule
            val v = View(activity)
            assertEquals(NativeModule.DRAG_HORIZONTAL, module.dragAxes(v))
        } finally {
            if (prev == null) made.remove("pkg") else made["pkg"] = prev
        }
    }
}
