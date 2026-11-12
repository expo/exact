package dev.exact.canvas

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
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
class PlaneTest {
    private fun f(v: Float) = v.toRawBits()
    private fun matrix(v: FloatArray) = intArrayOf(48) + v.map { f(it) }.toIntArray()
    private fun rect(color: Int, x: Float, y: Float, w: Float, h: Float) =
        intArrayOf(2, color, f(x), f(y), f(w), f(h)) + IntArray(8)
    private val identity = intArrayOf(1, f(1f), 0, 0, f(1f), 0, 0)
    private val polygon = intArrayOf(3, Color.RED, 0, 5,
        0, 0, 0, 1, f(80f), 0, 1, f(60f), f(60f), 1, 0, f(60f), 3)

    private fun pixels(bitmap: Bitmap): IntArray = IntArray(bitmap.width * bitmap.height).also {
        bitmap.getPixels(it, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
    }

    @Test fun projectivePathsMatchCanvasAndReturnToAffineDrawing() {
        val path = Path().apply { moveTo(0f, 0f); lineTo(80f, 0f); lineTo(60f, 60f); lineTo(0f, 60f); close() }
        for (w in listOf(1f, 2f)) for (shift in listOf(0f, 7f)) {
            val values = floatArrayOf(1f, 0f, 30f, 0f, 1f, 30f, 0.003f, 0f, w)
            val ops = IntBuffer.wrap(matrix(values) + polygon + identity + rect(Color.BLUE, 150f, 150f, 20f, 20f))
            val expected = Bitmap.createBitmap(200, 200, Bitmap.Config.ARGB_8888)
            val canvas = Canvas(expected)
            canvas.setMatrix(Matrix().apply { setValues(values); postTranslate(-shift, -shift) })
            canvas.drawPath(path, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.RED })
            canvas.setMatrix(Matrix().apply { setTranslate(-shift, -shift) })
            canvas.drawRect(150f, 150f, 170f, 170f, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.BLUE })
            val actual = Bitmap.createBitmap(200, 200, Bitmap.Config.ARGB_8888)
            Player(emptyMap()).apply { width = 200; height = 200 }
                .replayAt(Canvas(actual), ops, 0, ops.limit(), shift, shift)
            assertArrayEquals("w=$w, shift=$shift", pixels(expected), pixels(actual))
            assertEquals(10, Structure().end(ops, 0))
            val bounds = RectF()
            assertTrue(Bounds().measure(ops, 0, ops.limit(), bounds))
            assertTrue(bounds.left < 31f && bounds.right >= 170f)
        }
    }
    @Test fun turnedGradientMatchesTheExistingChromeReference() {
        // scripts/fixtures/space.contract, testId="y": an 80×50 card about (70,55).
        val angle = Math.toRadians(40.0)
        val c = Math.cos(angle).toFloat()
        val k = Math.sin(angle).toFloat() / 200f
        val h = floatArrayOf(c + 70f * k, 0f, 70f * (1f - c) - 4900f * k,
            55f * k, 1f, -3850f * k, k, 0f, 1f - 70f * k)
        val shader = intArrayOf(43, 19, 0, 0) +
            floatArrayOf(30f, 55f, 110f, 55f, 0f, 0f, 1f, 0f, 0f, 1f, 0f, 0f).map { f(it) }.toIntArray() +
            intArrayOf(2, 0, 0xffe53935.toInt(), f(1f), 0xff1e88e5.toInt())
        val ops = IntBuffer.wrap(matrix(h) + shader + rect(Color.BLACK, 30f, 30f, 80f, 50f))
        val actual = Bitmap.createBitmap(390, 460, Bitmap.Config.ARGB_8888)
        actual.eraseColor(Color.WHITE)
        Player(emptyMap()).apply { width = 390; height = 460 }.replay(Canvas(actual), ops, 0, ops.limit())
        val reference = android.graphics.BitmapFactory.decodeFile("../../../scripts/fixtures/space.web.png")
        assertNotNull("the existing Chrome reference", reference)
        var total = 0L
        var outside = 0
        for (y in 20 until 90) for (x in 20 until 120) {
            val a = actual.getPixel(x, y)
            val b = reference.getPixel(x, y)
            var worst = 0
            for (shift in listOf(0, 8, 16)) {
                val d = Math.abs(((a ushr shift) and 255) - ((b ushr shift) and 255))
                total += d; worst = Math.max(worst, d)
            }
            if (worst > 48) outside++
        }
        val mean = total.toDouble() / (100 * 70 * 3)
        println("Chrome y-rotation: mean=$mean/255, outliers=$outside/7000")
        assertTrue("mean difference $mean/255", mean <= 3.0)
        assertTrue("$outside pixels differ by more than 48/255", outside <= 70)
    }

    @Test fun projectiveShadowMatchesTheSoftwareIsland() {
        // The Rust plane test checks these commands and pixels against Painter::spatial.
        val bytes = javaClass.getResourceAsStream("/projective-shadow.ops")!!.readBytes()
        val ops = java.nio.ByteBuffer.wrap(bytes).order(java.nio.ByteOrder.LITTLE_ENDIAN).asIntBuffer()
        val reference = android.graphics.BitmapFactory.decodeStream(
            javaClass.getResourceAsStream("/projective-shadow.png"))
        val actual = Bitmap.createBitmap(320, 240, Bitmap.Config.ARGB_8888)
        actual.eraseColor(Color.WHITE)
        Player(emptyMap()).apply { width = 320; height = 240 }.replay(Canvas(actual), ops, 0, ops.limit())
        var total = 0L
        var outside = 0
        var shadow = 0
        for (y in 0 until 240) for (x in 0 until 320) {
            val a = actual.getPixel(x, y)
            val b = reference.getPixel(x, y)
            var worst = 0
            for (shift in listOf(0, 8, 16)) {
                val d = Math.abs(((a ushr shift) and 255) - ((b ushr shift) and 255))
                total += d; worst = Math.max(worst, d)
            }
            if (worst > 48) outside++
            if (Color.red(b) in 30..230) shadow++
        }
        val mean = total.toDouble() / (320 * 240 * 3)
        println("Software projective shadow: mean=$mean/255, outliers=$outside/76800, shadow=$shadow")
        assertTrue("the reference contains the shadow", shadow > 1000)
        assertTrue("mean difference $mean/255", mean <= 3.0)
        assertTrue("$outside pixels differ by more than 48/255", outside <= 768)
    }

    @Test fun projectiveClipsAndGroupOpacityMatchCanvas() {
        val outer = floatArrayOf(1f, 0f, 30f, 0f, 1f, 20f, 0.002f, 0f, 1f)
        val inner = floatArrayOf(0.8f, 0.1f, 20f, 0.1f, 1f, 20f, 0.003f, 0.001f, 1f)
        val clip = intArrayOf(4, 0, 0, f(100f), f(100f)) + IntArray(8)
        val ops = IntBuffer.wrap(matrix(outer) + clip + intArrayOf(7, f(0.5f)) + matrix(inner) +
            rect(Color.RED, 0f, 0f, 100f, 80f) + rect(Color.BLUE, 40f, 40f, 80f, 80f) +
            intArrayOf(6, 6) + identity + rect(Color.GREEN, 160f, 160f, 20f, 20f))
        val expected = Bitmap.createBitmap(200, 200, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(expected)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        canvas.setMatrix(Matrix().apply { setValues(outer) })
        canvas.save(); canvas.clipRect(0f, 0f, 100f, 100f)
        canvas.saveLayerAlpha(null, 127)
        canvas.setMatrix(Matrix().apply { setValues(inner) })
        paint.color = Color.RED; canvas.drawRect(0f, 0f, 100f, 80f, paint)
        paint.color = Color.BLUE; canvas.drawRect(40f, 40f, 120f, 120f, paint)
        canvas.restore(); canvas.restore(); canvas.setMatrix(Matrix())
        paint.color = Color.GREEN; canvas.drawRect(160f, 160f, 180f, 180f, paint)
        val actual = Bitmap.createBitmap(200, 200, Bitmap.Config.ARGB_8888)
        Player(emptyMap()).apply { width = 200; height = 200 }.replay(Canvas(actual), ops, 0, ops.limit())
        assertArrayEquals(pixels(expected), pixels(actual))
    }

}
