package dev.exact.canvas

import android.os.Build
import android.view.MotionEvent

/**
 * The fingers of a touch past its first. The first finger is the touch exact2 hears ([Native.touch]): a press, a scroll,
 * or a GPU canvas's pointer. Each other finger goes to exact2 by its own pointer id ([Native.finger]), and the canvas
 * holding the touch takes it as a pointer of its own where it lands on that canvas, and keeps it after the first finger
 * lifts. A second finger can join a transform binding. A scroll that takes the touch
 * ([Scrolling]) cancels it in exact2, and exact2 then cancels every finger the canvas took.
 */
internal class Fingers(private val view: ExactView) {
    /** The first finger's pointer id while it is down, else -1. */
    private var first = -1
    /** The other fingers that are down, each at its last pixels (x's and y's bits): a move that moved none of them is
     * not sent. */
    private val down = HashMap<Int, Long>()
    /** The index of the first finger among the pointers of the event [touch] read last. */
    private var index = 0

    /** A touch began: before anything else (a hosted field, a scroll) can take its events. */
    fun began(e: MotionEvent) { first = e.getPointerId(0); down.clear() }

    /** What [e] is to the touch exact2 hears: 0 down, 1 up, 2 move or 3 cancel, of the first finger at [index]; -1 when
     * it was only the other fingers'. */
    fun touch(e: MotionEvent): Int {
        index = 0
        if (e.actionMasked == MotionEvent.ACTION_MOVE && down.isNotEmpty()) {
            val primary = e.findPointerIndex(first)
            for (h in 0 until e.historySize) {
                for (i in 0 until e.pointerCount) {
                    if (down.containsKey(e.getPointerId(i))) send(2, e, i, h)
                }
                if (primary >= 0) {
                    view.forward(2, e.getHistoricalX(primary, h), e.getHistoricalY(primary, h),
                        e.getToolType(primary), -1L, e.historyNanos(h), null)
                }
            }
        }
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> return 0
            MotionEvent.ACTION_POINTER_DOWN -> {
                val i = e.actionIndex
                view.hold(null)
                send(0, e, i)
                return -1
            }
            MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_UP -> {
                val i = e.actionIndex
                val id = e.getPointerId(i)
                if (id != first) { if (down.containsKey(id)) send(1, e, i); return -1 }
                // The first finger lifts, here or with the touch's last: its release, and the others go on.
                for (other in 0 until e.pointerCount) {
                    if (other != i && down.containsKey(e.getPointerId(other))) send(2, e, other)
                }
                first = -1; index = i
                return 1
            }
            MotionEvent.ACTION_MOVE -> {
                for (i in 0 until e.pointerCount) {
                    val id = e.getPointerId(i)
                    if (id != first && down.containsKey(id)) send(2, e, i)
                }
                index = e.findPointerIndex(first)
                return if (index < 0) -1 else 2
            }
            else -> {
                // exact2 cancels the fingers it took with the touch.
                for (i in 0 until e.pointerCount) if (down.containsKey(e.getPointerId(i))) send(3, e, i)
                down.clear()
                index = e.findPointerIndex(first).coerceAtLeast(0); first = -1
                return 3
            }
        }
    }

    /** The first finger's event [e] to exact2 as [action] ([ExactView.forward]), each at its own time. A move carries
     * the samples Android batched into it since the last ([MotionEvent.getHistoricalX]): a drag's release velocity reads
     * them (LLP 1057.001 §3), and a fast flick between two frames is mostly history. */
    fun forward(e: MotionEvent, action: Int) {
        val tool = e.getToolType(index)
        val history = e.takeIf { (action == 1 || action == 2) && it.pointerCount == 1 && it.historySize > 0 }
        val at = if (Build.VERSION.SDK_INT >= 34) e.eventTimeNanos else e.eventTime * 1_000_000L
        view.forward(action, e.getX(index), e.getY(index), tool, -1L, at, history)
    }

    /** [e] to exact2 as [touch] splits it: how [Fields] replays a touch it held for a field that was not the one
     * under it. */
    fun relay(e: MotionEvent) { val action = touch(e); if (action >= 0) forward(e, action) }

    private fun send(action: Int, e: MotionEvent, index: Int, history: Int = -1) {
        val id = e.getPointerId(index)
        fun x(i: Int) = if (history < 0) e.getX(i) else e.getHistoricalX(i, history)
        fun y(i: Int) = if (history < 0) e.getY(i) else e.getHistoricalY(i, history)
        val x = x(index); val y = y(index)
        val tool = e.getToolType(index)
        val time = if (history >= 0) e.historyNanos(history) else if (Build.VERSION.SDK_INT >= 34) e.eventTimeNanos else e.eventTime * 1_000_000L
        val at = (x.toRawBits().toLong() shl 32) or (y.toRawBits().toLong() and 0xffffffffL)
        when (action) {
            0 -> down[id] = at
            2 -> if (down.put(id, at) == at) return
            else -> down.remove(id)
        }
        if (!view.started) return
        val primary = e.findPointerIndex(first)
        val px = if (primary >= 0) x(primary) else Float.NaN
        val py = if (primary >= 0) y(primary) else Float.NaN
        val o = view.owner
        if (o == null) { Native.finger(action, id, x, y, tool, time, px, py); if (view.started) view.pump(); return }
        if (action != 2) o.input()
        // A move waits for nothing: several fingers cost the main thread no frame time.
        if (o.finger(action, id, x, y, tool, time, px, py, if (action == 2) 0L else view.releaseWait)) view.publish()
    }
}

private fun MotionEvent.historyNanos(h: Int) =
    if (Build.VERSION.SDK_INT >= 34) getHistoricalEventTimeNanos(h) else getHistoricalEventTime(h) * 1_000_000L

/** Replay a single finger's batch on the thread that owns the presenter, before sending its current position. */
internal fun MotionEvent.forwardHistory(tool: Int) {
    for (h in 0 until historySize) {
        val at = historyNanos(h)
        Native.touch(2, getHistoricalX(0, h), getHistoricalY(0, h), tool, at)
    }
}
