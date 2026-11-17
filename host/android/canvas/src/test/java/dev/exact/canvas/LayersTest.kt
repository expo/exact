package dev.exact.canvas

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
class LayersTest {
    private val matrix = intArrayOf(1, 1f.toRawBits(), 0, 0, 1f.toRawBits(), 0, 0)
    private val background = intArrayOf(2, 0xff0000ff.toInt(), 0, 0, 100f.toRawBits(), 100f.toRawBits()) + IntArray(8)
    private val glyphs = intArrayOf(9, 1, 16f.toRawBits(), 0xff0000ff.toInt(), 0, 1, 42, 0, 16f.toRawBits())
    /** An op 30 body: a matrix, pivot, base and dash word (-1), then the drawing. */
    private val layerWords = intArrayOf(
        1f.toRawBits(), 0, 0, 0, 1f.toRawBits(), 0, 0, 0,
        1f.toRawBits(), 0, 0, 0, 1f.toRawBits(), 0, 0, -1,
    ) + background

    @Test fun colorTransitionsReachTheBackgroundAndTextInsideAPathClip() {
        // The issue's triangle, including the path's own matrix before the drawing's matrix.
        val triangle = intArrayOf(5, 0, 4, 0, 0, 0, 1, 100f.toRawBits(), 0, 1, 0, 100f.toRawBits(), 3)
        val prefix = matrix + triangle + matrix
        val words = prefix + background + glyphs + glyphs + intArrayOf(6)
        val visited = ArrayList<Int>()
        Structure().walkColorLayer(words) { visited.add(it) }
        assertEquals(listOf(0, matrix.size, matrix.size + triangle.size, prefix.size,
            prefix.size + background.size, prefix.size + background.size + glyphs.size, words.lastIndex), visited)
    }

    @Test fun colorTransitionsTraverseClipUnionsMasksAndImageTint() {
        val union = intArrayOf(45, 10, 2, 0, 2, 0, 0, 0, 3, 1, 1, 3)
        val mask = intArrayOf(46)
        val tint = intArrayOf(47, 0xff123456.toInt())
        for (op in listOf(union, mask, tint)) {
            val words = matrix + op + background + glyphs
            val visited = ArrayList<Int>()
            Structure().walkColorLayer(words) { visited.add(it) }
            assertEquals("op ${op[0]}", listOf(0, matrix.size, matrix.size + op.size,
                matrix.size + op.size + background.size), visited)
        }
    }

    @Test fun anUnknownOpStopsRecoloringAndRemainsVisibleToTheGlyphOnlyCheck() {
        val words = matrix + glyphs + intArrayOf(99) + background
        val visited = ArrayList<Int>()
        Structure().walkColorLayer(words) { visited.add(it) }
        assertEquals(listOf(0, matrix.size, matrix.size + glyphs.size), visited)
    }

    @Test fun linearHoldsTheSecondOutputBeforeDuplicateFirstInputs() {
        // linear(0 25%, 1 25%, 1), as Chrome and motion/src/easing.rs evaluate it.
        val stops = floatArrayOf(0.25f, 0f, 0.25f, 1f, 1f, 1f)
        assertEquals(1.0, Layers.piecewise(stops, 0.1), 1e-9)
    }

    @Test fun linearExtrapolatesBeforeAndAfterTwoStops() {
        // linear(0 25%, 1 75%).
        val stops = floatArrayOf(0.25f, 0f, 0.75f, 1f)
        assertEquals(-0.5, Layers.piecewise(stops, 0.0), 1e-9)
        assertEquals(1.5, Layers.piecewise(stops, 1.0), 1e-9)
    }

    @Test fun linearUsesTheLastOutputAtEachSharedInput() {
        val stops = floatArrayOf(
            0.25f, 0f, 0.25f, 1f, 0.25f, 2f,
            0.5f, 3f, 0.5f, 4f,
            0.75f, 5f, 0.75f, 6f,
        )
        assertEquals(2.0, Layers.piecewise(stops, 0.25), 1e-9)
        assertEquals(4.0, Layers.piecewise(stops, 0.5), 1e-9)
        assertEquals(6.0, Layers.piecewise(stops, 0.75), 1e-9)
        assertEquals(2.5, Layers.piecewise(stops, 0.375), 1e-9)
        assertEquals(4.5, Layers.piecewise(stops, 0.625), 1e-9)
        assertEquals(1.0, Layers.piecewise(stops, 0.0), 1e-9)
        assertEquals(6.0, Layers.piecewise(stops, 1.0), 1e-9)
    }

    @Test fun linearExtrapolatesUsingEachOuterSegment() {
        val stops = floatArrayOf(0.25f, 1f, 0.5f, 2f, 0.75f, 4f)
        assertEquals(0.0, Layers.piecewise(stops, 0.0), 1e-9)
        assertEquals(6.0, Layers.piecewise(stops, 1.0), 1e-9)
        assertEquals(1.5, Layers.piecewise(stops, 0.375), 1e-9)
        assertEquals(3.0, Layers.piecewise(stops, 0.625), 1e-9)
    }

    @Test fun linearWithAllInputsEqualHoldsTheSecondThenTheLastOutput() {
        val stops = floatArrayOf(0.5f, 1f, 0.5f, 2f, 0.5f, 3f)
        assertEquals(2.0, Layers.piecewise(stops, 0.0), 1e-9)
        assertEquals(3.0, Layers.piecewise(stops, 0.5), 1e-9)
        assertEquals(3.0, Layers.piecewise(stops, 1.0), 1e-9)
    }

    @Test fun linearWithNoStopsPreservesTheInput() {
        for (input in listOf(0.0, 0.25, 1.0)) {
            assertEquals(input, Layers.piecewise(floatArrayOf(), input), 1e-9)
        }
    }

    @Test fun linearWithOneStopHoldsItsOutput() {
        val stops = floatArrayOf(0.5f, 2f)
        for (input in listOf(0.0, 0.5, 1.0)) {
            assertEquals(2.0, Layers.piecewise(stops, input), 1e-9)
        }
    }

    @Test fun aLayerThatLostItsDrawingIsRecordedAgainAsItIsDrawn() {
        var recorded = 0
        val layers = Layers({ node, _, _, _, _, _ -> recorded++; node.beginRecording(); node.endRecording() })
        val node = layers.node(7)
        layers.set(7, IntBuffer.wrap(layerWords), 0, layerWords.size)
        node.beginRecording(); node.endRecording()
        // Its drawing rows freed before another drew it: HWUI let the node go, and exact2 sends op 30 only on change.
        node.discardDisplayList()
        assertFalse(node.hasDisplayList())
        assertSame(node, layers.drawn(7))
        assertTrue(node.hasDisplayList())
        assertEquals(1, recorded)
        layers.drawn(7)
        assertEquals("a live node is not recorded again", 1, recorded)
    }

    @Test fun aLayerLostInsideAnothersRefillIsRecordedAfterIt() {
        // The outer's re-recording draws the inner (op 29): the inner's refill waits, not nested into the same replay.
        val order = ArrayList<Int>()
        lateinit var layers: Layers
        var inner: RenderNode? = null
        layers = Layers({ node, _, _, _, _, _ ->
            val id = if (node === inner) 2 else 1
            order.add(id)
            node.beginRecording(); node.endRecording()
            if (id == 1) { assertSame(inner, layers.drawn(2)); assertFalse(inner!!.hasDisplayList()) }
        })
        val outer = layers.node(1); inner = layers.node(2)
        layers.set(1, IntBuffer.wrap(layerWords), 0, layerWords.size)
        layers.set(2, IntBuffer.wrap(layerWords), 0, layerWords.size)
        outer.beginRecording(); outer.endRecording()
        inner!!.beginRecording(); inner!!.endRecording()
        outer.discardDisplayList(); inner!!.discardDisplayList()
        assertSame(outer, layers.drawn(1))
        assertEquals(listOf(1, 2), order)
        assertTrue(inner!!.hasDisplayList())
    }

    @Test fun aLayerLostInsideADashPhasesRecordingIsRecordedAfterIt() {
        // A dash phase records the outer again through the same Player a refill uses: the inner waits for it too.
        val order = ArrayList<Int>()
        lateinit var layers: Layers
        lateinit var inner: RenderNode
        layers = Layers({ node, _, _, _, _, _ ->
            val id = if (node === inner) 2 else 1
            order.add(id)
            node.beginRecording(); node.endRecording()
            if (id == 1) assertFalse(layers.drawn(2)!!.hasDisplayList())
        })
        val outer = layers.node(1); inner = layers.node(2)
        // Path units per dash offset unit (word 14), and the word its phase is written to (word 15).
        val dashed = layerWords.copyOf().also { it[14] = 1f.toRawBits(); it[15] = 2 }
        layers.set(1, IntBuffer.wrap(dashed), 0, dashed.size)
        layers.set(2, IntBuffer.wrap(layerWords), 0, layerWords.size)
        outer.beginRecording(); outer.endRecording()
        inner.beginRecording(); inner.endRecording()
        inner.discardDisplayList()
        // The dash offset from 0 to 10 over a second, both fills.
        val track = intArrayOf(6, 0, 0, 0, 0, 1f.toRawBits(), 1f.toRawBits(), 0, 3, 0, 2, 0, 0, 0, 1f.toRawBits(), 10f.toRawBits(), 0)
        layers.tracks(1, IntBuffer.wrap(track), 0, track.size)
        layers.clock(0, 0)
        layers.sample(500_000_000L)
        assertEquals(listOf(1, 2), order)
        assertTrue(inner.hasDisplayList())
    }

    @Test fun anUnsetOrFreedLayerRefillsNothing() {
        var recorded = 0
        val layers = Layers({ node, _, _, _, _, _ -> recorded++; node.beginRecording(); node.endRecording() })
        val unset = layers.node(3)   // an op 29 before its op 30 in one stream: nothing kept to record again
        assertSame(unset, layers.drawn(3))
        assertFalse(unset.hasDisplayList())
        assertEquals(0, recorded)
        layers.set(4, IntBuffer.wrap(layerWords), 0, layerWords.size)
        layers.free(4); layers.flush { }
        assertNull(layers.drawn(4))
        assertEquals(0, recorded)
    }
}
