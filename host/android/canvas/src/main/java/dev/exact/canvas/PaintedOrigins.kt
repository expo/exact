package dev.exact.canvas

import android.graphics.Matrix
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.RenderNode
import java.util.WeakHashMap

/** A path through the recordings that paint a hosted view, from the view to the reader. */
internal class PaintedOrigins {
    data class Placement(val node: RenderNode, val x: Float, val y: Float, val clip: Rect? = null, val kept: Matrix? = null)

    companion object {
        /** The host asks for WebView features when the plan first contains an iframe, before its first drawing. */
        @Volatile var planHasFrames = false
    }
    /** A full stream is being ingested. Its placements become visible together when the reader replays its root. */
    var publishing = false
    @Volatile var tracksFrames = false
    @Volatile var hasHostedViews = false
    val frameClips get() = tracksFrames || planHasFrames || hasHostedViews
    @Volatile var version = 0L
        private set
    private var topology = 0L
    private val paths = WeakHashMap<RenderNode, List<Placement>>()
    private val owners = WeakHashMap<RenderNode, MutableSet<RenderNode>>()
    private val changed = WeakHashMap<RenderNode, Long>()
    // A worker's new row matters only once a displayed parent references it.
    private val recordings = WeakHashMap<RenderNode, List<Placement>>()
    private var root: List<Placement> = emptyList()

    @Synchronized fun moved(node: RenderNode) {
        if (!tracksFrames) return
        val frames = owners[node] ?: return
        version++
        for (frame in frames) changed[frame] = version
    }

    fun translate(node: RenderNode, x: Float, y: Float) {
        if ((node.setTranslationX(x) or node.setTranslationY(y)) && tracksFrames) moved(node)
    }

    @Synchronized fun record(node: RenderNode?, children: List<Placement>) {
        if (tracksFrames && children == (if (node == null) root else recordings[node].orEmpty())) return
        if (node == null) root = children else recordings[node] = children
        if (tracksFrames || paths.isNotEmpty()) {
            paths.clear(); owners.clear(); changed.clear()
            topology = ++version
        }
    }

    @Synchronized fun needsPlacement(frame: RenderNode, since: Long) =
        since < topology || (changed[frame] ?: 0L) > since

    private fun path(node: RenderNode, kept: Boolean): List<Placement>? {
        val result = ArrayList<Placement>()
        fun find(children: List<Placement>): Boolean {
            for (child in children) {
                if (child.kept != null && !kept) continue
                if (child.node !== node && !find(recordings[child.node].orEmpty())) continue
                result.add(child)
                return true
            }
            return false
        }
        return if (find(root)) result else null
    }

    @Synchronized fun transform(node: RenderNode, localToReader: Matrix, visible: RectF? = null): Boolean {
        // A kept route paints without taking a touch. Prefer the live placement when both are drawn.
        val route = if (visible == null) path(node, false).orEmpty() else paths.getOrPut(node) {
            val found = path(node, false) ?: path(node, true).orEmpty()
            for (edge in found) owners.getOrPut(edge.node) { HashSet() }.add(node)
            found
        }
        if (route.isEmpty()) return false
        localToReader.reset()
        val matrix = Matrix()
        for (edge in route) {
            edge.node.getMatrix(matrix)
            matrix.postTranslate(edge.node.left + edge.x, edge.node.top + edge.y)
            edge.kept?.let { matrix.postConcat(it) }
            localToReader.postConcat(matrix)
            if (visible != null) {
                if (edge.node.clipToBounds && !visible.intersect(0f, 0f, edge.node.width.toFloat(), edge.node.height.toFloat())) visible.setEmpty()
                matrix.mapRect(visible)
                edge.clip?.let { if (!visible.intersect(it.left.toFloat(), it.top.toFloat(), it.right.toFloat(), it.bottom.toFloat())) visible.setEmpty() }
            }
        }
        return true
    }
}
