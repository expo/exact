package dev.exact.canvas

import android.app.Activity
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.HardwareRenderer
import android.graphics.RenderNode
import android.view.View
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.IntBuffer
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativeLayersTest {
    private fun f(n: Float) = n.toRawBits()
    private val identity = intArrayOf(1, f(1f), 0, 0, f(1f), 0, 0)
    private val header = intArrayOf(f(1f), 0, 0, f(1f), 0, 0, 0, 0, 0, 0, f(1f), 0, f(1f), 0, 0, f(1f), -1)

    private fun native(color: String): IntArray {
        val bytes = """{"module":"test-view","props":"{\"color\":\"$color\"}"}""".toByteArray()
        val words = IntArray((bytes.size + 3) / 4)
        for (i in bytes.indices) words[i / 4] = words[i / 4] or ((bytes[i].toInt() and 255) shl (8 * (i % 4)))
        return identity + intArrayOf(28, 1, 2, f(8f), f(8f), f(16f), f(16f)) + IntArray(8) + intArrayOf(bytes.size) + words
    }

    private class Fixture {
        val activity = Robolectric.buildActivity(Activity::class.java).get()
        init { activity.intent.putExtra("async", "0") }
        var creates = 0
        var destroys = 0
        val module = object : NativeModule {
            override fun create(context: Context, props: JSONObject) = View(context).also { creates++; update(it, props) }
            override fun update(view: View, props: JSONObject) { view.setBackgroundColor(Color.parseColor(props.getString("color"))) }
            override fun destroy(view: View) { destroys++ }
        }
        val natives = Natives(activity, { File(".") }, mapOf("test-view" to module))
        val reader = ExactView(activity, false) {}.also { it.natives = natives }
        private val apply = ExactView::class.java.getDeclaredMethod("applyStream", IntBuffer::class.java).apply { isAccessible = true }
        private val player = ExactView::class.java.getDeclaredField("main").apply { isAccessible = true }.get(reader) as Player
        val layers = ExactView::class.java.getDeclaredField("layers").apply { isAccessible = true }.get(reader) as Layers
        init { player.width = 64; player.height = 64 }
        fun ingest(ops: IntArray) { apply.invoke(reader, IntBuffer.wrap(ops)) }
        fun pixel(ops: IntArray, x: Int = 16, y: Int = 16): Int {
            ingest(ops)
            val page = RenderNode("page").apply { setPosition(0, 0, 64, 64) }
            val canvas = page.beginRecording()
            canvas.drawColor(Color.WHITE)
            player.replay(canvas, IntBuffer.wrap(ops), 0, ops.size)
            page.endRecording()
            val bitmap = HardwareRenderer::class.java.getDeclaredMethod("createHardwareBitmap",
                RenderNode::class.java, Int::class.javaPrimitiveType, Int::class.javaPrimitiveType)
                .invoke(null, page, 64, 64) as Bitmap
            val copy = bitmap.copy(Bitmap.Config.ARGB_8888, false)
            val color = copy.getPixel(x, y)
            copy.recycle(); bitmap.recycle()
            return color
        }
    }

    private fun recorded(name: String): IntArray {
        val bytes = javaClass.getResourceAsStream("/$name.ops")!!.use { it.readBytes() }
        val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asIntBuffer()
        return IntArray(buffer.remaining()).also { buffer.get(it) }
    }

    @Test fun drawsTheLayerFromTheRustRecorder() {
        assertEquals(Color.RED, Fixture().pixel(recorded("red-layer")))
    }

    // A blue box, 32 x 16 at (8, 16) in layer 1, whose scale goes from 2 2 to 1 0.5 about (24, 24) in one second.
    @Test fun drawsTheScaledLayerFromTheRustRecorder() {
        with(Fixture()) {
            ingest(recorded("scaled-layer"))
            val node = layers.node(1)
            assertTrue(node.hasDisplayList())
            layers.sample(2_000_000_000L)
            assertEquals(1f, node.scaleX, 0f)
            assertEquals(0.5f, node.scaleY, 0f)
            val ref = intArrayOf(29, 1)
            assertEquals("the middle", Color.BLUE, pixel(ref, 24, 24))
            assertEquals("above half the height", Color.WHITE, pixel(ref, 24, 18))
            assertEquals("below half the height", Color.WHITE, pixel(ref, 24, 30))
            assertEquals("the whole width", Color.BLUE, pixel(ref, 10, 24))
            assertEquals("left of the width", Color.WHITE, pixel(ref, 6, 24))
        }
    }

    @Test fun playsEachScaleTrackFromTheRustRecorder() {
        with(Fixture()) {
            // The clock's zero is later than the sample the reader takes at the stream's end.
            val zero = Long.MAX_VALUE / 2
            ingest(recorded("scaled-layer") + intArrayOf(33, zero.toInt(), (zero ushr 32).toInt()))
            val node = layers.node(1)
            assertEquals(2f, node.scaleX, 0f)
            assertEquals(2f, node.scaleY, 0f)
            layers.sample(zero + 500_000_000L)
            assertEquals(1.5f, node.scaleX, 1e-3f)
            assertEquals(1.25f, node.scaleY, 1e-3f)
            layers.sample(zero + 2_000_000_000L)
            assertEquals(1f, node.scaleX, 0f)
            assertEquals(0.5f, node.scaleY, 0f)
        }
    }

    private val ref = intArrayOf(29, 7)
    private fun layer(id: Int, ops: IntArray) = (header + ops).let { intArrayOf(30, id, it.size) + it }
    private fun row(id: Int, ops: IntArray = ref) = intArrayOf(15, id, 0, f(64f), f(64f), ops.size + 1) + ops + intArrayOf(16)
    private fun draw(id: Int) = intArrayOf(17, id, 0, 0)

    @Test fun replacingALayersContentsReleasesItsNativeViewWhileTheRowRemains() {
        with(Fixture()) {
            ingest(row(10) + layer(7, native("red")) + draw(10))
            assertNotNull(natives.node(1))
            assertEquals(0, destroys)
            ingest(layer(7, identity) + draw(10))
            assertNull("row 10 still draws layer 7, whose native child is gone", natives.node(1))
            assertEquals("removal destroys the module view", 1, destroys)
            natives.destroyAll()
            assertEquals("the removed view is destroyed once", 1, destroys)
        }
    }

    @Test fun droppingALayerPreservesADirectReferenceFromTheSameRow() {
        with(Fixture()) {
            ingest(row(10, native("red") + ref) + layer(7, native("red")) + draw(10))
            ingest(layer(7, identity) + draw(10))
            assertNotNull("the row still draws the native view directly", natives.node(1))
            assertEquals(1, creates)
            assertEquals(0, destroys)
            ingest(intArrayOf(18, 10))
            ingest(intArrayOf())
            assertNull(natives.node(1))
            assertEquals(1, destroys)
            natives.destroyAll()
        }
    }

    @Test fun droppingOneLayerPreservesAnotherLayersReference() {
        with(Fixture()) {
            ingest(row(10) + row(11, intArrayOf(29, 8)) + layer(7, native("red")) +
                layer(8, native("red")) + draw(10) + draw(11))
            ingest(layer(7, identity) + draw(10) + draw(11))
            assertNotNull("layer 8 still draws the native view", natives.node(1))
            assertEquals(1, creates)
            assertEquals(0, destroys)
            ingest(layer(8, identity) + draw(10) + draw(11))
            assertNull("neither retained row directly draws the native view", natives.node(1))
            assertEquals(1, destroys)
            natives.destroyAll()
        }
    }

    @Test fun aNativeViewRemainsDrawnWhenItsAncestorLayerIsRetained() {
        val fixture = Fixture()
        fun stream(color: String) = ref + layer(7, native(color))
        with(fixture) {
            assertEquals("first drawing", Color.RED, pixel(stream("red")))
            assertEquals("retained ancestor", Color.RED, pixel(ref))
            assertEquals("changed props", Color.BLUE, pixel(stream("blue")))
            assertEquals("retained after the update", Color.BLUE, pixel(ref))
            assertEquals("the same view handles the update", 1, creates)
            ingest(intArrayOf(32, 7))
            ingest(intArrayOf())
            assertNull("the last layer releases the native view", natives.node(1))
            natives.destroyAll()
        }
    }

    private fun held(prop: Int, from: Float, to: Float) = intArrayOf(
        prop, 0, 1, f(0.5f), 0, f(1f), f(1f), 0, 3, 0, 2, 0, f(from), 0, f(1f), f(to), 0,
    )

    @Test fun nestedLayersMoveAndFadeTheirNativeView() {
        val fixture = Fixture()
        val tracks = held(0, 0f, 1f) + held(1, 0f, 16f)
        val nested = intArrayOf(29, 8)
        val ops = ref + layer(8, native("red")) + layer(7, nested) +
            intArrayOf(31, 7, tracks.size) + tracks +
            held(2, 0f, 16f).let { intArrayOf(31, 8, it.size) + it }
        with(fixture) {
            fun check(ops: IntArray) {
                val color = pixel(ops, 24, 24)
                assertEquals(255, Color.red(color))
                assertTrue("half opacity over white", Color.green(color) in 126..129)
                assertTrue("half opacity over white", Color.blue(color) in 126..129)
                assertEquals("the untranslated position is empty", Color.WHITE, pixel(ref, 10, 10))
            }
            check(ops)
            check(ref)
            natives.destroyAll()
        }
    }

    @Test fun aLayerKeepsItsNativeViewWhenAnotherRowTakesOver() {
        val fixture = Fixture()
        with(fixture) {
            assertEquals(Color.RED, pixel(row(10) + layer(7, native("red")) + draw(10)))
            assertEquals(Color.RED, pixel(row(11) + draw(11) + intArrayOf(18, 10)))
            assertEquals("the old row is freed on this stream", Color.RED, pixel(draw(11)))
            assertEquals(1, creates)
            assertEquals(Color.WHITE, pixel(row(12) + layer(7, identity) + draw(12) + intArrayOf(18, 11)))
            ingest(draw(12))
            assertNull("the replacement layer no longer draws the native view", natives.node(1))
            natives.destroyAll()
        }
    }
}
