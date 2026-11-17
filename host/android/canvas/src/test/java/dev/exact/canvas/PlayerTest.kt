package dev.exact.canvas

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.HardwareRenderer
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
}
