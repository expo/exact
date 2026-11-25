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
    private val layerOpacity: (Int) -> Float,
    private val groupMap: (Int, RectF) -> Unit,
) {
    internal data class Pose(val box: RectF, val radius: Float, val crop: RectF,
        val image: Int = 0, val naturalW: Float = 0f, val naturalH: Float = 0f,
        val unit: RectF = RectF(0f, 0f, 1f, 1f), val contentBox: RectF = box)

    private class Element(val key: Long) {
        val content = RenderNode("exact shared content").apply { setClipToBounds(false) }
        val slot = RenderNode("exact shared slot").apply { setClipToBounds(false) }
        var box = RectF()
        var crop = RectF()
        var contentBox = RectF()
        var unit = RectF(0f, 0f, 1f, 1f)
        var radius = 0f
        var opacity = 1f
        /** Device pixels a point covers where this definition was recorded. */
        var scale = 1f
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
    /** A [route] change's flight dissolves from the source's own drawing, [content] in [box] at [contentBox]. */
    private class Flight(val source: Long, val from: Pose, var progress: Float, var ancestorOpacity: Float,
        val route: Boolean, val content: RenderNode?, val box: RectF?, val contentBox: RectF?) {
        var shown: Pose = from
        /** Takes a route crossing's pose off the destination: rest = posed × [0] + ([1], [2]). */
        var unpose = floatArrayOf(1f, 0f, 0f)
    }
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

    /** Op 50 must precede IMAGE_FREE and every RenderNode mutation. Capture all sources
     * before hiding any destination, including an interrupted parent/child pair. */
    fun begin(b: IntBuffer) {
        if (b.limit() < 4 || b.get(0) != 50) return
        reduced = b.get(2) != 0
        val next = LinkedHashMap<Long, Flight>()
        repeat(b.get(3)) { n ->
            val at = 4 + n * 10
            val from = key(b, at); val to = key(b, at + 2); val progress = f(b, at + 4)
            val opacity = f(b, at + 5)
            // One from another source has yielded (an in-page handoff to a name a route change pairs).
            val flight = flights[to]?.takeIf { it.source == from } ?: capture(from)?.let {
                val route = b.get(at + 9) != 0
                val source = elements[from]?.takeIf { route }
                Flight(from, it, progress, opacity, route, source?.content, source?.box?.let(::RectF),
                    source?.contentBox?.let(::RectF))
            }
            if (flight != null) {
                flight.progress = progress; flight.ancestorOpacity = opacity; next[to] = flight
                flight.unpose = floatArrayOf(f(b, at + 6), f(b, at + 7), f(b, at + 8))
            }
        }
        // A dissolve's source drawing outlives its element only while its flight does.
        val held = next.values.mapNotNullTo(HashSet()) { it.content } + elements.values.map { it.content }
        for (gone in flights.values) gone.content?.takeIf { it !in held }?.discardDisplayList()
        // Landed: its content is drawn in its slot again, as itself.
        for (key in flights.keys - next.keys) elements[key]?.content?.setUseCompositingLayer(false, null)
        flights.clear(); flights.putAll(next)
        val live = 4 + b.get(3) * 10
        alive.clear()
        repeat(b.get(live)) { order ->
            val key = key(b, live + 1 + order * 2)
            alive.add(key)
            elements[key]?.order = order
        }
        for ((key, element) in elements) element.slot.alpha = if (flying(key)) 0f else 1f
    }

    /** Both ends of a flight hide: a source a route change leaves mounted too (LLP 1013.004 D4). */
    private fun flying(key: Long) = !reduced && (key in flights || flights.values.any { it.source == key })

    private fun capture(key: Long): Pose? {
        flights[key]?.let {
            val shown = it.shown
            return shown.copy(box = RectF(shown.box), crop = RectF(shown.crop), contentBox = RectF(shown.contentBox))
        }
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
            return pose.copy(box = map(pose.box), crop = map(pose.crop), contentBox = map(pose.contentBox),
                radius = pose.radius * scale)
        }
        return pose
    }

    /** Op 51's body is ordinary drawing, including independently named child slots. */
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
        e.opacity = f(b, h + 28); e.scale = f(b, h + 29)
        val count = b.get(h + 30)
        e.layers = IntArray(count) { b.get(h + 31 + it) }
        val parents = h + 31 + count
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
        e.slot.alpha = if (flying(id)) 0f else 1f
    }

    fun finish() {
        val dead = elements.keys.filter { it !in alive }
        flights.keys.retainAll(alive)
        val dissolving = flights.values.mapNotNull { it.content }.toSet()
        for (key in dead) elements.remove(key)?.let {
            it.slot.discardDisplayList()
            if (it.content !in dissolving) it.content.discardDisplayList()
        }
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
        return Pose(box, e.radius * scale, map(e, e.crop), e.image, e.naturalW, e.naturalH, e.unit, map(e, e.contentBox))
    }

    private fun target(e: Element, from: Pose): Pose {
        val placed = placed(e)
        if (e.fit !in IMAGE || e.image != 0) return placed
        return placed.copy(image = from.image, naturalW = from.naturalW,
            naturalH = from.naturalH, unit = from.unit)
    }

    private fun unposed(pose: Pose, u: FloatArray): Pose {
        fun map(r: RectF) = RectF(r.left * u[0] + u[1], r.top * u[0] + u[2], r.right * u[0] + u[1], r.bottom * u[0] + u[2])
        return pose.copy(box = map(pose.box), radius = pose.radius * u[0], crop = map(pose.crop), contentBox = map(pose.contentBox))
    }

    private fun ancestorOpacity(e: Element, flight: Flight): Float {
        var opacity = flight.ancestorOpacity
        for (id in e.layers) opacity *= layerOpacity(id)
        return opacity.coerceIn(0f, 1f)
    }

    fun draw(canvas: Canvas) {
        if (reduced) return
        val ordered = flights.entries.sortedBy { elements[it.key]?.order ?: 0 }
        for ((key, flight) in ordered) {
            val element = elements[key] ?: continue
            val to = unposed(target(element, flight.from), flight.unpose)
            val pose = interpolate(flight.from, to, flight.progress)
            flight.shown = pose
            if (pose.box.width() <= 0f || pose.box.height() <= 0f) continue
            val save = canvas.save()
            canvas.setMatrix(Matrix())
            // A child flies inside its flying parent, as Compose clips a shared
            // element to its parent's bounds in the overlay.
            for (parent in element.parents) flights[parent]?.shown?.let { around ->
                clip.reset()
                val r = around.radius.coerceIn(0f, min(around.box.width(), around.box.height()) / 2f)
                clip.addRoundRect(around.box, r, r, Path.Direction.CW)
                canvas.clipPath(clip)
            }
            // A dissolve's box ends where its contents do, each scaled by width: the source's
            // height at first, the destination's once it has dissolved, as Material's container
            // transform masks its height, so neither shows past the other.
            val t = fade(flight.progress)
            val shown = RectF(pose.box)
            if (flight.content != null && element.fit != LABEL) {
                val w = pose.box.width()
                val from = flight.box!!.height() * w / flight.box.width().coerceAtLeast(0.001f)
                val to = element.box.height() * w / element.box.width().coerceAtLeast(0.001f)
                shown.bottom = min(shown.bottom, shown.top + from + (to - from) * t)
            }
            clip.reset()
            val radius = pose.radius.coerceIn(0f, min(shown.width(), shown.height()) / 2f)
            clip.addRoundRect(shown, radius, radius, Path.Direction.CW)
            canvas.clipPath(clip)
            val opacity = ancestorOpacity(element, flight)
            if (element.fit in IMAGE) {
                canvas.clipRect(pose.contentBox)
                paint.alpha = (element.opacity.coerceIn(0f, 1f) * opacity * 255f + 0.5f).toInt()
                pictures(pose.image)?.let { canvas.drawBitmap(it, null, rasterRect(pose), paint) }
            } else {
                // The content already includes its own and its descendants' opacity.
                if (opacity < 1f) canvas.saveLayerAlpha(pose.box, (opacity * 255f + 0.5f).toInt())
                canvas.translate(pose.box.left, pose.box.top)
                // A route change's source dissolves into the destination, as Compose's
                // sharedBounds fades (LLP 1013.004 D3 step 4), so neither end jumps: ahead of
                // the box ([fade]), each holding full strength half that time, so content
                // alike at both ends does not dim.
                // Bounded by the box: a null bound is the view's clip, which would cut off
                // what reaches into the bars' bands.
                val bounds = RectF(0f, 0f, pose.box.width(), pose.box.height())
                flight.content?.takeIf { t < 1f }?.let { content ->
                    canvas.saveLayerAlpha(bounds, (min(1f, 2f * (1f - t)) * 255f + 0.5f).toInt())
                    drawScaled(canvas, content, flight.box!!, flight.contentBox!!, pose, element.fit == LABEL)
                    canvas.restore()
                    canvas.saveLayerAlpha(bounds, (min(1f, 2f * t) * 255f + 0.5f).toInt())
                }
                drawScaled(canvas, element.content, element.box, element.contentBox, pose,
                    flight.route && element.fit == LABEL)
            }
            canvas.restoreToCount(save)
        }
    }

    /** A route flight scales a label by its lines' width, which its content box is, as Compose's
     * `scaleToBounds` scales a text by its own: its box is often as wide as its row at one end and its
     * text at the other, and its line height need not follow its font. Other content scales by width,
     * as Material's container transform fits it. The flight's box clips the rest. */
    private fun drawScaled(canvas: Canvas, content: RenderNode, box: RectF, contentBox: RectF, pose: Pose,
        label: Boolean) {
        // Scaled as a picture, as Compose's scaleToBounds scales a graphics layer: drawn scaled, its text
        // would be rasterized at a new size every frame of the flight.
        content.setUseCompositingLayer(true, null)
        val save = canvas.save()
        if (label) {
            val scale = pose.contentBox.width() / contentBox.width().coerceAtLeast(0.001f)
            // Centred on its lines, as `scaleToBounds` aligns it.
            canvas.translate(pose.contentBox.left - pose.box.left, pose.contentBox.centerY() - pose.box.top)
            canvas.scale(scale, scale)
            canvas.translate(box.left - contentBox.left, box.top - contentBox.centerY())
        } else {
            val scale = pose.box.width() / box.width().coerceAtLeast(0.001f)
            canvas.scale(scale, scale)
        }
        canvas.drawRenderNode(content)
        canvas.restoreToCount(save)
    }

    internal fun shown(key: Long): Pose? = flights[key]?.shown
    internal fun isFlying(key: Long) = !reduced && key in flights

    companion object {
        /** A route flight's crossfade at its bounds' [progress]: Compose fades a shared bounds' contents
         * on nonSpatialExpressiveSpring (stiffness 1600) while the bounds move on stiffness 400, both
         * critically damped, so the fade leads: halfway, it is 85 % done. */
        internal fun fade(progress: Float): Float {
            val spring = { x: Double -> 1.0 - (1.0 + x) * Math.exp(-x) }
            var lo = 0.0; var hi = 20.0
            repeat(32) { val x = (lo + hi) / 2; if (spring(x) < progress) lo = x else hi = x }
            return spring(lo + hi).toFloat()
        }

        /** An element's `fit`: a plain image's object-fit, or a label. */
        private val IMAGE = 1..5
        private const val LABEL = 6
        fun key(b: IntBuffer, at: Int): Long = (b.get(at).toLong() and 0xffffffffL) or (b.get(at + 1).toLong() shl 32)
        private fun f(b: IntBuffer, at: Int) = Float.fromBits(b.get(at))
        private fun rect(b: IntBuffer, at: Int): RectF = RectF(f(b, at), f(b, at + 1), f(b, at) + f(b, at + 2), f(b, at + 1) + f(b, at + 3))
        private fun mix(a: Float, b: Float, p: Float) = a + (b - a) * p
        internal fun interpolate(from: Pose, to: Pose, p: Float): Pose {
            fun rect(a: RectF, b: RectF) = RectF(mix(a.left, b.left, p), mix(a.top, b.top, p),
                mix(a.right, b.right, p), mix(a.bottom, b.bottom, p))
            return Pose(rect(from.box, to.box), mix(from.radius, to.radius, p), rect(from.crop, to.crop),
                to.image, to.naturalW, to.naturalH, to.unit, rect(from.contentBox, to.contentBox))
        }
        internal fun rasterRect(pose: Pose): RectF = RectF(
            pose.crop.left + pose.unit.left * pose.crop.width(),
            pose.crop.top + pose.unit.top * pose.crop.height(),
            pose.crop.left + pose.unit.right * pose.crop.width(),
            pose.crop.top + pose.unit.bottom * pose.crop.height())
    }
}
