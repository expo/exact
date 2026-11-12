package dev.exact.canvas

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.RenderNode
import java.nio.IntBuffer
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/** LLP 1013.000: the arriving content stays live; only its normal slot is hidden.
 * Geometry is captured here, before a new stream changes native scroll or layer state. */
internal class SharedElements(
    private val player: Player,
    private val pictures: (Int) -> Bitmap?,
    private val layerMap: (Int, RectF) -> Unit,
    private val groupMap: (Int, RectF) -> Unit,
) {
    internal data class Pose(val box: RectF, val radius: Float, val crop: RectF,
        val image: Int = 0, val naturalW: Float = 0f, val naturalH: Float = 0f,
        val unit: RectF = RectF(0f, 0f, 1f, 1f))

    private class Element(val key: Long) {
        val content = RenderNode("exact shared content").apply { setClipToBounds(false) }
        val slot = RenderNode("exact shared slot").apply { setClipToBounds(false) }
        var box = RectF()
        var crop = RectF()
        var contentBox = RectF()
        var unit = RectF(0f, 0f, 1f, 1f)
        var radius = 0f
        var opacity = 1f
        var image = 0
        var fit = 0
        var naturalW = 0f
        var naturalH = 0f
        var originX = 0f
        var originY = 0f
        var group = 0
        var row = 0
        var order = 0
        var layers = IntArray(0)
        var parents = LongArray(0)
    }
    private class Flight(val from: Pose, var progress: Float) { var shown: Pose = from }
    private val elements = LinkedHashMap<Long, Element>()
    private val flights = LinkedHashMap<Long, Flight>()
    private val alive = HashSet<Long>()
    private val heldFrees = LinkedHashSet<Int>()
    private val rows = HashMap<Int, Pair<Float, Float>>()
    private var reduced = false
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val clip = Path()

    fun node(key: Long): RenderNode = elements.getOrPut(key) { Element(key) }.slot
    fun nodes(): Map<Long, RenderNode> = elements.mapValues { it.value.slot }

    /** Op 49 must precede IMAGE_FREE and every RenderNode mutation. Capture all sources
     * before hiding any destination, including an interrupted parent/child pair. */
    fun begin(b: IntBuffer) {
        if (b.limit() < 4 || b.get(0) != 49) return
        reduced = b.get(2) != 0
        val next = LinkedHashMap<Long, Flight>()
        repeat(b.get(3)) { n ->
            val at = 4 + n * 5
            val from = key(b, at); val to = key(b, at + 2); val progress = f(b, at + 4)
            val flight = flights[to] ?: capture(from)?.let { Flight(it, progress) }
            if (flight != null) { flight.progress = progress; next[to] = flight }
        }
        flights.clear(); flights.putAll(next)
        val live = 4 + b.get(3) * 5
        alive.clear()
        repeat(b.get(live)) { alive.add(key(b, live + 1 + it * 2)) }
        for ((key, element) in elements) element.slot.alpha = if (!reduced && key in flights) 0f else 1f
    }

    private fun capture(key: Long): Pose? {
        flights[key]?.let { return it.shown.copy(box = RectF(it.shown.box), crop = RectF(it.shown.crop)) }
        val element = elements[key] ?: return null
        val pose = placed(element)
        // A child not flying by itself is displayed inside its flying parent's content.
        for (parent in element.parents.reversed()) {
            val flight = flights[parent] ?: continue
            val slot = elements[parent]?.let { placed(it).box } ?: continue
            val scale = flight.shown.box.width() / slot.width().coerceAtLeast(0.001f)
            fun map(r: RectF) = RectF(flight.shown.box.left + (r.left - slot.left) * scale,
                flight.shown.box.top + (r.top - slot.top) * scale,
                flight.shown.box.left + (r.right - slot.left) * scale,
                flight.shown.box.top + (r.bottom - slot.top) * scale)
            return pose.copy(box = map(pose.box), crop = map(pose.crop), radius = pose.radius * scale)
        }
        return pose
    }

    /** Op 50's body is ordinary drawing, including independently named child slots. */
    fun define(b: IntBuffer, at: Int, ingest: (Int, Int) -> Unit) {
        val h = at + 2
        val id = key(b, h)
        val e = elements.getOrPut(id) { Element(id) }
        e.order = b.get(h + 2); e.group = b.get(h + 3); e.row = b.get(h + 4)
        e.originX = f(b, h + 5); e.originY = f(b, h + 6)
        e.box = rect(b, h + 7); e.radius = f(b, h + 11)
        e.image = b.get(h + 12); e.crop = rect(b, h + 13)
        e.fit = b.get(h + 17); e.naturalW = b.get(h + 18).toFloat(); e.naturalH = b.get(h + 19).toFloat()
        e.unit = rect(b, h + 20); e.contentBox = rect(b, h + 24)
        e.opacity = f(b, h + 28)
        val count = b.get(h + 29)
        e.layers = IntArray(count) { b.get(h + 30 + it) }
        val parents = h + 30 + count
        e.parents = LongArray(b.get(parents)) { key(b, parents + 1 + it * 2) }
        val body = parents + 1 + 2 * e.parents.size
        val end = at + 2 + b.get(at + 1)
        ingest(body, end)
        val width = max(1, ceil(e.box.width().toDouble()).toInt())
        val height = max(1, ceil(e.box.height().toDouble()).toInt())
        e.content.setPosition(0, 0, width, height)
        val drawing = e.content.beginRecording()
        player.replayAt(drawing, b, body, end, e.box.left, e.box.top)
        e.content.endRecording()
        e.slot.setPosition(0, 0, width, height)
        val slot = e.slot.beginRecording()
        slot.translate(e.box.left, e.box.top)
        slot.drawRenderNode(e.content)
        e.slot.endRecording()
        e.slot.alpha = if (!reduced && id in flights) 0f else 1f
    }

    fun finish() {
        val dead = elements.keys.filter { it !in alive }
        for (key in dead) elements.remove(key)?.let { it.slot.discardDisplayList(); it.content.discardDisplayList() }
        flights.keys.retainAll(alive)
        rows.keys.retainAll(elements.values.map { it.row }.toSet())
    }

    fun placeRow(id: Int, x: Float, y: Float) {
        if (elements.isNotEmpty()) rows[id] = Pair(x, y)
    }

    /** A freed source raster remains usable until the arriving raster replaces it. */
    fun holdImage(id: Int): Boolean {
        if (flights.none { (key, flight) -> flight.from.image == id && (elements[key]?.image ?: 0) == 0 }) return false
        heldFrees.add(id)
        return true
    }
    fun releaseImages(release: (Int) -> Unit) {
        val keep = flights.mapNotNull { (key, flight) ->
            if ((elements[key]?.image ?: 0) == 0) flight.from.image else null
        }.toSet()
        for (id in heldFrees.toList()) if (id !in keep) { heldFrees.remove(id); release(id) }
    }

    private fun map(e: Element, r: RectF): RectF {
        val out = RectF(r)
        for (id in e.layers.reversed()) layerMap(id, out)
        val row = rows[e.row]
        if (row != null) out.offset(row.first, row.second) else out.offset(e.originX, e.originY)
        if (e.group != 0) groupMap(e.group, out)
        return out
    }

    private fun placed(e: Element): Pose {
        val box = map(e, e.box)
        val scale = box.width() / e.box.width().coerceAtLeast(0.001f)
        return Pose(box, e.radius * scale, map(e, e.crop), e.image, e.naturalW, e.naturalH, e.unit)
    }

    private fun target(e: Element, from: Pose): Pose {
        val placed = placed(e)
        if (e.fit == 0 || e.image != 0) return placed
        val content = map(e, e.contentBox)
        val crop = fitted(from.naturalW, from.naturalH, e.fit, content)
        return placed.copy(crop = crop, image = from.image, naturalW = from.naturalW,
            naturalH = from.naturalH, unit = from.unit)
    }

    fun draw(canvas: Canvas) {
        if (reduced) return
        val ordered = flights.entries.sortedBy { elements[it.key]?.order ?: 0 }
        for ((key, flight) in ordered) {
            val element = elements[key] ?: continue
            val to = target(element, flight.from)
            val pose = interpolate(flight.from, to, flight.progress)
            flight.shown = pose
            if (pose.box.width() <= 0f || pose.box.height() <= 0f) continue
            val save = canvas.save()
            canvas.setMatrix(Matrix())
            clip.reset()
            val radius = pose.radius.coerceIn(0f, min(pose.box.width(), pose.box.height()) / 2f)
            clip.addRoundRect(pose.box, radius, radius, Path.Direction.CW)
            canvas.clipPath(clip)
            if (element.fit != 0) {
                paint.alpha = (element.opacity.coerceIn(0f, 1f) * 255f + 0.5f).toInt()
                pictures(pose.image)?.let { canvas.drawBitmap(it, null, rasterRect(pose), paint) }
            } else {
                canvas.translate(pose.box.left, pose.box.top)
                val scale = pose.box.width() / element.box.width().coerceAtLeast(0.001f)
                canvas.scale(scale, scale)
                canvas.drawRenderNode(element.content)
            }
            canvas.restoreToCount(save)
        }
    }

    internal fun shown(key: Long): Pose? = flights[key]?.shown
    internal fun isFlying(key: Long) = !reduced && key in flights

    companion object {
        fun key(b: IntBuffer, at: Int): Long = (b.get(at).toLong() and 0xffffffffL) or (b.get(at + 1).toLong() shl 32)
        private fun f(b: IntBuffer, at: Int) = Float.fromBits(b.get(at))
        private fun rect(b: IntBuffer, at: Int): RectF = RectF(f(b, at), f(b, at + 1), f(b, at) + f(b, at + 2), f(b, at + 1) + f(b, at + 3))
        private fun mix(a: Float, b: Float, p: Float) = a + (b - a) * p
        internal fun interpolate(from: Pose, to: Pose, p: Float): Pose {
            val box = RectF(mix(from.box.left, to.box.left, p), mix(from.box.top, to.box.top, p),
                mix(from.box.right, to.box.right, p), mix(from.box.bottom, to.box.bottom, p))
            // Crop offsets and sizes are interpolated in points, independently of the box.
            val crop = RectF(box.left + mix(from.crop.left - from.box.left, to.crop.left - to.box.left, p),
                box.top + mix(from.crop.top - from.box.top, to.crop.top - to.box.top, p), 0f, 0f)
            crop.right = crop.left + mix(from.crop.width(), to.crop.width(), p)
            crop.bottom = crop.top + mix(from.crop.height(), to.crop.height(), p)
            return Pose(box, mix(from.radius, to.radius, p), crop, to.image,
                to.naturalW, to.naturalH, to.unit)
        }
        internal fun rasterRect(pose: Pose): RectF = RectF(
            pose.crop.left + pose.unit.left * pose.crop.width(),
            pose.crop.top + pose.unit.top * pose.crop.height(),
            pose.crop.left + pose.unit.right * pose.crop.width(),
            pose.crop.top + pose.unit.bottom * pose.crop.height())

        internal fun fitted(w: Float, h: Float, fit: Int, box: RectF): RectF {
            if (w <= 0f || h <= 0f) return RectF(box)
            val sx = box.width() / w; val sy = box.height() / h
            val scale = when (fit) { 2 -> min(sx, sy); 3 -> max(sx, sy); 4 -> 1f; 5 -> min(1f, min(sx, sy)); else -> 0f }
            val width = if (fit == 1) box.width() else w * scale
            val height = if (fit == 1) box.height() else h * scale
            val left = box.centerX() - width / 2f; val top = box.centerY() - height / 2f
            return RectF(left, top, left + width, top + height)
        }
    }
}
