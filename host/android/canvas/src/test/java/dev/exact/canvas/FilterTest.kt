package dev.exact.canvas

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.HardwareRenderer
import android.graphics.RenderNode
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.IntBuffer
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** A box's CSS filter as HWUI draws it (ops 53 and 54, LLP 1172.023.001 D1), against the CPU chain's pixels. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class FilterTest {
    private fun f(value: Float) = value.toRawBits()
    private val identity = intArrayOf(1, f(1f), 0, 0, f(1f), 0, 0)
    private fun fill(color: Int, x: Float, y: Float, w: Float, h: Float) =
        intArrayOf(2, color, f(x), f(y), f(w), f(h)) + IntArray(8)

    /** [ops] replayed over a white page [w] × [h], drawn by HWUI. */
    private fun draw(ops: IntBuffer, w: Int, h: Int): Bitmap {
        val page = RenderNode("page").apply { setPosition(0, 0, w, h) }
        val canvas = page.beginRecording()
        canvas.drawColor(Color.WHITE)
        Player(emptyMap()).apply { width = w; height = h }.replay(canvas, ops, 0, ops.limit())
        page.endRecording()
        val drawn = HardwareRenderer::class.java.getDeclaredMethod("createHardwareBitmap",
            RenderNode::class.java, Int::class.javaPrimitiveType, Int::class.javaPrimitiveType)
            .invoke(null, page, w, h) as Bitmap
        return drawn.copy(Bitmap.Config.ARGB_8888, false)
    }

    /** Mean |Δ| per channel (of 255) over [x0, x1) × [y0, y1), and the pixels off by more than 32 in a channel. */
    private fun difference(a: Bitmap, b: Bitmap, x0: Int, y0: Int, x1: Int, y1: Int): Pair<Double, Int> {
        var total = 0L
        var off = 0
        for (y in y0 until y1) for (x in x0 until x1) {
            val p = a.getPixel(x, y); val q = b.getPixel(x, y)
            var worst = 0
            for (shift in listOf(0, 8, 16)) {
                val d = Math.abs(((p ushr shift) and 255) - ((q ushr shift) and 255))
                total += d; worst = Math.max(worst, d)
            }
            if (worst > 32) off++
        }
        return total.toDouble() / ((x1 - x0) * (y1 - y0) * 3) to off
    }

    /**
     * `canvas::filter::tests::the_reader_is_held_to_the_cpu_chain_on_this_stream` records `box-filter.ops` and
     * the CPU chain's pixels of the same page, `box-filter.png`: seven chains, each held to them here.
     */
    @Test fun chainsDrawAsTheCpuChainDoes() {
        val bytes = javaClass.getResourceAsStream("/box-filter.ops")!!.readBytes()
        val ops = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asIntBuffer()
        val reference = BitmapFactory.decodeStream(javaClass.getResourceAsStream("/box-filter.png"))
        val actual = draw(ops, 320, 240)
        val boxes = mapOf(
            "blur past the page's corner" to intArrayOf(0, 0, 80, 60),
            "invert sepia contrast brightness" to intArrayOf(20, 60, 68, 108),
            "drop shadow under hue-rotate" to intArrayOf(100, 60, 164, 124),
            "drop shadow and blur at opacity 0.5" to intArrayOf(180, 60, 244, 124),
            "drop shadow in a rounded clip" to intArrayOf(20, 140, 88, 208),
            "drop shadow under clip-path" to intArrayOf(120, 140, 184, 188),
            "grayscale brightness opacity" to intArrayOf(200, 140, 248, 188),
        )
        for ((what, r) in boxes) {
            val (mean, off) = difference(actual, reference, r[0], r[1], r[2], r[3])
            println("HWUI against the CPU chain, $what: mean |Δ| ${"%.2f".format(mean)}/255, $off pixels off by > 32")
            assertTrue("$what: mean |Δ| $mean/255", mean <= 4.0)
            assertTrue("$what: $off pixels off by > 32", off <= (r[2] - r[0]) * (r[3] - r[1]) / 50)
        }
        val (mean, _) = difference(actual, reference, 0, 0, 320, 240)
        println("HWUI against the CPU chain, the page: mean |Δ| ${"%.2f".format(mean)}/255")
    }

    /**
     * CSS reads nothing past a filter's input, and nothing is drawn past its region: a blur over content that meets
     * the region's edge fades to half there and stops. The JVM's HWUI draws the node through a layer as large as
     * the canvas, so this holds under `CLAMP` too: it does not tell the two edge modes apart.
     */
    @Test fun aBlurFadesAtItsRegionsEdge() {
        val blur = intArrayOf(53, 8, f(10f), f(10f), f(40f), f(40f), 1, 0, f(4f), f(4f))
        val ops = IntBuffer.wrap(identity + blur + fill(Color.RED, 10f, 10f, 40f, 40f) + intArrayOf(54))
        val drawn = draw(ops, 60, 60)
        val middle = drawn.getPixel(30, 30)
        assertTrue("the middle stays red: ${Integer.toHexString(middle)}", Color.red(middle) == 255 && Color.green(middle) < 4)
        val edge = drawn.getPixel(10, 30)
        assertTrue("the edge fades to half: ${Integer.toHexString(edge)}", Color.green(edge) in 80..176)
        assertEquals("nothing past the region", Color.WHITE, drawn.getPixel(8, 30))
    }
}
