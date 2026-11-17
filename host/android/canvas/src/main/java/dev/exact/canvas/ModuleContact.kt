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

    fun begin(view: View, down: MotionEvent, x: Float, y: Float): Boolean {
        localToReader.setTranslate(x, y)
        readerToLocal.setTranslate(-x, -y)
        if (!send(view, down)) return false
        point.fill(0f); localToReader.mapPoints(point)
        view.translationX = point[0]; view.translationY = point[1]
        target = view; owned = true
        remember(down)
        return true
    }

    fun deliver(e: MotionEvent) {
        target?.let { send(it, e) }
        remember(e)
        if (e.actionMasked == MotionEvent.ACTION_UP || e.actionMasked == MotionEvent.ACTION_CANCEL) {
            target = null; last?.recycle(); last = null; owned = false
        }
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

    private fun send(view: View, e: MotionEvent): Boolean {
        if (transform(view, localToReader)) localToReader.invert(readerToLocal)
        val own = MotionEvent.obtain(e)
        own.transform(readerToLocal)
        return try { view.dispatchTouchEvent(own) } finally { own.recycle() }
    }
}

/** The placements recorded by Player, read through the nodes' current transforms when a contact moves. */
internal class PaintedOrigins {
    class Placement(val node: android.graphics.RenderNode, val x: Float, val y: Float)
    // A worker's new row matters only once the displayed parent references it. Weak keys let retired recordings go.
    private val recordings = java.util.WeakHashMap<android.graphics.RenderNode, List<Placement>>()
    private var root: List<Placement> = emptyList()

    @Synchronized fun record(node: android.graphics.RenderNode?, children: List<Placement>) {
        if (node == null) root = children else recordings[node] = children
    }

    @Synchronized fun transform(node: android.graphics.RenderNode, localToReader: Matrix): Boolean {
        val matrix = Matrix()
        fun find(children: List<Placement>): Boolean {
            for (child in children) {
                if (child.node === node) localToReader.reset()
                else if (!find(recordings[child.node] ?: emptyList())) continue
                child.node.getMatrix(matrix)
                localToReader.postConcat(matrix)
                localToReader.postTranslate(child.node.left + child.x, child.node.top + child.y)
                return true
            }
            return false
        }
        return find(root)
    }
}
