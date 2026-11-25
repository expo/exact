package dev.exact.canvas

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.HardwareRenderer
import android.graphics.RectF
import android.graphics.RenderNode
import java.nio.IntBuffer
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PlayerTest {
    private fun f(value: Int) = value.toFloat().toRawBits()
    private val identity = intArrayOf(1, f(1), 0, 0, f(1), 0, 0)
    private fun rect(y: Int) = intArrayOf(0, f(y), f(200), f(40)) + IntArray(8)
    private fun blur(y: Int) = intArrayOf(42, 1, 0, f(8)) + rect(y)

    @Test fun pixelatedAnimationsRemainLiveHardwareDraws() {
        val recorded = ArrayList<Pair<Boolean, Int>>()
        val drawable = object : android.graphics.drawable.Drawable() {
            override fun getIntrinsicWidth() = 2
            override fun getIntrinsicHeight() = 1
            override fun draw(canvas: Canvas) { recorded.add(canvas.isHardwareAccelerated to bounds.width()) }
            override fun setAlpha(alpha: Int) {}
            override fun setColorFilter(filter: android.graphics.ColorFilter?) {}
            @Deprecated("Drawable requires it")
            override fun getOpacity() = android.graphics.PixelFormat.TRANSLUCENT
        }
        val player = Player(emptyMap()).apply {
            width = 8; height = 4
            animated = { drawable }
        }
        val page = RenderNode("page").apply { setPosition(0, 0, 8, 4) }
        for (width in listOf(4, 5)) {
            recorded.clear()
            val ops = identity + intArrayOf(49, 2, 8, 1, 0, 0, f(width), f(2))
            player.replay(page.beginRecording(), IntBuffer.wrap(ops), 0, ops.size)
            page.endRecording()
            assertEquals(listOf(true to if (width == 4) 4 else 6), recorded)
            assertTrue(page.hasDisplayList())
        }
    }

    private fun stream(backdrops: List<IntArray>): IntBuffer {
        val words = ArrayList<Int>()
        words.addAll(identity.toList())
        words.addAll((intArrayOf(4, 0, 0, f(200), f(200)) + IntArray(8)).toList())
        words.addAll(listOf(20, 7))
        for (row in 0..2) {
            // A definition and a row id both contain 21, the GROUP_END opcode.
            val body = intArrayOf(2, 21, 0, 0, f(200), f(80)) + IntArray(8) + intArrayOf(16)
            words.addAll((intArrayOf(15, 21 + row, 0, 200, 80, body.size) + body).toList())
            words.addAll(listOf(17, 21 + row, 0, f(row * 80)))
        }
        words.addAll(listOf(21, 6))
        for (backdrop in backdrops) words.addAll((identity + backdrop).toList())
        return IntBuffer.wrap(words.toIntArray())
    }

    @Test fun backdropsReuseScrollerRowsOnEveryRecording() {
        val cases = listOf(
            listOf(blur(160)),
            listOf(intArrayOf(27, f(8)) + rect(160)),
            listOf(intArrayOf(42, 1, 1, 0) + rect(0)),
            listOf(blur(160), blur(40)),
        )
        for ((case, backdrops) in cases.withIndex()) for (withHolder in listOf(false, true)) {
            val scroller = RenderNode("scroller").apply { setClipToBounds(false) }
            val stretch = RenderNode("stretch").apply {
                setPosition(0, 0, 200, 200)
                beginRecording().drawRenderNode(scroller)
                endRecording()
            }
            val rows = listOf(Color.RED, Color.GREEN, Color.BLUE).map { color ->
                RenderNode("row").apply {
                    setPosition(0, 0, 200, 80)
                    beginRecording().drawColor(color)
                    endRecording()
                }
            }
            val reads = ArrayList<Int>()
            val holders = ArrayList<Int>()
            val player = Player(emptyMap()).apply {
                width = 200; height = 200
                group = { id -> assertEquals(7, id); scroller }
                row = { id -> reads.add(id); rows[id - 21] }
                holder = { id -> holders.add(id); if (withHolder) stretch else null }
            }
            val ops = stream(backdrops)
            val page = RenderNode("page").apply { setPosition(0, 0, 200, 200) }
            repeat(2) { frame ->
                reads.clear(); holders.clear()
                player.replay(page.beginRecording(), ops, 0, ops.limit())
                page.endRecording()
                val label = "case $case, holder $withHolder, frame $frame"
                assertEquals("$label: only the page records rows", listOf(21, 22, 23), reads)
                // Two filters also replay the first filter inside the second.
                val draws = if (backdrops.size == 2) 4 else 2
                assertEquals("$label: each replay draws the group or its holder", List(draws) { 7 }, holders)
                assertTrue("$label: the scroller remains recorded", scroller.hasDisplayList())
            }
        }
    }

    /** A shader and the shape it paints (op 43 before op 2) are one draw: a played gradient, a press's ripple, is
     * scaled as it is drawn, with no offscreen layer. */
    @Test fun aShadedShapeIsOneShape() {
        val shaded = identity + intArrayOf(43, 0, 2, Color.BLUE) + rect(0)
        val two = shaded + intArrayOf(2, Color.BLUE) + rect(40)
        val player = Player(emptyMap())
        assertTrue(player.singleShape(IntBuffer.wrap(shaded), 0, shaded.size))
        assertFalse(player.singleShape(IntBuffer.wrap(two), 0, two.size))
    }

    /** A layer whose drawing the reader cannot bound (a picture's slot, a scroller's rows) is the window's size, not
     * the view's clip: an edge box's background in it reaches past the view into the bars' bands. */
    @Test fun anUnboundedLayerReachesTheWindow() {
        val fill = intArrayOf(2, Color.BLUE, 0, f(-20), f(80), f(40)) + IntArray(8)
        val ops = identity + intArrayOf(7, 0.5f.toRawBits(), 23, 1) + fill + fill + intArrayOf(6)
        var bounds: RectF? = null
        val canvas = object : Canvas(Bitmap.createBitmap(80, 80, Bitmap.Config.ARGB_8888)) {
            override fun saveLayerAlpha(rect: RectF?, alpha: Int): Int {
                bounds = rect?.let(::RectF)
                return super.saveLayerAlpha(rect, alpha)
            }
        }
        Player(emptyMap()).apply { width = 80; height = 80; window.set(0f, -20f, 80f, 100f) }
            .replay(canvas, IntBuffer.wrap(ops), 0, ops.size)
        assertEquals(RectF(-1f, -21f, 81f, 101f), bounds)
    }

    /** SVG's `stroke-miterlimit`, op 13's word after the join: a 28° corner, whose miter is 4.1 widths long, is
     * bevelled at the initial limit of 4 and mitered at 10, as the other painters draw it. */
    @Test fun aStrokeTakesItsMiterLimit() {
        fun tip(limit: Float): Int {
            val corner = intArrayOf(3, 0, f(10), f(50), 1, f(50), f(40), 1, f(10), f(30))
            val ops = identity + intArrayOf(13, Color.BLUE, f(8), 0, 0, limit.toRawBits()) + corner
            val bitmap = Bitmap.createBitmap(80, 80, Bitmap.Config.ARGB_8888)
            Player(emptyMap()).apply { width = 80; height = 80 }
                .replay(Canvas(bitmap), IntBuffer.wrap(ops), 0, ops.size)
            return bitmap.getPixel(60, 40)
        }
        assertEquals(0, tip(4f))
        assertEquals(Color.BLUE, tip(10f))
    }

    @Test fun collectingAHostedViewsClipPreservesThePaint() {
        val node = RenderNode("module").apply { setPosition(0, 0, 60, 60) }
        val clip = intArrayOf(4, f(10), f(20), f(40), f(30)) + IntArray(8)
        val draw = intArrayOf(28, 1, 2, 0, 0, f(60), f(60)) + IntArray(9)
        val stream = identity + clip + draw + intArrayOf(6)
        fun paint(collect: Boolean): Bitmap {
            val graph = PaintedOrigins().apply { hasHostedViews = collect }
            val bitmap = Bitmap.createBitmap(80, 80, Bitmap.Config.ARGB_8888)
            val canvas = object : Canvas(bitmap) {
                override fun drawRenderNode(node: RenderNode) { drawColor(Color.BLUE) }
            }
            Player(emptyMap()).apply { origins = graph; native = { node } }
                .replay(canvas, IntBuffer.wrap(stream), 0, stream.size)
            if (collect) {
                val visible = RectF(0f, 0f, 60f, 60f)
                assertTrue(graph.transform(node, android.graphics.Matrix(), visible))
                assertEquals(RectF(10f, 20f, 50f, 50f), visible)
            }
            return bitmap
        }
        val without = paint(false)
        assertEquals(Color.BLUE, without.getPixel(10, 20))
        assertEquals(Color.TRANSPARENT, without.getPixel(9, 20))
        assertTrue(without.sameAs(paint(true)))
    }

    @Test fun anIframeKeepsItsFilterOriginAndItsOwnPaintClip() {
        val graph = PaintedOrigins().apply { tracksFrames = true }
        val frame = RenderNode("frame").apply { setPosition(0, 0, 200, 100) }
        val root = RenderNode("root")
        val player = Player(emptyMap()).apply { origins = graph; native = { frame } }
        val blur = intArrayOf(53, 8, 0, f(200), f(300), f(180), 1, 0, f(4), f(4))
        val half = 0.5f.toRawBits()
        val transform = intArrayOf(1, half, 0, 0, half, 0, f(225))
        val draw = intArrayOf(28, 1, 1, 0, 0, f(200), f(100)) + IntArray(9)
        val ops = identity + blur + transform + draw + intArrayOf(54)
        player.replay(root.beginRecording(400, 600), IntBuffer.wrap(ops), 0, ops.size)
        root.endRecording()
        val matrix = android.graphics.Matrix()
        val clip = RectF(0f, 0f, 200f, 100f)
        assertTrue(graph.transform(frame, matrix, clip))
        assertEquals(RectF(0f, 225f, 100f, 275f), clip)
    }

    /** A box's CSS filter (ops 53 and 54): its drawing goes into a node at the region's device bounds, which carries
     * the chain as a RenderEffect, and the walkers step over both ops. */
    @Test fun aFilterDrawsItsBoxIntoAnEffectNode() {
        val unchanged = IntArray(20).also { for (k in 0 until 4) it[k * 6] = f(1) }
        val ops = identity + intArrayOf(53, 26, f(10), f(10), f(40), f(30), 1, 1) + unchanged +
            intArrayOf(2, Color.BLUE, f(20), f(20), f(10), f(10)) + IntArray(8) + intArrayOf(54)
        val buffer = IntBuffer.wrap(ops)
        var i = 0
        val walker = Structure()
        while (i < ops.size) i = walker.end(buffer, i)
        assertEquals(ops.size, i)
        val drawn = ArrayList<RenderNode>()
        val canvas = object : Canvas(Bitmap.createBitmap(80, 80, Bitmap.Config.ARGB_8888)) {
            override fun drawRenderNode(node: RenderNode) { drawn.add(node) }
        }
        Player(emptyMap()).apply { width = 80; height = 80 }.replay(canvas, buffer, 0, ops.size)
        val node = drawn.single()
        assertEquals(listOf(10, 10, 50, 40), listOf(node.left, node.top, node.right, node.bottom))
        assertTrue(node.hasDisplayList())
    }

    /** A blue 20 × 20 square at (30, 30) on white, through [count] functions [fn] over the region (10, 10, 60, 60). */
    private fun filtered(count: Int, fn: IntArray): Bitmap {
        val ops = identity + intArrayOf(53, 5 + fn.size, f(10), f(10), f(60), f(60), count) + fn +
            intArrayOf(2, Color.BLUE, f(30), f(30), f(20), f(20)) + IntArray(8) + intArrayOf(54)
        val page = RenderNode("page").apply { setPosition(0, 0, 80, 80) }
        val canvas = page.beginRecording()
        canvas.drawColor(Color.WHITE)
        Player(emptyMap()).apply { width = 80; height = 80 }.replay(canvas, IntBuffer.wrap(ops), 0, ops.size)
        page.endRecording()
        return HardwareRenderer::class.java.getDeclaredMethod("createHardwareBitmap",
            RenderNode::class.java, Int::class.javaPrimitiveType, Int::class.javaPrimitiveType)
            .invoke(null, page, 80, 80) as Bitmap
    }

    /** The chain draws as the painter's: a blur spreads the box past its edge, a colour matrix scales its channels,
     * and a drop shadow goes under it, moved. */
    @Test fun aFilterDrawsItsChain() {
        val blurred = filtered(1, intArrayOf(0, f(4), f(4)))
        assertTrue("blurred past the edge", Color.red(blurred.getPixel(27, 40)) in 1..254)
        assertTrue("blue within", Color.red(blurred.getPixel(40, 40)) < 64)
        val half = IntArray(20).also { for (k in 0 until 3) it[k * 6] = 0.5f.toRawBits(); it[18] = f(1) }
        val dim = filtered(1, intArrayOf(1) + half)
        assertEquals("brightness(0.5)", 128f, Color.blue(dim.getPixel(40, 40)).toFloat(), 2f)
        assertEquals("white outside the box", Color.WHITE, dim.getPixel(20, 40))
        val shadow = filtered(1, intArrayOf(2, f(1), f(10), 0, Color.BLACK))
        assertTrue("the box above its shadow", Color.blue(shadow.getPixel(45, 40)) > 200)
        assertTrue("the shadow beside it", Color.blue(shadow.getPixel(55, 40)) < 80)
    }

    /** Op 55, a kept row through a matrix: the reader draws the row's node, and the node's box through the matrix is
     * what the op covers. */
    @Test fun aKeptRowIsDrawnThroughItsMatrix() {
        val node = RenderNode("row").apply {
            setPosition(0, 0, 100, 50)
            beginRecording().drawColor(Color.RED)
            endRecording()
        }
        val through = floatArrayOf(0.5f, 0f, 0f, 0.5f, 10f, 20f).map { it.toRawBits() }
        val ops = identity + intArrayOf(55, 9) + through
        val buffer = IntBuffer.wrap(ops)
        assertEquals(ops.size, Structure().end(buffer, identity.size))
        val reads = ArrayList<Int>()
        val player = Player(emptyMap()).apply {
            width = 200; height = 200
            row = { id -> reads.add(id); node }
        }
        val page = RenderNode("page").apply { setPosition(0, 0, 200, 200) }
        player.replay(page.beginRecording(), buffer, 0, ops.size)
        page.endRecording()
        assertEquals(listOf(9), reads)
        val bounds = RectF()
        assertTrue(Bounds().apply { row = { node } }.measure(buffer, 0, ops.size, bounds))
        // The node's 100×50 at half size from (10, 20), not its unscaled box.
        assertTrue("$bounds", bounds.contains(RectF(10f, 20f, 60f, 45f)) && bounds.right < 70f && bounds.bottom < 50f)
    }

    /** A paragraph's lines in one style are one text blob; a run in another colour is its own. */
    @Test fun runsInOneStyleAreDrawnTogether() {
        val font = android.graphics.fonts.Font.Builder(java.io.File("../../../apps/canvas-gallery/assets/ExposureSansOriginal03-Regular.ttf")).build()
        val calls = ArrayList<Int>()
        val canvas = object : Canvas(Bitmap.createBitmap(80, 80, Bitmap.Config.ARGB_8888)) {
            override fun drawGlyphs(ids: IntArray, idOffset: Int, positions: FloatArray, posOffset: Int, count: Int,
                font: android.graphics.fonts.Font, paint: android.graphics.Paint) { calls.add(count) }
        }
        fun run(color: Int, y: Int) = intArrayOf(9, 1, f(12), color, 0, 2, 36, f(0), f(y), 37, f(8), f(y))
        val ops = identity + run(Color.BLACK, 12) + run(Color.BLACK, 28) + run(Color.BLUE, 44)
        Player(mapOf(1 to font)).apply { width = 80; height = 80 }.replay(canvas, IntBuffer.wrap(ops), 0, ops.size)
        assertEquals(listOf(4, 2), calls)
    }

    @Implements(LegacyGlyphs::class, isInAndroidSdk = false)
    class GlyphOutlines {
        companion object {
            @JvmStatic @Implementation fun glyph(font: java.nio.ByteBuffer, index: Int, axes: IntArray, id: Int) =
                intArrayOf(1, 4, 0, 0, 0, 1, 1f.toBits(), 0, 1, 0, 1f.toBits(), 4)
        }
    }

    @Test @Config(sdk = [30], shadows = [GlyphOutlines::class])
    fun overlappingLegacyRunsCompositeSeparately() {
        val font = android.graphics.fonts.Font.Builder(java.io.File("../../../apps/canvas-gallery/assets/ExposureSansOriginal03-Regular.ttf")).build()
        val run = intArrayOf(9, 1, f(12), 0x80000000.toInt(), 0, 1, 36, f(5), f(5))
        val ops = identity + run + run
        val bitmap = Bitmap.createBitmap(24, 24, Bitmap.Config.ARGB_8888)
        Player(mapOf(1 to font)).apply { width = 24; height = 24 }.replay(Canvas(bitmap), IntBuffer.wrap(ops), 0, ops.size)
        assertEquals(192, Color.alpha(bitmap.getPixel(7, 7)))
    }
}
