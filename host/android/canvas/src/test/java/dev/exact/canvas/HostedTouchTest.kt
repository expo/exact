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
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [HostedTouchTest.Engine::class, HostedTouchTest.Reader::class],
    instrumentedPackages = ["dev.exact.canvas.Native", "dev.exact.canvas.ExactView"])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HostedTouchTest {
    /** Owner reads on its engine queue and posts the answer to main; never answer inside Contacts.touch. */
    @Implements(ExactView::class)
    class Reader : org.robolectric.shadows.ShadowView() {
        @Implementation(methodName = "ask\$canvas")
        fun ask(x: Float, y: Float, then: (IntArray) -> Unit) {
            val main = android.os.Handler(android.os.Looper.getMainLooper())
            Engine.asksPending.addLast {
                val answer = Native.under(x, y)
                val deliver = { then(answer) }
                if (Engine.deferReplies) Engine.replies.addLast(deliver) else Engine.asksPending.addLast(deliver)
            }
            main.post { Engine.flush() }
        }
    }

    @Implements(Native::class)
    class Engine {
        companion object {
            var reply = intArrayOf()
            var beforeReply: (() -> Unit)? = null
            var deferReplies = false
            val replies = ArrayDeque<() -> Unit>()
            val asksPending = ArrayDeque<() -> Unit>()
            var flushing = false
            fun flush() {
                if (flushing) return
                flushing = true
                try { while (asksPending.isNotEmpty()) asksPending.removeFirst().invoke() }
                finally { flushing = false }
            }
            val touches = ArrayList<Int>()
            val scrolls = ArrayList<Float>()
            val deltas = ArrayList<Triple<Int, Float, Float>>()
            val fingers = ArrayList<Pair<Int, Int>>()
            val fingerPoints = ArrayList<Pair<Float, Float>>()
            var takesLongPress = false
            var asks = 0
            @Implementation @JvmStatic fun __staticInitializer__() {}
            @Implementation @JvmStatic fun under(x: Float, y: Float): IntArray {
                asks++
                val answer = reply.copyOf()
                val at = 3 + 11 * answer[1]
                val left = Float.fromBits(answer[at + 1]); val top = Float.fromBits(answer[at + 2])
                if (x < left || y < top || x >= left + Float.fromBits(answer[at + 3]) ||
                    y >= top + Float.fromBits(answer[at + 4])) answer[at] = 0
                beforeReply?.also { beforeReply = null }?.invoke()
                return answer
            }
            @Implementation @JvmStatic fun touch(action: Int, x: Float, y: Float, tool: Int) {
                touches.add(action)
                if (action == 4 && takesLongPress) reply[3 + 11 * reply[1] + 7]++
            }
            @Implementation @JvmStatic fun finger(action: Int, id: Int, x: Float, y: Float) {
                fingers.add(action to id); fingerPoints.add(x to y)
            }
            @Implementation @JvmStatic fun scrolled(id: Int, x: Float, y: Float, rev: Int) {
                scrolls.add(y); deltas.add(Triple(id, x, y))
            }
            @Implementation @JvmStatic fun scrollers(): IntArray? {
                if (deltas.isEmpty()) return null
                return (0 until reply[1]).flatMap { n ->
                    val i = 3 + 11 * n
                    val moved = deltas.filter { it.first == reply[i] }
                    listOf(reply[i], reply[i + 1], reply[i + 2],
                        (Float.fromBits(reply[i + 3]) + moved.sumOf { it.second.toDouble() }.toFloat()).toBits(),
                        (Float.fromBits(reply[i + 4]) + moved.sumOf { it.third.toDouble() }.toFloat()).toBits(),
                        0, 0, reply[i + 5], reply[i + 6])
                }.toIntArray()
            }
            @Implementation @JvmStatic fun scrollDrag(id: Int, ended: Boolean, vx: Float, vy: Float, rest: Boolean, restX: Float, restY: Float, flings: Boolean) {}
            @Implementation @JvmStatic fun flingEnded(id: Int) {}
            @Implementation @JvmStatic fun refresh(id: Int) {}
            @Implementation @JvmStatic fun nextDue() = -1.0
        }
    }

    private class Fixture(
        val accepts: Boolean = true, scroller: Boolean = false, claims: Int = 0, axis: Int = 2,
        longClickable: Boolean = false, clickable: Boolean = false, disallow: Boolean = false, nested: Boolean = false,
    ) {
        init {
            // ShadowVelocityTracker.clear leaves coordinates in its history, and hasPointer still reads them.
            // Discard pooled shadows so each fixture's event clock has an independent velocity history.
            val pool = ReflectionHelpers.getStaticField<Any>(
                VelocityTracker::class.java, "sPool")
            while (ReflectionHelpers.callInstanceMethod<VelocityTracker?>(pool, "acquire") != null) {}
        }
        val slop get() = android.view.ViewConfiguration.get(activity).scaledTouchSlop
        val controller = Robolectric.buildActivity(Activity::class.java)
        val activity = controller.get()
        val events = ArrayList<Int>()
        val points = ArrayList<Pair<Float, Float>>()
        val pointers = ArrayList<List<Int>>()
        val pointerPoints = ArrayList<List<Pair<Float, Float>>>()
        val times = ArrayList<Long>()
        var longClicks = 0
        var clicks = 0
        val native = object : NativeModule {
            override fun create(context: Context, props: JSONObject): View = object : View(context) {
                init {
                    if (longClickable) setOnLongClickListener { longClicks++; true }
                    if (clickable) setOnClickListener { clicks++ }
                }
                override fun onTouchEvent(e: MotionEvent): Boolean {
                    if (disallow && e.actionMasked == MotionEvent.ACTION_DOWN) parent.requestDisallowInterceptTouchEvent(true)
                    events.add(e.actionMasked); points.add(e.x to e.y)
                    pointers.add((0 until e.pointerCount).map { e.getPointerId(it) })
                    pointerPoints.add((0 until e.pointerCount).map { e.getX(it) to e.getY(it) })
                    times.add(e.eventTime)
                    return if (longClickable || clickable) super.onTouchEvent(e) else accepts
                }
            }.let { child ->
                if (nested) FrameLayout(context).apply { addView(child, FrameLayout.LayoutParams(-1, -1)) } else child
            }
            override fun update(view: View, props: JSONObject) {}
        }
        val natives = Natives(activity, File("."), mapOf("test-map" to native))
        val view = ExactView(activity, false) {}.also { it.started = true; it.natives = natives }
        val parent = FrameLayout(activity).also { it.addView(natives); it.addView(view) }
        init {
            Engine.touches.clear(); Engine.scrolls.clear(); Engine.deltas.clear(); Engine.fingers.clear(); Engine.takesLongPress = false; Engine.asks = 0; Engine.beforeReply = null
            Engine.fingerPoints.clear(); Engine.deferReplies = false; Engine.replies.clear(); Engine.asksPending.clear(); Engine.flushing = false
            natives.ensure(7, 2, 200, 300, "{\"module\":\"test-map\",\"slot\":2,\"generation\":3}", 0)
            Engine.reply = intArrayOf(claims, if (scroller) 1 else 0, 0) +
                (if (scroller) intArrayOf(9, axis, 0, 0, 0, 500f.toBits(), 500f.toBits(), 0, 0, 300f.toBits(), 600f.toBits()) else intArrayOf()) +
                intArrayOf(7, 10f.toBits(), 20f.toBits(), 200f.toBits(), 300f.toBits(), 2, 3, 0, 10f.toBits(), 20f.toBits(), 0)
        }
        fun fingers(action: Int, ids: IntArray, ys: FloatArray, time: Long, xs: FloatArray = FloatArray(ids.size) { 100f }) {
            val properties = ids.map { id -> MotionEvent.PointerProperties().also { it.id = id; it.toolType = MotionEvent.TOOL_TYPE_FINGER } }.toTypedArray()
            val coords = ys.mapIndexed { i, y -> MotionEvent.PointerCoords().also { it.x = xs[i]; it.y = y; it.pressure = 1f; it.size = 1f } }.toTypedArray()
            val e = MotionEvent.obtain(100, time, action, ids.size, properties, coords, 0, 0, 1f, 1f, 0, 0, 0, 0)
            assertTrue(view.dispatchTouchEvent(e)); e.recycle()
            flush()
        }
        fun touch(action: Int, x: Float = 100f, y: Float = 100f, time: Long = 100L) {
            val e = MotionEvent.obtain(100L, time, action, x, y, 0)
            assertTrue(view.dispatchTouchEvent(e)); e.recycle()
            flush()
        }
        fun flush() = Engine.flush()
        fun started() { view.ownerStarted(true); flush() }
        fun answer() { Engine.replies.removeFirst().invoke(); flush() }
    }

    private fun field(f: Fixture, multiline: Boolean = false): Fields {
        val fields = Fields(f.activity).also { it.adopted(f.view); f.view.fields = it; f.parent.addView(it) }
        val value = (if (multiline) (1..40).joinToString("\n") { "line $it" } else "").toByteArray()
        val words = IntArray((value.size + 3) / 4)
        value.forEachIndexed { i, b -> words[i / 4] = words[i / 4] or ((b.toInt() and 255) shl (8 * (i % 4))) }
        val look = IntArray(18).also { it[0] = if (multiline) 1 else 0; it[4] = -1; it[11] = 16f.toBits() }
        fields.told(intArrayOf(1, 8, 1, 1, 0, 220f.toBits(), 20f.toBits(), 200f.toBits(), 200f.toBits(),
            0, 0, (-1f).toBits(), 0, 0, value.size) + words + look + IntArray(5))
        Engine.reply[2] = 8
        return fields
    }

    private fun assertNoScrollContact(f: Fixture) {
        val scrolling = ReflectionHelpers.getField<Scrolling>(f.view, "scrolling")
        for (name in listOf("early", "owing")) {
            assertTrue("Scrolling.$name must release abandoned events",
                ReflectionHelpers.getField<Collection<*>>(scrolling, name).isEmpty())
        }
        for (name in listOf("down", "tracker")) {
            assertNull("Scrolling.$name must release the abandoned contact",
                ReflectionHelpers.getField<Any?>(scrolling, name))
        }
    }

    @Test fun verticalDragScrollsThePageWithoutScrollingMultilineText() {
        val f = Fixture(scroller = true)
        val fields = field(f, multiline = true)
        val text = fields.text(8)!!
        // Use Android's movement method and editable text, not a consuming touch listener.
        f.controller.setup()
        f.activity.setContentView(f.parent)
        f.parent.layout(0, 0, 600, 800)
        assertTrue(text.requestFocus())
        text.measure(View.MeasureSpec.makeMeasureSpec(200, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(200, View.MeasureSpec.EXACTLY))
        text.layout(0, 0, 200, 200)
        assertTrue(text.length() > 100)
        assertTrue(text.layout.height > text.height)
        val before = text.scrollY
        f.touch(0, x = 250f, y = 180f)
        f.touch(2, x = 250f, y = 130f, time = 120)
        f.touch(2, x = 250f, y = 80f, time = 140)
        f.touch(1, x = 250f, y = 80f, time = 160)
        assertEquals("the field must not apply the intercepted move", before, text.scrollY)
        assertEquals(listOf(50f - f.slop, 50f), Engine.scrolls)
        f.view.running(false, false)
        f.controller.pause().stop().destroy()
    }

    @Test fun disallowOnTheCrossingMoveIsTooLateToKeepAFieldDrag() {
        val f = Fixture(scroller = true)
        val fields = field(f)
        val heard = ArrayList<Int>()
        fields.text(8)!!.setOnTouchListener { _, e ->
            heard.add(e.actionMasked)
            if (e.actionMasked == 2) fields.requestDisallowInterceptTouchEvent(true)
            true
        }
        f.touch(0, x = 250f)
        f.touch(2, x = 250f, y = 60f, time = 120)
        f.touch(2, x = 250f, y = 20f, time = 140)
        f.touch(1, x = 250f, y = 20f, time = 160)
        assertEquals(listOf(0, 3), heard)
        assertEquals(listOf(40f - f.slop, 40f), Engine.scrolls)
        f.view.running(false, false)
    }

    @Test fun aPinchWithBothFingersMovingStaysWithTheMap() {
        for (late in listOf(false, true)) {
            val f = Fixture(scroller = true)
            val pinch = {
                f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(100f, 120f), 110)
                f.fingers(2, intArrayOf(0, 1), floatArrayOf(60f, 160f), 130)
                f.fingers(2, intArrayOf(0, 1), floatArrayOf(20f, 200f), 150)
                f.fingers(6 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(20f, 200f), 170)
                f.touch(2, y = 0f, time = 190)
                f.touch(1, y = 0f, time = 210)
            }
            if (late) Engine.beforeReply = pinch
            f.touch(0)
            if (!late) pinch()
            assertEquals(listOf(0, 5, 2, 2, 6, 2, 1), f.events)
            assertTrue(Engine.scrolls.isEmpty())
        }
    }

    @Test fun aSecondPointerDoesNotTakeAnEstablishedScroll() {
        val f = Fixture(scroller = true)
        f.touch(0, x = 250f)
        f.touch(2, x = 250f, y = 60f, time = 110)
        f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(60f, 100f), 130)
        f.fingers(2, intArrayOf(0, 1), floatArrayOf(20f, 60f), 150)
        f.fingers(6 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(20f, 60f), 170)
        f.touch(2, y = 0f, time = 190)
        f.touch(1, y = 0f, time = 210)
        assertTrue(f.events.isEmpty())
        assertEquals(listOf(40f - f.slop, 40f, 20f), Engine.scrolls)
        f.view.running(false, false)
    }

    @Test fun anOverflowReleasesAHeldFieldToExactAndIgnoresTheLateReply() {
        val f = Fixture(scroller = true)
        val fields = field(f)
        val heard = ArrayList<Int>()
        fields.text(8)!!.setOnTouchListener { _, e -> heard.add(e.actionMasked); true }
        Engine.beforeReply = {
            repeat(241) { f.touch(2, x = 250f, y = 100f - it, time = 110L + it) }
            assertEquals(listOf(0) + List(241) { 2 }, Engine.touches)
        }
        f.touch(0, x = 250f)
        f.touch(1, x = 250f, y = -140f, time = 400)
        assertEquals(listOf(0) + List(241) { 2 } + listOf(1), Engine.touches)
        assertTrue(heard.isEmpty())
        val held = Fields::class.java.getDeclaredField("held").also { it.isAccessible = true }.get(fields) as Collection<*>
        assertTrue("no field events remain retained", held.isEmpty())
        val contacts = ExactView::class.java.getDeclaredField("contacts").also { it.isAccessible = true }.get(f.view)
        val pending = Contacts::class.java.getDeclaredField("pending").also { it.isAccessible = true }.get(contacts) as Collection<*>
        assertTrue("no contact events remain retained", pending.isEmpty())
        assertNoScrollContact(f)
    }

    @Test fun aMovingAllocationUsesItsCurrentPaintedOrigin() {
        val f = Fixture()
        val player = ReflectionHelpers.getField<Player>(f.view, "main")
        val canvas = object : android.graphics.Canvas() {
            override fun drawRenderNode(node: android.graphics.RenderNode) {}
        }
        fun paint(y: Float) {
            val ops = intArrayOf(28, 7, 2, 10f.toBits(), y.toBits(), 200f.toBits(), 300f.toBits()) + IntArray(9)
            player.replay(canvas, java.nio.IntBuffer.wrap(ops), 0, ops.size)
        }
        paint(20f)
        f.touch(0, x = 50f, y = 50f)
        paint(120f)
        f.touch(2, x = 50f, y = 150f, time = 120)
        f.touch(1, x = 50f, y = 150f, time = 140)
        assertEquals(listOf(40f to 30f, 40f to 30f, 40f to 30f), f.points)
    }

    @Test fun aContactFollowsItsRetainedRowAndAnimatingLayer() {
        val f = Fixture()
        val player = ReflectionHelpers.getField<Player>(f.view, "main")
        val layer = ReflectionHelpers.getField<Layers>(f.view, "layers").node(11)
        layer.setPosition(5, 6, 205, 306)
        val row = android.graphics.RenderNode("hosted row")
        row.setPosition(0, 0, 300, 600)
        ReflectionHelpers.getField<MutableMap<Int, android.graphics.RenderNode>>(f.view, "rows")[22] = row
        val canvas = object : android.graphics.Canvas() {
            override fun drawRenderNode(node: android.graphics.RenderNode) {}
        }
        val native = intArrayOf(1, 1f.toBits(), 0, 0, 1f.toBits(), 0, 0) +
            intArrayOf(28, 7, 2, 10f.toBits(), 20f.toBits(), 200f.toBits(), 300f.toBits()) + IntArray(9)
        player.replayAt(canvas, java.nio.IntBuffer.wrap(native), 0, native.size, 5f, 6f, layer)
        player.replay(canvas, java.nio.IntBuffer.wrap(intArrayOf(29, 11)), 0, 2, into = row)
        fun paint(x: Float, y: Float) {
            val ops = intArrayOf(20, 9, 17, 22, x.toBits(), y.toBits(), 21)
            player.replay(canvas, java.nio.IntBuffer.wrap(ops), 0, ops.size)
        }
        paint(0f, 0f)
        f.touch(0, x = 50f, y = 50f)
        paint(30f, 40f)
        val group = ReflectionHelpers.getField<Map<Int, android.graphics.RenderNode>>(f.view, "groups").getValue(9)
        group.translationX = 5f; group.translationY = 15f
        layer.translationX = 10f; layer.translationY = 100f
        f.touch(2, x = 95f, y = 205f, time = 120)
        f.touch(3, x = 95f, y = 205f, time = 140)
        assertEquals(listOf(40f to 30f, 40f to 30f, 40f to 30f), f.points)
    }

    private fun transformedContact(rotation: Boolean) {
        val f = Fixture()
        val player = ReflectionHelpers.getField<Player>(f.view, "main")
        val layer = ReflectionHelpers.getField<Layers>(f.view, "layers").node(11)
        layer.setPosition(5, 6, 205, 306)
        layer.pivotX = 0f; layer.pivotY = 0f
        if (rotation) { layer.rotationZ = 90f; layer.translationX = 100f }
        else { layer.scaleX = 2f; layer.scaleY = 3f }
        val row = android.graphics.RenderNode("transformed row")
        row.setPosition(0, 0, 300, 600)
        row.pivotX = 0f; row.pivotY = 0f
        row.scaleX = 2f; row.scaleY = 2f
        ReflectionHelpers.getField<MutableMap<Int, android.graphics.RenderNode>>(f.view, "rows")[22] = row
        val canvas = object : android.graphics.Canvas() {
            override fun drawRenderNode(node: android.graphics.RenderNode) {}
        }
        val native = intArrayOf(1, 1f.toBits(), 0, 0, 1f.toBits(), 0, 0) +
            intArrayOf(28, 7, 2, 10f.toBits(), 20f.toBits(), 200f.toBits(), 300f.toBits()) + IntArray(9)
        player.replayAt(canvas, java.nio.IntBuffer.wrap(native), 0, native.size, 5f, 6f, layer)
        player.replay(canvas, java.nio.IntBuffer.wrap(intArrayOf(29, 11)), 0, 2, into = row)
        val rows = intArrayOf(17, 22, 7f.toBits(), 8f.toBits())
        player.replay(canvas, java.nio.IntBuffer.wrap(rows), 0, rows.size)
        // Content is at (5,14) within the layer, which sits at (5,6) within the doubled row.
        fun screen(x: Float, y: Float): Pair<Float, Float> = if (rotation)
            (7f + 2f * (105f - (14f + y))) to (8f + 2f * (6f + 5f + x))
        else (7f + 2f * (5f + 2f * (5f + x))) to (8f + 2f * (6f + 3f * (14f + y)))
        val a = screen(10f, 15f); val b = screen(20f, 25f)
        f.touch(0, x = a.first, y = a.second)
        f.fingers(5 or (1 shl 8), intArrayOf(0, 8), floatArrayOf(a.second, b.second), 120,
            floatArrayOf(a.first, b.first))
        layer.translationY += 30f
        f.fingers(2, intArrayOf(0, 8), floatArrayOf(a.second + 60f, b.second + 60f), 140,
            floatArrayOf(a.first, b.first))
        f.fingers(3, intArrayOf(0, 8), floatArrayOf(a.second + 60f, b.second + 60f), 160,
            floatArrayOf(a.first, b.first))
        assertEquals(listOf(0, 5, 2, 3), f.events)
        for (points in f.pointerPoints) for ((i, point) in points.withIndex()) {
            assertEquals(if (i == 0) 10f else 20f, point.first, 0.001f)
            assertEquals(if (i == 0) 15f else 25f, point.second, 0.001f)
        }
    }

    @Test fun scaledLayersTransformEveryPointerIntoTheHostedView() = transformedContact(false)
    @Test fun rotatedLayersTransformEveryPointerIntoTheHostedView() = transformedContact(true)

    @Test fun aHostedViewReceivesAPanInItsOwnCoordinates() {
        val f = Fixture()
        f.touch(MotionEvent.ACTION_DOWN)
        f.touch(MotionEvent.ACTION_MOVE, y = 50f, time = 120)
        f.touch(MotionEvent.ACTION_UP, y = 50f, time = 140)
        assertEquals(listOf(0, 2, 1), f.events)
        assertEquals(90f to 80f, f.points.first())
        assertEquals(listOf(0, 3), Engine.touches)
    }
    @Test fun aTapAndACancelReachTheViewOnce() {
        for (end in listOf(MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL)) {
            val f = Fixture()
            f.touch(0); f.touch(end, time = 120)
            assertEquals(listOf(0, end), f.events)
            assertEquals(listOf(0, 3), Engine.touches)
        }
    }

    @Test fun aViewDecliningDownHearsNothingElse() {
        val f = Fixture(accepts = false)
        f.touch(0); f.touch(2, y = 50f, time = 120); f.touch(1, y = 50f, time = 140)
        assertEquals(listOf(0), f.events)
        assertEquals(listOf(0, 2, 1), Engine.touches)
    }

    @Test fun verticalDragStaysWithTheMapWithEarlyOrLateHitReplies() {
        for (late in listOf(false, true)) {
            val f = Fixture(scroller = true, disallow = true)
            val moves = {
                f.touch(2, y = 70f, time = 120)
                f.touch(2, y = 20f, time = 140)
                f.touch(2, y = 40f, time = 160)
                f.touch(1, y = 0f, time = 180)
            }
            if (late) Engine.beforeReply = moves
            f.touch(0)
            if (!late) moves()
            assertEquals(listOf(0, 2, 2, 2, 1), f.events)
            assertTrue("a drag starting inside the map cannot scroll the page", Engine.scrolls.isEmpty())
            assertEquals(listOf(0, 3), Engine.touches)
            f.view.running(false, false)
        }
    }

    @Test fun aViewDisallowingInterceptionKeepsEitherAxisForTheWholeSequence() {
        for (axis in listOf(1, 2)) for (late in listOf(false, true)) for (multi in listOf(false, true)) {
            val f = Fixture(scroller = true, axis = axis, disallow = true)
            val moves = {
                // Cross scroll slop before adding a finger, then leave the hosted bounds.
                f.touch(2, x = if (axis == 1) 60f else 100f, y = if (axis == 2) 60f else 100f, time = 120)
                if (multi) f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(60f, 100f), 130)
                val ids = if (multi) intArrayOf(0, 1) else intArrayOf(0)
                f.fingers(2, ids, FloatArray(ids.size) { if (axis == 2) -50f else 100f }, 150,
                    FloatArray(ids.size) { if (axis == 1) -50f else 100f })
                if (multi) f.fingers(6 or (1 shl 8), ids, floatArrayOf(-50f, 100f), 170)
                f.touch(1, x = -50f, y = -50f, time = 190)
            }
            if (late) Engine.beforeReply = moves
            f.touch(0)
            if (!late) moves()
            assertEquals(if (multi) listOf(0, 2, 5, 2, 6, 1) else listOf(0, 2, 2, 1), f.events)
            assertTrue("axis=$axis late=$late multi=$multi", Engine.deltas.isEmpty())
            assertEquals(listOf(0, 3), Engine.touches)
            f.view.running(false, false)
        }
    }

    @Test fun aClickableHostedViewIsCancelledWhenThePageIntercepts() {
        for (late in listOf(false, true)) {
            val f = Fixture(scroller = true, clickable = true)
            val moves = {
                f.touch(2, y = 60f, time = 120)
                f.touch(2, y = 20f, time = 140)
                f.touch(1, y = 20f, time = 160)
            }
            if (late) Engine.beforeReply = moves
            f.touch(0)
            if (!late) moves()
            assertEquals(listOf(0, 3), f.events)
            assertEquals(listOf(40f - f.slop, 40f), Engine.scrolls)
            assertEquals(listOf(0, 3), Engine.touches)
            f.view.running(false, false)
        }
    }

    @Test fun aDescendantsInterceptionRequestCanBeRevokedAndEndsWithTheSequence() {
        for (end in listOf(1, 3)) for (revoke in listOf(false, true)) {
            val f = Fixture(scroller = true, nested = true)
            val group = f.natives.taker(7, 2, 3)!! as FrameLayout
            val child = group.getChildAt(0)
            child.setOnTouchListener { v, e ->
                if (e.actionMasked == 0) v.parent.requestDisallowInterceptTouchEvent(true)
                false
            }
            f.touch(0)
            f.touch(2, y = 95f, time = 110)
            if (revoke) child.parent.requestDisallowInterceptTouchEvent(false)
            f.touch(2, y = 60f, time = 120)
            f.touch(2, y = 20f, time = 140)
            f.touch(end, y = 20f, time = 160)
            assertEquals(if (revoke) listOf(0, 2, 3) else listOf(0, 2, 2, 2, end), f.events)
            assertEquals(if (revoke) listOf(40f - f.slop, 40f) else emptyList<Float>(), Engine.scrolls)
            assertFalse("up and cancel clear the parent's request", f.natives.keeps)
            child.setOnTouchListener(null)
            f.events.clear(); Engine.scrolls.clear()
            f.touch(0, time = 200)
            f.touch(2, y = 60f, time = 220)
            f.touch(2, y = 20f, time = 240)
            f.touch(3, time = 260)
            assertEquals(listOf(0, 3), f.events)
            assertEquals(listOf(40f - f.slop, 40f), Engine.scrolls)
            f.view.running(false, false)
        }
    }

    @Test fun aSecondPointerCannotReturnAnInterceptedScrollToTheHostedView() {
        val f = Fixture(scroller = true, clickable = true)
        f.touch(0)
        f.touch(2, y = 60f, time = 120)
        f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(60f, 100f), 140)
        f.fingers(2, intArrayOf(0, 1), floatArrayOf(20f, 60f), 160)
        f.fingers(3, intArrayOf(0, 1), floatArrayOf(20f, 60f), 180)
        assertEquals(listOf(0, 3), f.events)
        assertEquals(listOf(40f - f.slop, 40f), Engine.scrolls)
        f.view.running(false, false)
    }

    @Test fun aDragOnAnAxisThePageCannotScrollStaysWithTheView() {
        for (axis in listOf(1, 2)) {
            val f = Fixture(scroller = true, axis = axis, clickable = true)
            f.touch(0)
            f.touch(2, x = if (axis == 2) 60f else 100f, y = if (axis == 1) 60f else 100f, time = 120)
            f.touch(1, time = 140)
            assertEquals(listOf(0, 2, 1), f.events)
            assertTrue(Engine.deltas.isEmpty())
            f.view.running(false, false)
        }
    }

    @Test fun aDragStartingOutsideTheHostedRegionStillScrollsOnEitherAxis() {
        for (axis in listOf(1, 2)) for (late in listOf(false, true)) {
            val f = Fixture(scroller = true, axis = axis)
            val moves = {
                f.touch(2, x = if (axis == 1) 200f else 250f, y = if (axis == 2) 60f else 100f, time = 120)
                f.touch(2, x = if (axis == 1) 150f else 250f, y = if (axis == 2) 20f else 100f, time = 140)
                f.touch(3, time = 160)
            }
            if (late) Engine.beforeReply = moves
            f.touch(0, x = 250f)
            if (!late) moves()
            assertEquals(listOf(
                Triple(9, if (axis == 1) 50f - f.slop else 0f, if (axis == 2) 40f - f.slop else 0f),
                Triple(9, if (axis == 1) 50f else 0f, if (axis == 2) 40f else 0f)), Engine.deltas)
            assertTrue(f.events.isEmpty())
            f.view.running(false, false)
        }
    }

    @Test fun decliningAHostedDownLeavesTheDragWithTheScroller() {
        val f = Fixture(accepts = false, scroller = true)
        f.touch(0); f.touch(2, y = 60f, time = 120); f.touch(2, y = 20f, time = 140); f.touch(1, time = 160)
        assertEquals(listOf(0), f.events)
        assertEquals(listOf(0, 3), Engine.touches)
        assertEquals(listOf(Triple(9, 0f, 40f - f.slop), Triple(9, 0f, 40f)), Engine.deltas)
        assertEquals(0f, f.natives.taker(7, 2, 3)!!.translationX)
        f.view.running(false, false)
    }

    @Test fun nestedScrollersOutsideHostedViewsHandOffTheRemainderOnEitherAxis() {
        for (axis in listOf(1, 2)) {
            val f = Fixture(scroller = true, axis = axis)
            val old = Engine.reply
            Engine.reply = intArrayOf(0, 2, 0) + intArrayOf(8, axis, 0, 0, 0,
                10f.toBits(), 10f.toBits(), 0, 0, 300f.toBits(), 600f.toBits()) + old.drop(3).toIntArray()
            f.touch(0, x = 250f)
            f.touch(2, x = if (axis == 1) 210f else 250f, y = if (axis == 2) 60f else 100f, time = 120)
            f.touch(2, x = if (axis == 1) 170f else 250f, y = if (axis == 2) 20f else 100f, time = 140)
            f.touch(3, time = 160)
            assertEquals(listOf(Triple(8, if (axis == 1) 10f else 0f, if (axis == 2) 10f else 0f),
                Triple(9, if (axis == 1) 30f - f.slop else 0f, if (axis == 2) 30f - f.slop else 0f),
                Triple(9, if (axis == 1) 40f else 0f, if (axis == 2) 40f else 0f)), Engine.deltas)
            assertTrue(f.events.isEmpty())
            f.view.running(false, false)
        }
    }

    @Test fun aCanvasContactStillRelaysEveryFingerAndKeepsExactScrollArbitration() {
        for (scroll in listOf(false, true)) {
            val f = Fixture(scroller = scroll)
            // A GPU canvas has no hosted module allocation in Native.under's reply.
            Engine.reply[3 + 11 * Engine.reply[1]] = 0
            f.touch(0)
            f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(100f, 120f), 110)
            f.fingers(2, intArrayOf(0, 1), floatArrayOf(60f, 160f), 130)
            f.fingers(2, intArrayOf(0, 1), floatArrayOf(20f, 200f), 150)
            f.fingers(6, intArrayOf(0, 1), floatArrayOf(20f, 200f), 170)
            f.fingers(2, intArrayOf(1), floatArrayOf(220f), 190)
            f.fingers(1, intArrayOf(1), floatArrayOf(220f), 210)
            assertEquals(if (scroll) listOf(0, 3) else listOf(0, 2, 2, 1), Engine.touches)
            assertEquals(if (scroll) listOf(0 to 1) else listOf(0 to 1, 2 to 1, 2 to 1, 2 to 1, 1 to 1), Engine.fingers)
            assertEquals(if (scroll) listOf(40f - f.slop, 40f, -20f) else emptyList<Float>(), Engine.scrolls)
            assertTrue(f.events.isEmpty())
            f.view.running(false, false)
        }
    }

    @Test fun catchingAFlingChecksTheNewDownForAHostedView() {
        for (inside in listOf(false, true)) for (late in listOf(false, true)) for (accepts in listOf(false, true)) for (end in listOf(1, 3)) {
            // Model a recycled tracker left by a previous contact with later event times.
            val previous = VelocityTracker.obtain()
            repeat(20) { i ->
                val event = MotionEvent.obtain(100, 300L + i, if (i == 0) 0 else 2, 250f, 100f, 0)
                previous.addMovement(event)
                event.recycle()
            }
            previous.recycle()
            val f = Fixture(scroller = true, accepts = accepts, disallow = accepts)
            f.touch(0, x = 250f, y = 200f)
            f.touch(2, x = 250f, y = 150f, time = 110)
            f.touch(2, x = 250f, y = 100f, time = 120)
            f.touch(1, x = 250f, y = 50f, time = 130)
            val scrolling = ExactView::class.java.getDeclaredField("scrolling").also { it.isAccessible = true }.get(f.view)
            val mode = Scrolling::class.java.getDeclaredField("mode").also { it.isAccessible = true }
            assertEquals("the preceding drag must have started a fling", 4, mode.getInt(scrolling))
            Engine.deltas.clear(); Engine.scrolls.clear(); Engine.touches.clear()
            val x = if (inside) 100f else 250f
            val moves = {
                f.touch(2, x = x, y = 60f, time = 160)
                f.touch(2, x = x, y = 20f, time = 180)
                f.touch(end, x = x, y = 20f, time = 200)
            }
            if (late) Engine.beforeReply = moves
            f.touch(0, x = x, time = 140)
            if (!late) moves()
            assertEquals(if (inside && accepts) listOf(0, 2, 2, end) else if (inside) listOf(0) else emptyList<Int>(), f.events)
            assertEquals(if (inside && accepts) emptyList<Float>() else listOf(40f, 40f), Engine.scrolls)
            assertEquals("the provisional Exact down must be balanced before teardown", if (inside) listOf(0, 3) else emptyList<Int>(), Engine.touches)
            f.view.running(false, false)
        }
    }

    @Test fun horizontalPanAndSecondPointerStayWithTheMapInAVerticalScroller() {
        for (late in listOf(false, true)) {
            val f = Fixture(scroller = true)
            val moves = {
                f.touch(2, x = 150f, time = 120)
                f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(100f, 120f), 140)
                f.fingers(2, intArrayOf(0, 1), floatArrayOf(100f, 140f), 160)
                f.fingers(6, intArrayOf(0, 1), floatArrayOf(100f, 140f), 180)
                f.fingers(2, intArrayOf(1), floatArrayOf(150f), 200)
                f.fingers(1, intArrayOf(1), floatArrayOf(150f), 220)
            }
            if (late) Engine.beforeReply = moves
            f.touch(0)
            if (!late) moves()
            assertEquals(listOf(0, 2, 5, 2, 6, 2, 1), f.events)
            assertEquals(listOf(0, 1), f.pointers[3])
            assertEquals(listOf(1), f.pointers[5])
            assertTrue(Engine.scrolls.isEmpty())
        }
    }

    @Test fun aClaimedExactAxisKeepsTheGestureAndAnUnclaimedAxisReachesTheMap() {
        for (axis in listOf(1, 2)) {
            val f = Fixture(scroller = true, claims = axis, axis = 1, disallow = true)
            f.touch(0); f.touch(2, x = 150f, time = 120); f.touch(1, x = 150f, time = 140)
            assertEquals(if (axis == 1) listOf(0, 3) else listOf(0, 2, 1), f.events)
            assertEquals(if (axis == 1) listOf(0, 2, 1) else listOf(0, 3), Engine.touches)
            assertTrue(Engine.deltas.isEmpty())
        }
    }

    @Test fun aStopDiscardsPendingContactsAndCancelsAnAcceptedOne() {
        for (late in listOf(false, true)) {
            val f = Fixture(scroller = true)
            val stop = {
                f.touch(2, y = 70f, time = 120)
                f.touch(1, y = 70f, time = 140)
                f.view.running(false, false)
            }
            if (late) Engine.beforeReply = stop
            f.touch(0)
            if (!late) f.view.running(false, false)
            assertEquals(if (late) emptyList<Int>() else listOf(0, 3), f.events)
            f.touch(2, y = 0f, time = 160)
            assertTrue(Engine.scrolls.isEmpty())
        }
    }

    @Test fun aRetiredAllocationCannotReceiveOrScrollTheRestOfItsContact() {
        val f = Fixture(scroller = true)
        f.touch(0)
        f.natives.unrowed(emptySet())
        f.touch(2, y = 60f, time = 120)
        f.touch(2, y = 20f, time = 140)
        f.touch(1, y = 20f, time = 160)
        assertEquals(listOf(0, 3), f.events)
        assertEquals(listOf(0, 3), Engine.touches)
        assertTrue("retirement must swallow the rest of the drag", Engine.scrolls.isEmpty())
        f.touch(0, time = 180)
        f.touch(2, y = 60f, time = 200)
        f.touch(2, y = 20f, time = 220)
        f.touch(1, y = 20f, time = 240)
        assertEquals("a new contact may scroll", listOf(40f - f.slop, 40f), Engine.scrolls)
        f.view.running(false, false)
    }

    @Test fun twoCompletedTapsBehindOneReplyRemainWholeAndOrdered() {
        val f = Fixture()
        Engine.beforeReply = {
            f.touch(1, time = 120)
            f.touch(0, time = 160)
            f.touch(1, time = 180)
        }
        f.touch(0)
        assertEquals(listOf(0, 1, 0, 1), f.events)
        assertEquals(listOf(0, 3, 0, 3), Engine.touches)
    }

    @Test fun aFieldBesideTheMapKeepsItsSelectionGestureAfterALateReply() {
        val f = Fixture(scroller = true)
        val fields = Fields(f.activity).also { it.adopted(f.view); f.view.fields = it; f.parent.addView(it) }
        // Field 8 occupies the neighbouring box. Empty value, default look, five empty strings.
        val row = intArrayOf(1, 8, 1, 1, 0, 220f.toBits(), 20f.toBits(), 200f.toBits(), 200f.toBits(),
            0, 0, (-1f).toBits(), 0, 0, 0) + IntArray(18) + IntArray(5)
        fields.told(row)
        val heard = ArrayList<Int>()
        fields.text(8)!!.setOnTouchListener { _, e ->
            heard.add(e.actionMasked)
            if (e.actionMasked == 0) fields.requestDisallowInterceptTouchEvent(true)
            true
        }
        Engine.reply[2] = 8
        Engine.beforeReply = {
            f.touch(2, x = 250f, y = 50f, time = 120)
            f.touch(1, x = 250f, y = 50f, time = 140)
        }
        f.touch(0, x = 250f)
        assertEquals(listOf(0, 2, 1), heard)
        assertTrue(f.events.isEmpty())
        assertTrue(Engine.scrolls.isEmpty())
        assertTrue(Engine.touches.isEmpty())
    }

    @Test fun anExactContextMenuCancelsTheNativeTapButAnUntakenLongPressDoesNot() {
        for (takes in listOf(false, true)) {
            val f = Fixture()
            Engine.takesLongPress = takes
            Engine.reply[Engine.reply.lastIndex] = 7
            f.controller.setup()
            f.activity.setContentView(f.parent)
            f.touch(0)
            // Run the system's timeout, not a wall-clock wait; the hit answer still crosses Native.under.
            org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idleFor(
                java.time.Duration.ofMillis(android.view.ViewConfiguration.getLongPressTimeout().toLong()))
            f.touch(1, time = 800)
            assertEquals(if (takes) emptyList<Int>() else listOf(0, 1), f.events)
            assertEquals(listOf(0, 4, 3), Engine.touches)
            f.controller.pause().stop().destroy()
        }
    }

    @Test fun anExactContextMenuLeavesAnOngoingRefreshVisible() {
        val f = Fixture(scroller = true)
        f.controller.setup()
        f.activity.setContentView(f.parent)
        f.parent.layout(0, 0, 600, 800)
        Engine.reply[4] = 2 or 16
        f.touch(0, x = 250f)
        f.touch(2, x = 250f, y = 200f, time = 120)
        f.touch(2, x = 250f, y = 400f, time = 140)
        f.touch(1, x = 250f, y = 400f, time = 160)
        val indicator = (0 until f.parent.childCount).map { f.parent.getChildAt(it) }.filterIsInstance<Refresh>().single()
        assertTrue(indicator.spinning)
        val scrolling = ReflectionHelpers.getField<Scrolling>(f.view, "scrolling")
        scrolling.told(intArrayOf(9, 2 or 16 or 32, 0, 0, 0, 0, 0, 500f.toBits(), 500f.toBits()))
        Engine.reply[4] = 2 or 16 or 32
        Engine.reply[Engine.reply.lastIndex] = 7
        Engine.takesLongPress = true
        f.touch(0, time = android.os.SystemClock.uptimeMillis())
        org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idleFor(
            java.time.Duration.ofMillis(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 1))
        assertTrue(Engine.touches.contains(4))
        assertSame("opening a context menu must leave the refresh indicator attached", f.parent, indicator.parent)
        assertTrue(indicator.spinning)
        f.touch(1, time = android.os.SystemClock.uptimeMillis())
        f.view.running(false, false)
        f.controller.pause().stop().destroy()
    }

    @Test fun aFirstPointerLiftBeforeSlopCannotStartADragFromTheRemainingFinger() {
        val f = Fixture(scroller = true)
        Engine.beforeReply = {
            f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(100f, 100f), 110)
            f.fingers(6, intArrayOf(0, 1), floatArrayOf(100f, 100f), 120)
            f.fingers(2, intArrayOf(1), floatArrayOf(20f), 140)
            f.fingers(1, intArrayOf(1), floatArrayOf(20f), 160)
        }
        f.touch(0)
        assertEquals(listOf(0, 5, 6, 2, 1), f.events)
        assertTrue(Engine.scrolls.isEmpty())
    }

    @Test fun aCancelBeforeTheReplyRemainsACancelAndAnOldAllocationGetsNoDown() {
        val f = Fixture()
        Engine.beforeReply = { f.touch(3, time = 120) }
        f.touch(0)
        assertEquals(listOf(0, 3), f.events)
        f.events.clear()
        Engine.beforeReply = {
            f.natives.ensure(7, 2, 200, 300, "{\"module\":\"test-map\",\"slot\":2,\"generation\":4}", 0)
            f.touch(1, time = 200)
        }
        f.touch(0, time = 180)
        assertTrue(f.events.isEmpty())
    }

    @Test fun paddedAndBorderedNativeContentReceivesItsPaintedCoordinates() {
        val f = Fixture()
        // Border box (10,20), 20px padding and a 3px border; op 28 paints the content at (33,43).
        val at = 3
        Engine.reply[at + 3] = 246f.toBits()
        Engine.reply[at + 4] = 346f.toBits()
        var painted = 0f to 0f
        val canvas = object : android.graphics.Canvas() {
            override fun drawRenderNode(node: android.graphics.RenderNode) {
                val origin = floatArrayOf(0f, 0f)
                matrix.mapPoints(origin)
                painted = origin[0] to origin[1]
            }
        }
        val ops = intArrayOf(28, 7, 2, 33f.toBits(), 43f.toBits(), 200f.toBits(), 300f.toBits()) + IntArray(9)
        Player(emptyMap()).also { it.native = f.natives::node }.replay(canvas, java.nio.IntBuffer.wrap(ops), 0, ops.size)
        assertEquals(33f to 43f, painted)
        Engine.reply[at + 8] = painted.first.toBits()
        Engine.reply[at + 9] = painted.second.toBits()
        val native = f.natives.taker(7, 2, 3)!!
        assertEquals(200, native.width)
        assertEquals(300, native.height)
        f.touch(0, x = 38f, y = 48f)
        f.touch(1, x = 228f, y = 338f, time = 120)
        assertEquals(listOf(5f to 5f, 195f to 295f), f.points)
        assertEquals(33f, native.translationX)
        assertEquals(43f, native.translationY)
    }

    @Test fun anExactLongPressWinsBeforeANativeLongClickEvenWithADelayedReply() {
        for (takes in listOf(false, true)) for (claims in listOf(0, 1)) {
            val f = Fixture(longClickable = true, scroller = true, claims = claims)
            f.controller.setup()
            f.activity.setContentView(f.parent)
            f.parent.layout(0, 0, 600, 800)
            Engine.takesLongPress = takes
            Engine.reply[Engine.reply.lastIndex] = 7
            val timeout = android.view.ViewConfiguration.getLongPressTimeout().toLong()
            val main = org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper())
            f.touch(0, time = android.os.SystemClock.uptimeMillis())
            Engine.beforeReply = {
                main.idleFor(java.time.Duration.ofMillis(timeout * 2))
                assertEquals("native recognition cannot run before Exact answers", 0, f.longClicks)
            }
            main.idleFor(java.time.Duration.ofMillis(timeout))
            main.idleFor(java.time.Duration.ofMillis(timeout * 2))
            assertEquals(if (takes) 0 else 1, f.longClicks)
            if (takes) {
                f.touch(2, y = 60f, time = android.os.SystemClock.uptimeMillis())
                f.touch(2, y = 20f, time = android.os.SystemClock.uptimeMillis())
            }
            f.touch(1, time = android.os.SystemClock.uptimeMillis())
            assertTrue("a resolved long press owns the rest of the sequence", Engine.deltas.isEmpty())
            assertEquals(1, Engine.touches.count { it == 4 })
            f.view.running(false, false)
            f.controller.pause().stop().destroy()
        }
    }

    @Test fun aStationaryNativeLongClickWorksInsideASwipeableRow() {
        for (claims in listOf(0, 1, 2)) for (nested in listOf(false, true)) {
            val f = Fixture(longClickable = true, clickable = true, scroller = true, claims = claims, nested = nested)
            f.controller.setup()
            f.activity.setContentView(f.parent)
            f.parent.layout(0, 0, 600, 800)
            f.touch(0, time = android.os.SystemClock.uptimeMillis())
            val main = org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper())
            val timeout = android.view.ViewConfiguration.getLongPressTimeout().toLong()
            main.idleFor(java.time.Duration.ofMillis(timeout + 1))
            assertEquals("claims=$claims, nested=$nested: the long click fires before release", 1, f.longClicks)
            f.touch(1, time = android.os.SystemClock.uptimeMillis())
            main.idleFor(java.time.Duration.ofMillis(timeout * 2))
            assertEquals("claims=$claims, nested=$nested: one long click at the normal deadline", 1, f.longClicks)
            assertEquals("a long click must not also click", 0, f.clicks)
            assertEquals(listOf(0, 1), f.events)
            assertTrue(Engine.deltas.isEmpty())
            f.view.running(false, false)
            f.controller.pause().stop().destroy()
        }
    }

    @Test fun aHeldNativeButtonStillYieldsToAnExactSwipe() {
        for (context in listOf(false, true)) {
            val f = Fixture(clickable = true, scroller = true, claims = 1)
            f.controller.setup()
            f.activity.setContentView(f.parent)
            f.parent.layout(0, 0, 600, 800)
            if (context) Engine.reply[Engine.reply.lastIndex] = 7
            f.touch(0, time = android.os.SystemClock.uptimeMillis())
            val main = org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper())
            main.idleFor(java.time.Duration.ofMillis(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 1))
            f.touch(2, x = 150f, time = android.os.SystemClock.uptimeMillis())
            f.touch(1, x = 150f, time = android.os.SystemClock.uptimeMillis())
            main.idle()
            assertEquals("the row must receive its swipe after the hold (context=$context)", listOf(2, 1), Engine.touches.takeLast(2))
            assertEquals(listOf(0, 3), f.events)
            assertEquals(0, f.clicks)
            assertTrue(Engine.deltas.isEmpty())
            f.view.running(false, false)
            f.controller.pause().stop().destroy()
        }
    }

    private fun secondPointerCancelsContextMenu(late: Boolean) {
        val f = Fixture(claims = 1, scroller = true)
        f.controller.setup()
        f.activity.setContentView(f.parent)
        f.parent.layout(0, 0, 600, 800)
        Engine.takesLongPress = true
        Engine.reply[Engine.reply.lastIndex] = 7
        val main = org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper())
        val timeout = android.view.ViewConfiguration.getLongPressTimeout().toLong()
        val second = {
            f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(100f, 120f), android.os.SystemClock.uptimeMillis())
        }
        if (late) Engine.beforeReply = {
            second()
            main.idleFor(java.time.Duration.ofMillis(timeout + 1))
        }
        f.touch(0, time = android.os.SystemClock.uptimeMillis())
        if (!late) second()
        main.idleFor(java.time.Duration.ofMillis(timeout + 1))
        assertFalse("a second pointer cancels Exact's context menu", Engine.touches.contains(4))
        repeat(250) { i ->
            f.fingers(2, intArrayOf(0, 1), floatArrayOf(100f, 120f + i), android.os.SystemClock.uptimeMillis())
            assertEquals("the stationary first pointer must not delay native pinch delivery", listOf(0, 5) + List(i + 1) { 2 }, f.events)
        }
        assertTrue(Engine.scrolls.isEmpty())
        f.fingers(6 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(100f, 120f), android.os.SystemClock.uptimeMillis())
        f.touch(1, time = android.os.SystemClock.uptimeMillis())
        assertEquals(listOf(0, 5) + List(250) { 2 } + listOf(6, 1), f.events)
        f.view.running(false, false)
        f.controller.pause().stop().destroy()
    }

    @Test fun aSecondPointerCancelsTheContextMenuWithAnImmediateHit() = secondPointerCancelsContextMenu(false)
    @Test fun aSecondPointerCancelsTheContextMenuWithADelayedHit() = secondPointerCancelsContextMenu(true)

    @Test fun aDeclinedLongPressReplaysBufferedMovesBeforeSamplesQueuedDuringItsReply() {
        val f = Fixture()
        f.controller.setup(); f.activity.setContentView(f.parent)
        Engine.reply[Engine.reply.lastIndex] = 7
        val now = android.os.SystemClock.uptimeMillis()
        f.touch(0, time = now)
        f.touch(2, x = 101f, time = now + 10)
        Engine.deferReplies = true
        org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idleFor(
            java.time.Duration.ofMillis(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 1))
        assertEquals(1, Engine.replies.size)
        f.touch(2, x = 102f, time = now + 600)
        f.touch(1, x = 102f, time = now + 610)
        Engine.deferReplies = false
        f.answer()
        assertEquals(listOf(0, 2, 2, 1), f.events)
        assertEquals(listOf(now, now + 10, now + 600, now + 610), f.times)
        f.view.running(false, false); f.controller.pause().stop().destroy()
    }

    @Test fun aSecondPointerDuringTheLongPressReplyPreemptsTheMenu() {
        val f = Fixture(claims = 1, scroller = true)
        f.controller.setup(); f.activity.setContentView(f.parent)
        Engine.reply[Engine.reply.lastIndex] = 7
        Engine.takesLongPress = true
        val now = android.os.SystemClock.uptimeMillis()
        f.touch(0, time = now)
        f.touch(2, x = 101f, time = now + 10)
        Engine.deferReplies = true
        org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idleFor(
            java.time.Duration.ofMillis(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 1))
        assertEquals(1, Engine.replies.size)
        f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(100f, 120f), now + 600)
        Engine.deferReplies = false
        f.answer()
        assertEquals(listOf(0, 2, 5), f.events)
        assertFalse("a queued second finger must preempt the long-press action", Engine.touches.contains(4))
        f.fingers(2, intArrayOf(0, 1), floatArrayOf(100f, 160f), now + 620)
        f.fingers(3, intArrayOf(0, 1), floatArrayOf(100f, 160f), now + 640)
        assertEquals(listOf(0, 2, 5, 2, 3), f.events)
        assertTrue(Engine.scrolls.isEmpty())
        f.view.running(false, false); f.controller.pause().stop().destroy()
    }

    @Test fun extraFingerMovesBeforeStartupReachExactInOrder() {
        val f = Fixture()
        f.view.started = false
        f.touch(0, x = 250f)
        f.fingers(5 or (1 shl 8), intArrayOf(0, 8), floatArrayOf(100f, 120f), 110, floatArrayOf(250f, 260f))
        f.fingers(2, intArrayOf(0, 8), floatArrayOf(100f, 160f), 120, floatArrayOf(250f, 270f))
        assertTrue(Engine.fingers.isEmpty())
        f.started()
        assertEquals(listOf(0 to 8, 2 to 8), Engine.fingers)
        assertEquals(listOf(260f to 120f, 270f to 160f), Engine.fingerPoints)
        f.fingers(6 or (1 shl 8), intArrayOf(0, 8), floatArrayOf(100f, 160f), 130, floatArrayOf(250f, 270f))
        f.touch(1, x = 250f, time = 140)
        assertEquals(listOf(0 to 8, 2 to 8, 1 to 8), Engine.fingers)
    }

    @Test fun aSecondPointerOverridesAnAlreadyDispatchedLongPressReply() {
        val f = Fixture(claims = 1, scroller = true)
        f.controller.setup(); f.activity.setContentView(f.parent)
        Engine.reply[Engine.reply.lastIndex] = 7
        Engine.takesLongPress = true
        val now = android.os.SystemClock.uptimeMillis()
        f.touch(0, time = now)
        lateinit var delayAfterAction: () -> Unit
        delayAfterAction = {
            if (Engine.touches.contains(4)) Engine.deferReplies = true
            else Engine.beforeReply = delayAfterAction
        }
        Engine.beforeReply = delayAfterAction
        org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idleFor(
            java.time.Duration.ofMillis(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 1))
        assertEquals(1, Engine.replies.size)
        assertTrue(Engine.touches.contains(4))
        f.fingers(5 or (1 shl 8), intArrayOf(0, 1), floatArrayOf(100f, 120f), now + 600)
        Engine.deferReplies = false
        f.answer()
        assertEquals(listOf(0, 5), f.events)
        f.fingers(2, intArrayOf(0, 1), floatArrayOf(100f, 160f), now + 620)
        f.fingers(3, intArrayOf(0, 1), floatArrayOf(100f, 160f), now + 640)
        assertEquals(listOf(0, 5, 2, 3), f.events)
        assertTrue(Engine.scrolls.isEmpty())
        f.view.running(false, false); f.controller.pause().stop().destroy()
    }

    @Test fun aDownBeforeOwnerStartIsTrackedAndResolvedWhenTheOwnerStarts() {
        for (inside in listOf(false, true)) {
            val f = Fixture(scroller = true, disallow = true)
            f.view.started = false
            val x = if (inside) 100f else 250f
            f.touch(0, x = x)
            assertEquals(0, Engine.asks)
            assertTrue(Engine.touches.isEmpty())
            f.touch(2, x = x, y = 60f, time = 120)
            assertEquals(0, Engine.asks)
            assertTrue(Engine.fingers.isEmpty())
            f.started()
            f.touch(2, x = x, y = 20f, time = 140)
            f.touch(3, x = x, y = 20f, time = 160)
            assertEquals(if (inside) listOf(0, 2, 2, 3) else emptyList<Int>(), f.events)
            assertEquals(if (inside) emptyList<Float>() else listOf(40f - f.slop, 40f), Engine.scrolls)
            f.view.running(false, false)
        }
    }

    @Test fun startupOverflowDiscardsTheWholeContactInsteadOfReplayingAnUnreleasedDown() {
        val f = Fixture()
        f.view.started = false
        f.touch(0)
        repeat(241) { f.touch(2, time = 110L + it) }
        f.touch(1, time = 400)
        f.started()
        assertTrue(f.events.isEmpty())
        assertNoScrollContact(f)
        f.touch(0, time = 500)
        f.touch(1, time = 520)
        assertEquals(listOf(0, 1), f.events)
        f.view.running(false, false)
    }

    @Test fun aCompletedContactBeforeOwnerStartLeavesNoUnansweredScroll() {
        val f = Fixture(scroller = true)
        f.view.started = false
        f.touch(0)
        f.touch(1, time = 120)
        f.started()
        assertEquals(listOf(0, 1), f.events)
        val scrolling = ReflectionHelpers.getField<Scrolling>(f.view, "scrolling")
        assertTrue("no pre-start serial can ever receive a hit reply",
            ReflectionHelpers.getField<Collection<*>>(scrolling, "owing").isEmpty())
        f.view.running(false, false)
    }

}
