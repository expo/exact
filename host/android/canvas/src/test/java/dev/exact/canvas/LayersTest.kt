package dev.exact.canvas

import org.junit.Assert.*
import org.junit.Test

class LayersTest {
    private val matrix = intArrayOf(1, 1f.toRawBits(), 0, 0, 1f.toRawBits(), 0, 0)
    private val background = intArrayOf(2, 0xff0000ff.toInt(), 0, 0, 100f.toRawBits(), 100f.toRawBits()) + IntArray(8)
    private val glyphs = intArrayOf(9, 1, 16f.toRawBits(), 0xff0000ff.toInt(), 0, 1, 42, 0, 16f.toRawBits())

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
}
