package dev.exact.canvas

import android.view.MotionEvent
import android.view.ScaleGestureDetector
import kotlin.math.hypot

/** The scale detector only observes. [Scrolling] keeps the reader's active pointer. */
internal class ReportingPinch(private val view: ExactView) : ScaleGestureDetector.SimpleOnScaleGestureListener() {
    private var detector: ScaleGestureDetector? = null
    private data class Listener(val index: Int, val generation: Int, val session: Long)
    private val paths = mutableMapOf<Int, List<Listener>>()
    private var enabled = false
    private var target: Listener? = null
    private var firstId = -1
    private var secondId = -1
    private var span = 0f
    private var scale = 1f
    private var x = 0f
    private var y = 0f
    private var active = false
    private var cancelled = false

    private fun send(f: () -> Unit) {
        val owner = view.owner
        if (owner != null) owner.call(f) else { f(); view.pump() }
    }

    private fun listeners(words: IntArray, start: Int, end: Int) = (start until end step 4).map { i ->
        Listener(words[i], words[i + 1], (words[i + 2].toLong() and 0xffffffffL) or (words[i + 3].toLong() shl 32))
    }

    fun arm(id: Int, hit: IntArray) {
        paths.clear()
        enabled = hit.isNotEmpty() && hit[0] and 128 != 0
        if (enabled) paths[id] = listeners(hit, hit.lastIndex - hit.last(), hit.lastIndex)
    }

    private fun enroll(e: MotionEvent) {
        val second = e.actionIndex
        if (e.getToolType(second) != MotionEvent.TOOL_TYPE_FINGER) return
        val cx = e.getX(second); val cy = e.getY(second)
        val path = view.owner?.ask { Native.pinchPath(cx, cy) } ?: Native.pinchPath(cx, cy)
        val owners = listeners(path, 0, path.size)
        paths[e.getPointerId(second)] = owners
        for (first in 0 until e.pointerCount) {
            if (first == second || e.getToolType(first) != MotionEvent.TOOL_TYPE_FINGER) continue
            val listener = paths[e.getPointerId(first)]?.firstOrNull { it in owners } ?: continue
            val initial = hypot(cx - e.getX(first), cy - e.getY(first))
            if (!initial.isFinite() || initial <= 0f) continue
            target = listener
            span = initial
            firstId = e.getPointerId(first)
            secondId = e.getPointerId(second)
            scale = 1f
            detector = ScaleGestureDetector(view.context, this).apply {
                isQuickScaleEnabled = false
                isStylusScaleEnabled = false
            }
            break
        }
    }

    fun observe(e: MotionEvent) {
        if (!enabled) return
        val action = e.actionMasked
        if (action == MotionEvent.ACTION_POINTER_UP || action == MotionEvent.ACTION_UP) paths.remove(e.getPointerId(e.actionIndex))
        if (action == MotionEvent.ACTION_POINTER_DOWN && detector == null) enroll(e)
        val watching = detector ?: return
        val a = e.findPointerIndex(firstId); val b = e.findPointerIndex(secondId)
        val lift = action == MotionEvent.ACTION_POINTER_UP && (e.actionIndex == a || e.actionIndex == b)
        cancelled = action == MotionEvent.ACTION_CANCEL || (lift && e.flags and MotionEvent.FLAG_CANCELED != 0)
        if (a >= 0 && b >= 0 && !cancelled) {
            x = (e.getX(a) + e.getX(b)) / 2
            y = (e.getY(a) + e.getY(b)) / 2
            val factor = hypot(e.getX(b) - e.getX(a), e.getY(b) - e.getY(a)) / span
            if (factor.isFinite() && factor > 0f) scale = factor
            if (e.pointerCount == 2) watching.onTouchEvent(e)
            else if (!active && (action == MotionEvent.ACTION_MOVE ||
                    (action == MotionEvent.ACTION_POINTER_DOWN && e.actionIndex == b))) {
                // Give the detector just its enrolled contacts in their original order.
                val indices = intArrayOf(a, b)
                val props = Array(2) { MotionEvent.PointerProperties().apply { e.getPointerProperties(indices[it], this) } }
                val coords = Array(2) { MotionEvent.PointerCoords().apply { e.getPointerCoords(indices[it], this) } }
                val pairAction = if (action == MotionEvent.ACTION_POINTER_DOWN) action or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT) else action
                val pair = MotionEvent.obtain(e.downTime, e.eventTime, pairAction, 2, props, coords,
                    e.metaState, e.buttonState, e.xPrecision, e.yPrecision, e.deviceId, e.edgeFlags, e.source, e.flags)
                try { watching.onTouchEvent(pair) } finally { pair.recycle() }
            }
        }
        // Android can end detection below its minimum span while both fingers
        // are still down. The recognized pair keeps reporting until a lift.
        if (active && action == MotionEvent.ACTION_MOVE) sample(false)
        if (lift || cancelled) {
            if (active) sample(true)
            detector = null
            active = false
        }
    }

    override fun onScaleBegin(detector: ScaleGestureDetector): Boolean {
        active = true
        return true
    }

    fun cancel() {
        enabled = false
        paths.clear()
        cancelled = true
        if (active) sample(true)
        active = false
        detector = null
    }

    private fun sample(released: Boolean) {
        val listener = target ?: return
        val factor = if (cancelled) 1f else scale
        val cx = x; val cy = y
        send { Native.pinch(listener.index, listener.generation, listener.session, released, factor, cx, cy) }
    }
}
