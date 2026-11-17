package dev.exact.canvas

import android.view.GestureDetector
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.VelocityTracker
import android.view.ViewConfiguration
import android.widget.OverScroller
import android.view.Choreographer
import kotlin.math.abs
import kotlin.math.hypot

/**
 * A zoomable scroller's own gestures (LLP 1057.001.000 §5, "Android"), with the platform's classes as its photo
 * viewers use them: the pinch and the quick scale (a double tap held and dragged) [ScaleGestureDetector] recognizes,
 * the double tap [GestureDetector] recognizes, and while the scroller can move, a one-finger pan along both axes and
 * its fling ([OverScroller]). exact2 owns the zoom and the offset: it is told the pinch's scale and focal point
 * ([Native.zoom]), the double tap ([Native.zoomTap]) and the pan's moves, as a drag's ([Scrolling]).
 *
 * [Scrolling] hands it each touch whose answer names a zoomable scroller under the down ([down]), then the touch's
 * events ([touch]). From when one of its gestures begins the touch is its own: exact2's contact is cancelled, as for
 * a scroll ([ExactView.cancelContact]), and a drag [Scrolling] had is over. Until then a one-finger drag of a
 * scroller that cannot move is [Scrolling]'s, which hands it to the scrollers outside.
 */
internal class Zooming(private val view: ExactView, private val host: Host) {
    /** What [Scrolling] does for it. */
    interface Host {
        /** Move [s] by ([dx], [dy]) px as the reader's drag, within its travel. */
        fun pan(s: Scrolling.Scroller, dx: Float, dy: Float)
        /** [f], a call that tells exact2, on its thread behind the moves sent before. */
        fun tell(f: () -> Unit)
        /** The touch is this one's: a drag [Scrolling] had ends where it is, and nothing more of the touch is its. */
        fun took(x: Float, y: Float, tool: Int)
    }

    private val config = ViewConfiguration.get(view.context)
    private val slop = config.scaledTouchSlop
    private val minFling = config.scaledMinimumFlingVelocity
    private val maxFling = config.scaledMaximumFlingVelocity
    private val frames = Choreographer.getInstance()

    /** The innermost zoomable scroller under the touch, and what the touch's answer said of it. */
    private var target: Scrolling.Scroller? = null
    private var taps = false
    private var pans = false
    /** The touch is this one's. */
    var owns = false
        private set
    private var scaling = false
    /** This touch scaled (a pinch, a quick scale): its double tap does not zoom when it lifts. */
    private var scaled = false
    /** This touch is a double tap's second. */
    private var doubled = false
    /** The pan has begun: exact2 was told the drag began. */
    private var panning = false
    private var pointer = -1
    private var tool = 0
    private var downX = 0f; private var downY = 0f
    private var lastX = 0f; private var lastY = 0f
    private var focusX = 0f; private var focusY = 0f
    private var tracker: VelocityTracker? = null
    private val fling = OverScroller(view.context)
    private var flingX = 0; private var flingY = 0
    private var flung: Scrolling.Scroller? = null
    /** A pinch ended with a finger still down, which pans once the scroller can move. */
    private var panAfter = false
    /** A fling a down caught: that touch pans from its first move. */
    private var caught = false

    /** Its fling runs. */
    val active get() = flung != null

    /** Its fling moves [s]. */
    fun flinging(s: Scrolling.Scroller) = flung === s

    private val scale = ScaleGestureDetector(view.context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScaleBegin(d: ScaleGestureDetector): Boolean {
            val s = target ?: return false
            if (!s.port.contains(d.focusX, d.focusY)) return false
            take(d.focusX, d.focusY)
            endPan(0f, 0f)
            scaling = true; scaled = true
            focusX = d.focusX; focusY = d.focusY
            host.tell { Native.zoom(s.id, 0, 1f, 0f, 0f, 0f, 0f) }
            return true
        }

        override fun onScale(d: ScaleGestureDetector): Boolean {
            val s = target ?: return false
            val (x0, y0, f) = Triple(focusX, focusY, d.scaleFactor)
            val (x1, y1) = d.focusX to d.focusY
            focusX = x1; focusY = y1
            host.tell { Native.zoom(s.id, 1, f, x0, y0, x1, y1) }
            return true
        }

        override fun onScaleEnd(d: ScaleGestureDetector) {
            val s = target ?: return
            scaling = false
            host.tell { Native.zoom(s.id, 2, 1f, 0f, 0f, 0f, 0f) }
        }
    }).apply { isQuickScaleEnabled = true }

    private val gestures = GestureDetector(view.context, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDoubleTap(e: MotionEvent): Boolean {
            if (!taps) return false
            // The second tap is the double tap's: it presses nothing, and the first tap's press, which waited for
            // it, never comes (D5).
            take(e.x, e.y)
            doubled = true
            val (x, y) = e.x to e.y
            host.tell { Native.zoomTap(0, x, y) }
            return true
        }
    }).apply { setIsLongpressEnabled(false) }

    /**
     * A touch came down at [e] ([Scrolling.answered]): over [s], the innermost zoomable scroller under it, or none.
     * [taps]: a double tap there zooms it; [pans]: a drag from there is its pan, the scroller being the innermost one
     * under the touch, able to move, and no gesture of exact2's claiming the drag.
     */
    fun down(e: MotionEvent, s: Scrolling.Scroller?, taps: Boolean, pans: Boolean) {
        target = s; this.taps = taps; this.pans = pans || (caught && s != null)
        owns = caught && s != null; panning = false; panAfter = false
        scaling = false; scaled = false; doubled = false
        pointer = e.getPointerId(0); tool = e.getToolType(0)
        downX = e.x; downY = e.y; lastX = e.x; lastY = e.y
        tracker?.recycle()
        tracker = VelocityTracker.obtain().also { it.addMovement(e) }
        caught = false
        if (s == null) return
        scale.onTouchEvent(e)
        gestures.onTouchEvent(e)
    }

    /** A down at ([x], [y]) while its fling runs over the scroller: the fling stops, and the touch is the pan's. */
    fun catches(x: Float, y: Float): Boolean {
        val s = flung ?: return false
        halt()
        caught = s.port.contains(x, y)
        return caught
    }

    /** [e], an event after the touch's down: whether it is this one's. */
    fun touch(e: MotionEvent): Boolean {
        if (target == null) return false
        tracker?.addMovement(e)
        scale.onTouchEvent(e)
        gestures.onTouchEvent(e)
        when (e.actionMasked) {
            MotionEvent.ACTION_MOVE -> {
                if (scaling || e.pointerCount > 1) return owns
                val i = e.findPointerIndex(pointer).takeIf { it >= 0 } ?: 0.also { pointer = e.getPointerId(0) }
                val x = e.getX(i); val y = e.getY(i)
                if (!panning && pans && !doubled && (owns || hypot(x - downX, y - downY) > slop)) {
                    take(x, y)
                    beginPan(x, y)
                } else if (panAfter && target!!.let { it.maxX > 0f || it.maxY > 0f }) {
                    panAfter = false
                    beginPan(lastX, lastY)
                }
                if (panning) {
                    val s = target!!
                    host.pan(s, lastX - x, lastY - y)
                    lastX = x; lastY = y
                }
            }
            MotionEvent.ACTION_POINTER_UP -> {
                // A finger lifts and one still down is followed, from where it is.
                val i = e.actionIndex
                val other = if (i == 0) 1 else 0
                if (other < e.pointerCount) { pointer = e.getPointerId(other); lastX = e.getX(other); lastY = e.getY(other) }
                // After a pinch the finger left pans on, once the scroller can move.
                if (owns && !panning && e.pointerCount == 2) panAfter = true
                tracker?.clear()
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                val lifted = e.actionMasked == MotionEvent.ACTION_UP
                if (lifted && doubled && !scaled) { val (x, y) = e.x to e.y; target?.let { host.tell { Native.zoomTap(1, x, y) } } }
                var vx = 0f; var vy = 0f
                val t = tracker
                if (lifted && panning && t != null) {
                    t.computeCurrentVelocity(1000, maxFling.toFloat())
                    vx = -t.getXVelocity(pointer); vy = -t.getYVelocity(pointer)
                }
                val was = owns
                endPan(vx, vy)
                owns = false; target = null
                tracker?.recycle(); tracker = null
                return was
            }
        }
        return owns
    }

    /** The touch is this one's from here. */
    private fun take(x: Float, y: Float) {
        if (owns && !caught) return
        owns = true
        host.took(x, y, tool)
    }

    private fun beginPan(x: Float, y: Float) {
        val s = target ?: return
        panning = true
        lastX = x; lastY = y
        host.tell { Native.scrollDrag(s.id, false, 0f, 0f, false, 0f, 0f, false) }
    }

    /** The pan ends at ([vx], [vy]) px/s, positive as the offset grows: a fling, or rest. */
    private fun endPan(vx: Float, vy: Float) {
        val s = target ?: return
        if (!panning) return
        panning = false
        val flings = hypot(vx, vy) >= minFling
        if (flings) {
            fling.fling(0, 0, vx.toInt(), vy.toInt(), Int.MIN_VALUE, Int.MAX_VALUE, Int.MIN_VALUE, Int.MAX_VALUE)
            flingX = 0; flingY = 0
            flung = s
            frames.postFrameCallback(flinger)
        }
        val restX = s.at(true) + if (flings) fling.finalX.toFloat() else 0f
        val restY = s.at(false) + if (flings) fling.finalY.toFloat() else 0f
        host.tell { Native.scrollDrag(s.id, true, vx, vy, true, restX, restY, flings) }
    }

    /** The fling ends where it is; exact2 is told it stopped. */
    fun halt() {
        val s = flung ?: return
        flung = null
        fling.forceFinished(true)
        frames.removeFrameCallback(flinger)
        host.tell { Native.flingEnded(s.id) }
    }

    /** The activity stopped: what is down or flinging ends. */
    fun stopped() {
        halt()
        if (panning) endPan(0f, 0f)
        if (scaling) { scaling = false; target?.let { s -> host.tell { Native.zoom(s.id, 2, 1f, 0f, 0f, 0f, 0f) } } }
        owns = false; target = null; caught = false
        tracker?.recycle(); tracker = null
    }

    private val flinger: Choreographer.FrameCallback = Choreographer.FrameCallback {
        val s = flung ?: return@FrameCallback
        if (!fling.computeScrollOffset()) { halt(); view.timerReport(); return@FrameCallback }
        val dx = (fling.currX - flingX).toFloat(); val dy = (fling.currY - flingY).toFloat()
        flingX = fling.currX; flingY = fling.currY
        val (x, y) = s.at(true) to s.at(false)
        host.pan(s, dx, dy)
        // At its ends it goes no further.
        val moved = abs(s.at(true) - x) + abs(s.at(false) - y) > 0.01f
        if (fling.isFinished || !moved && abs(dx) + abs(dy) > 0.5f) halt() else frames.postFrameCallback(flinger)
        view.timerReport()
    }
}
