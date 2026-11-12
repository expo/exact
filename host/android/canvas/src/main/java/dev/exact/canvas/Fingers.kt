package dev.exact.canvas

import android.view.MotionEvent

/**
 * The fingers of a touch past its first. The first finger is the touch exact2 hears ([Native.touch]): a press, a scroll,
 * or a GPU canvas's pointer. Each other finger goes to exact2 by its own pointer id ([Native.finger]), and the canvas
 * holding the touch takes it as a pointer of its own where it lands on that canvas, and keeps it after the first finger
 * lifts. Where no canvas holds the touch, a second finger cancels it, as it always has. A scroll that takes the touch
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
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> return 0
            MotionEvent.ACTION_POINTER_DOWN -> {
                val i = e.actionIndex
                view.hold(null)
                send(0, e.getPointerId(i), e.getX(i), e.getY(i))
                return -1
            }
            MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_UP -> {
                val i = e.actionIndex
                val id = e.getPointerId(i)
                if (id != first) { if (down.containsKey(id)) send(1, id, e.getX(i), e.getY(i)); return -1 }
                // The first finger lifts, here or with the touch's last: its release, and the others go on.
                first = -1; index = i
                return 1
            }
            MotionEvent.ACTION_MOVE -> {
                for (i in 0 until e.pointerCount) {
                    val id = e.getPointerId(i)
                    if (id != first && down.containsKey(id)) send(2, id, e.getX(i), e.getY(i))
                }
                index = e.findPointerIndex(first)
                return if (index < 0) -1 else 2
            }
            else -> {
                // exact2 cancels the fingers it took with the touch.
                down.clear()
                index = e.findPointerIndex(first).coerceAtLeast(0); first = -1
                return 3
            }
        }
    }

    /** The first finger's event [e] to exact2 as [action] ([ExactView.forward]). */
    fun forward(e: MotionEvent, action: Int) = view.forward(action, e.getX(index), e.getY(index), e.getToolType(index), -1L)

    /** [e] to exact2 as [touch] splits it: how [Fields] replays a touch it held for a field that was not the one
     * under it. */
    fun relay(e: MotionEvent) { val action = touch(e); if (action >= 0) forward(e, action) }

    private fun send(action: Int, id: Int, x: Float, y: Float) {
        val at = (x.toRawBits().toLong() shl 32) or (y.toRawBits().toLong() and 0xffffffffL)
        when (action) {
            0 -> down[id] = at
            2 -> if (down.put(id, at) == at) return
            else -> down.remove(id)
        }
        if (!view.started) return
        val o = view.owner
        if (o == null) { Native.finger(action, id, x, y); if (view.started) view.pump(); return }
        if (action != 2) o.input()
        // A move waits for nothing: several fingers cost the main thread no frame time.
        if (o.finger(action, id, x, y, if (action == 2) 0L else view.releaseWait)) view.publish()
    }
}
