package dev.exact.canvas

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Canvas
import android.graphics.RectF
import android.graphics.RenderNode
import java.nio.IntBuffer
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SharedElementsTest {
    private fun f(n: Float) = n.toRawBits()
    private fun key(k: Long) = intArrayOf(k.toInt(), (k ushr 32).toInt())
    private fun rect(r: RectF) = intArrayOf(f(r.left), f(r.top), f(r.width()), f(r.height()))
    private fun state(alive: List<Long>, pairs: List<Triple<Long, Long, Float>> = emptyList(), reduced: Boolean = false): IntBuffer {
        val body = arrayListOf(if (reduced) 1 else 0, pairs.size)
        for ((a, b, p) in pairs) body.addAll((key(a) + key(b) + f(p)).toList())
        body.add(alive.size)
        for (k in alive) body.addAll(key(k).toList())
        return IntBuffer.wrap(intArrayOf(49, body.size) + body.toIntArray())
    }
    private fun definition(k: Long, box: RectF, image: Int = 0, fit: Int = 0,
        crop: RectF = box, group: Int = 0, row: Int = 0, layers: IntArray = intArrayOf(),
        parents: List<Long> = emptyList(), content: IntArray = intArrayOf(), opacity: Float = 1f): IntBuffer {
        val head = key(k) + intArrayOf(k.toInt(), group, row, 0, 0) + rect(box) + f(12f) + image + rect(crop) +
            intArrayOf(fit, 400, 200) + rect(RectF(0f, 0f, 1f, 1f)) + rect(box) + f(opacity) + layers.size + layers + parents.size + parents.flatMap { key(it).toList() }.toIntArray()
        val paint = if (content.isNotEmpty()) content else intArrayOf(1, f(1f), 0, 0, f(1f), 0, 0, 2, Color.RED) + rect(box) + IntArray(8)
        return IntBuffer.wrap(intArrayOf(50, head.size + paint.size) + head + paint)
    }
    private class Fixture {
        val pictures = HashMap<Int, Bitmap>()
        var scroll = 0f
        var layerScale = 1f
        val player = Player(emptyMap())
        val shared = SharedElements(player, { pictures[it] }, { _, r ->
            r.set(r.left * layerScale, r.top * layerScale, r.right * layerScale, r.bottom * layerScale)
        }, { _, r -> r.offset(0f, scroll) })
        val page = RenderNode("test page").apply { setPosition(0, 0, 500, 800) }
        init { player.shared = { shared.node(it) } }
        fun define(b: IntBuffer) { shared.define(b, 0) { _, _ -> } }
        fun draw() { shared.draw(page.beginRecording()); page.endRecording() }
    }
    private fun assertRect(expected: RectF, actual: RectF) {
        assertEquals(expected.left, actual.left, 0.01f); assertEquals(expected.top, actual.top, 0.01f)
        assertEquals(expected.right, actual.right, 0.01f); assertEquals(expected.bottom, actual.bottom, 0.01f)
    }

    @Test fun capturesNativeScrollAndLayerBeforeTheyAreResetAndReversesFromShown() {
        val t = Fixture()
        val thumb = RectF(20f, 200f, 120f, 250f)
        val detail = RectF(0f, 0f, 400f, 300f)
        t.shared.begin(state(listOf(1)))
        t.define(definition(1, thumb, group = 7, row = 11, layers = intArrayOf(3)))
        t.shared.placeRow(11, 40f, 130f)
        t.shared.finish()
        t.layerScale = 1.5f; t.scroll = -80f
        t.shared.begin(state(listOf(2), listOf(Triple(1, 2, 0f))))
        t.layerScale = 1f; t.scroll = 0f
        t.define(definition(2, detail)); t.shared.finish(); t.draw()
        assertRect(RectF(70f, 350f, 220f, 425f), t.shared.shown(2)!!.box)
        assertEquals(0f, t.shared.node(2).alpha, 0f)
        t.shared.begin(state(listOf(2), listOf(Triple(1, 2, 0.4f))))
        t.define(definition(2, detail)); t.shared.finish(); t.draw()
        val mid = RectF(t.shared.shown(2)!!.box)
        t.shared.begin(state(listOf(3), listOf(Triple(2, 3, 0f))))
        t.define(definition(3, thumb)); t.shared.finish(); t.draw()
        assertRect(mid, t.shared.shown(3)!!.box)
        t.shared.begin(state(listOf(3))); t.shared.finish()
        assertFalse(t.shared.isFlying(3)); assertEquals(1f, t.shared.node(3).alpha, 0f)
    }

    @Test fun pendingRasterKeepsPhotoAndCropMorphsInPoints() {
        val t = Fixture()
        t.pictures[7] = Bitmap.createBitmap(400, 200, Bitmap.Config.ARGB_8888)
        val box = RectF(10f, 20f, 110f, 120f)
        val crop = RectF(-40f, 20f, 160f, 120f)
        t.shared.begin(state(listOf(1))); t.define(definition(1, box, 7, 3, crop)); t.shared.finish()
        t.shared.begin(state(listOf(2), listOf(Triple(1, 2, 0.5f))))
        assertTrue(t.shared.holdImage(7))
        t.define(definition(2, RectF(0f, 0f, 400f, 400f), fit = 2)); t.shared.finish(); t.draw()
        val shown = t.shared.shown(2)!!
        assertEquals(7, shown.image)
        assertRect(RectF(-20f, 60f, 280f, 210f), shown.crop)
        val freed = ArrayList<Int>(); t.shared.releaseImages { freed.add(it) }
        assertTrue(freed.isEmpty())
        t.pictures[8] = Bitmap.createBitmap(400, 200, Bitmap.Config.ARGB_8888)
        t.define(definition(2, RectF(0f, 0f, 400f, 400f), 8, 2, RectF(0f, 100f, 400f, 300f)))
        t.draw(); assertEquals(8, t.shared.shown(2)!!.image)
        t.shared.releaseImages { freed.add(it) }; assertEquals(listOf(7), freed)
    }

    @Test fun nestedSlotsFlyIndependentlyAndReducedMotionRestoresBoth() {
        val t = Fixture()
        t.shared.begin(state(listOf(1, 2)))
        t.define(definition(1, RectF(10f, 20f, 110f, 120f)))
        t.define(definition(2, RectF(20f, 30f, 80f, 50f), parents = listOf(1)))
        t.shared.finish()
        val pairs = listOf(Triple(1L, 3L, 0.5f), Triple(2L, 4L, 0.5f))
        t.shared.begin(state(listOf(3, 4), pairs))
        t.define(definition(4, RectF(40f, 80f, 280f, 120f), parents = listOf(3)))
        t.define(definition(3, RectF(0f, 60f, 400f, 500f), content = intArrayOf(51) + key(4)))
        t.shared.finish(); t.draw()
        assertEquals(0f, t.shared.node(3).alpha, 0f); assertEquals(0f, t.shared.node(4).alpha, 0f)
        assertRect(RectF(30f, 55f, 180f, 85f), t.shared.shown(4)!!.box)
        t.shared.begin(state(listOf(3, 4), pairs, reduced = true)); t.shared.finish()
        assertFalse(t.shared.isFlying(3)); assertFalse(t.shared.isFlying(4))
        assertEquals(1f, t.shared.node(3).alpha, 0f); assertEquals(1f, t.shared.node(4).alpha, 0f)
    }

    @Test fun aMovingTargetAndSpringOvershootAreNotClamped() {
        val from = SharedElements.Pose(RectF(0f, 0f, 100f, 80f), 10f, RectF(0f, 0f, 100f, 80f))
        val to = SharedElements.Pose(RectF(20f, 50f, 220f, 250f), 20f, RectF(20f, 50f, 220f, 250f))
        val over = SharedElements.interpolate(from, to, 1.1f)
        assertRect(RectF(22f, 55f, 232f, 267f), over.box)
        val t = Fixture()
        t.shared.begin(state(listOf(1))); t.define(definition(1, from.box)); t.shared.finish()
        t.shared.begin(state(listOf(2), listOf(Triple(1, 2, 0.5f))))
        t.define(definition(2, to.box, group = 7)); t.shared.finish(); t.draw()
        t.scroll = -40f; t.draw()
        assertEquals(5f, t.shared.shown(2)!!.box.top, 0.01f)
    }

    @Test fun liveImageOpacityChangesDuringTheFlight() {
        val t = Fixture()
        val photo = Bitmap.createBitmap(100, 100, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.RED) }
        t.pictures[7] = photo
        val box = RectF(0f, 0f, 100f, 100f)
        t.shared.begin(state(listOf(1)))
        t.define(definition(1, box, 7, 1, opacity = 0.5f)); t.shared.finish()
        t.shared.begin(state(listOf(2), listOf(Triple(1, 2, 0.5f))))
        val pixels = Bitmap.createBitmap(100, 100, Bitmap.Config.ARGB_8888)
        for ((opacity, alpha) in listOf(0.5f to 128, 0.25f to 64)) {
            t.define(definition(2, box, 7, 1, opacity = opacity)); t.shared.finish()
            pixels.eraseColor(Color.TRANSPARENT)
            t.shared.draw(Canvas(pixels))
            assertEquals(alpha, Color.alpha(pixels.getPixel(50, 50)))
        }
    }

    @Test fun differentDecodeCropsKeepTheNaturalImageMappingContinuous() {
        val from = SharedElements.Pose(RectF(10f, 20f, 110f, 120f), 12f,
            RectF(-40f, 20f, 160f, 120f), 7, 400f, 200f, RectF(0.25f, 0f, 0.75f, 1f))
        val to = SharedElements.Pose(RectF(0f, 0f, 400f, 400f), 0f,
            RectF(0f, 100f, 400f, 300f), 8, 400f, 200f)
        val first = SharedElements.interpolate(from, to, 0f)
        assertRect(from.crop, first.crop)
        assertRect(RectF(-40f, 20f, 160f, 120f), SharedElements.rasterRect(first))
        val pending = SharedElements.interpolate(from, to.copy(image = 7, unit = from.unit), 0.5f)
        val loaded = SharedElements.interpolate(from, to, 0.5f)
        assertRect(pending.crop, loaded.crop)
        // The same natural-image point stays in place when its decoded backing changes.
        val a = SharedElements.rasterRect(pending)
        val b = SharedElements.rasterRect(loaded)
        assertEquals(a.left, b.left + 0.25f * b.width(), 0.01f)
        assertEquals(a.right, b.left + 0.75f * b.width(), 0.01f)
    }
}
