package dev.exact.canvas

import android.graphics.Matrix
import android.view.MotionEvent
import android.view.View

/** A hosted view's contact (LLP 1133.008 D8): Android's down result admits delivery until release or interception. */
internal class ModuleContact(private val transform: (View, Matrix) -> Boolean) {
    private var target: View? = null
    private var last: MotionEvent? = null
    private val point = FloatArray(2)
    private val localToReader = Matrix()
    private val readerToLocal = Matrix()
    val active get() = target != null
    var owned = false
        private set

    fun begin(view: View, down: MotionEvent, x: Float, y: Float, place: Boolean = true): Boolean {
        localToReader.setTranslate(x, y)
        readerToLocal.setTranslate(-x, -y)
        if (send(view, down) != true) return false
        if (place) {
            point.fill(0f); localToReader.mapPoints(point)
            view.translationX = point[0]; view.translationY = point[1]
        }
        target = view; owned = true
        remember(down)
        return true
    }

    fun deliver(e: MotionEvent): View? {
        val receiver = target?.takeIf { send(it, e) != null }
        remember(e)
        if (e.actionMasked == MotionEvent.ACTION_UP || e.actionMasked == MotionEvent.ACTION_CANCEL) {
            target = null; last?.recycle(); last = null; owned = false
        }
        return receiver
    }

    /** Whether the view is a frame's WebView that can scroll its page the way a finger moved by ([dx], [dy]) goes,
     * along the longer. Other views ask by `requestDisallowInterceptTouchEvent`; a clipped `TextView` would answer
     * that it can scroll and does not. */
    fun scrolls(dx: Float, dy: Float): Boolean {
        val v = (target as? android.webkit.WebView) ?: return false
        return if (kotlin.math.abs(dx) > kotlin.math.abs(dy)) v.canScrollHorizontally(if (dx < 0) 1 else -1)
        else v.canScrollVertically(if (dy < 0) 1 else -1)
    }

    /** Retirement consumes the remaining sequence, even if the same View is lent to another element. */
    fun retired(view: View): Boolean {
        if (target !== view) return false
        cancel()
        return true
    }

    fun cancel() {
        val v = target
        val e = last
        if (v != null && e != null) {
            val cancel = MotionEvent.obtain(e)
            cancel.action = MotionEvent.ACTION_CANCEL
            send(v, cancel)
            cancel.recycle()
        }
        target = null
        last?.recycle(); last = null
    }

    fun clear() { cancel(); owned = false }

    private fun remember(e: MotionEvent) { last?.recycle(); last = MotionEvent.obtain(e) }

    private fun send(view: View, e: MotionEvent): Boolean? {
        if (transform(view, localToReader) && !localToReader.invert(readerToLocal)) return null
        val own = MotionEvent.obtain(e)
        own.transform(readerToLocal)
        return try { view.dispatchTouchEvent(own) } finally { own.recycle() }
    }
}
