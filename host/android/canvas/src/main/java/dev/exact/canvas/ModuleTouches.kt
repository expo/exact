package dev.exact.canvas

import android.view.MotionEvent

/** A module button's contact waits for its own hit answer, including after it lifts. */
internal class ModuleTouches(private val view: ExactView) {
    private var target = 0
    private var slot = 0
    private var generation = 0
    private var session = 0L
    private var awaiting = 0
    private var longPresses = 0
    private var released: MotionEvent? = null
    private var serial = 0
    private var x = 0f
    private var y = 0f
    private var downX = 0f
    private var downY = 0f
    private val later = ArrayDeque<MotionEvent>()
    private var draining = false

    /** A later contact must not reach exact2 before this one's release. */
    fun queued(e: MotionEvent): Boolean {
        if (released == null) return false
        later.addLast(MotionEvent.obtain(e))
        return true
    }

    fun begin(e: MotionEvent) {
        cancel()
        downX = e.x; downY = e.y
    }

    fun moved(e: MotionEvent, slop: Int) {
        if (Math.abs(e.x - downX) > slop || Math.abs(e.y - downY) > slop) cancel()
    }

    fun down(e: MotionEvent) {
        val contact = serial
        awaiting = 1
        view.under(e.x, e.y) { words ->
            if (contact != serial) return@under
            val at = 3 + 11 * words.getOrElse(1) { 0 }
            if (words.size >= at + 10) {
                target = words[at]
                x = Float.fromBits(words[at + 1]); y = Float.fromBits(words[at + 2])
                slot = words[at + 5]; generation = words[at + 6]
                longPresses = words[at + 7]
                session = (words[at + 8].toLong() and 0xffffffffL) or (words[at + 9].toLong() shl 32)
            }
            answered()
        }
    }

    fun longPressed(x: Float, y: Float) {
        if (target == 0 && awaiting == 0) return
        val contact = serial
        awaiting++
        view.ask(x, y) { words ->
            if (contact != serial) return@ask
            val at = 3 + 11 * words.getOrElse(1) { 0 }
            // A taken context menu consumes this contact's click.
            if (words.getOrElse(at + 7) { longPresses } != longPresses) target = 0
            answered()
            drain()
        }
    }

    fun release(e: MotionEvent): Boolean {
        if (awaiting == 0 && target == 0) return false
        view.hold(null)
        Exact.touched = e.eventTime
        if (awaiting > 0) released = MotionEvent.obtain(e) else complete(e)
        return true
    }

    fun cancel() {
        target = 0; awaiting = 0; serial++
        released?.let { e ->
            released = null
            view.forward(3, e.x, e.y, e.getToolType(0), -1L)
            e.recycle()
        }
    }

    fun stopped() {
        for (e in later) e.recycle()
        later.clear()
        cancel()
    }

    private fun complete(e: MotionEvent) {
        val clicked = target != 0 && view.natives?.click(target, slot, generation, session, x, y) == true
        cancel()
        view.forward(if (clicked) 3 else 1, e.x, e.y, e.getToolType(0), -1L)
    }

    private fun answered() {
        if (--awaiting > 0) return
        released?.let { e ->
            released = null
            complete(e)
            e.recycle()
        }
    }

    /** After scrolling has taken its hit answer and held events, replay the following contacts in order. */
    fun drain() {
        if (draining) return
        draining = true
        try {
            while (released == null && later.isNotEmpty()) {
                val e = later.removeFirst()
                try { view.replayTouch(e) } finally { e.recycle() }
            }
        } finally { draining = false }
    }
}
