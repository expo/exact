package dev.exact.canvas

import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.RenderNode
import android.view.View

/** The real window geometry of a WebView whose picture is recorded inside the painted page. */
internal class FrameWindow {
    private val matrix = Matrix()
    private val inverse = Matrix()
    private val applied = Matrix()
    private var drawScaleX = 1f
    private var drawScaleY = 1f
    private var drawRotation = 0f
    private val recordedAt = Matrix()
    private val contents = RenderNode("iframe contents").apply { setClipToBounds(false) }
    private val values = FloatArray(9)
    private val bounds = RectF()
    private val clip = RectF()
    private val point = FloatArray(2)
    var placed = false
        private set

    fun reset() { placed = false }

    fun place(view: View, node: RenderNode, graph: PaintedOrigins, hidden: Boolean): Boolean {
        bounds.set(0f, 0f, node.width.toFloat(), node.height.toFloat())
        if (hidden || !graph.transform(node, matrix, bounds)) {
            matrix.setTranslate(-node.width - 100000f, 0f)
            bounds.setEmpty()
        }
        if (placed && matrix == applied && clip == bounds) return false
        placed = true
        clip.set(bounds)
        applied.set(matrix)
        matrix.getValues(values)
        // View's window, accessibility and WebView all read its ordinary transform properties.
        val x = values[Matrix.MSCALE_X]; val y = values[Matrix.MSKEW_Y]
        val sx = kotlin.math.hypot(x, y)
        view.pivotX = 0f; view.pivotY = 0f
        view.translationX = values[Matrix.MTRANS_X]; view.translationY = values[Matrix.MTRANS_Y]
        val angle = if (sx == 0f) kotlin.math.atan2(-values[Matrix.MSKEW_X], values[Matrix.MSCALE_Y]) else kotlin.math.atan2(y, x)
        view.rotation = Math.toDegrees(angle.toDouble()).toFloat()
        view.scaleX = sx
        view.scaleY = if (sx == 0f) kotlin.math.hypot(values[Matrix.MSKEW_X], values[Matrix.MSCALE_Y])
            else (x * values[Matrix.MSCALE_Y] - y * values[Matrix.MSKEW_X]) / sx
        (view.parent as? View)?.postInvalidateOnAnimation()
        if (view.scaleX != drawScaleX || view.scaleY != drawScaleY || view.rotation != drawRotation) return true
        // drawChild keeps the live View node. Counter just its translation since the recording.
        if (view.matrix.invert(inverse)) {
            inverse.preConcat(recordedAt)
            inverse.getValues(values)
            contents.translationX = values[Matrix.MTRANS_X]
            contents.translationY = values[Matrix.MTRANS_Y]
        }
        return false
    }

    fun toReader(rect: Rect, offset: android.graphics.Point?): Boolean {
        bounds.set(rect)
        applied.mapRect(bounds)
        if (!bounds.intersect(clip)) return false
        bounds.roundOut(rect)
        if (offset != null) {
            point[0] = offset.x.toFloat(); point[1] = offset.y.toFloat()
            applied.mapPoints(point)
            offset.set(Math.round(point[0]), Math.round(point[1]))
        }
        return true
    }

    inline fun draw(canvas: Canvas, view: View, drawChild: (Canvas) -> Unit) {
        recordedAt.set(view.matrix)
        drawScaleX = view.scaleX; drawScaleY = view.scaleY; drawRotation = view.rotation
        contents.translationX = 0f; contents.translationY = 0f
        contents.setPosition(0, 0, view.width, view.height)
        val local = contents.beginRecording()
        if (recordedAt.invert(inverse)) {
            local.concat(inverse)
            local.translate(-view.left.toFloat(), -view.top.toFloat())
            drawChild(local)
        }
        contents.endRecording()
        canvas.drawRenderNode(contents)
    }
}
