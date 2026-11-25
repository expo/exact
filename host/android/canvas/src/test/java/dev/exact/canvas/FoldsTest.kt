package dev.exact.canvas

import android.graphics.Rect
import androidx.window.layout.FoldingFeature
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class FoldsTest {
    private fun feature(
        overrideBounds: Rect, vertical: Boolean = true, separating: Boolean = true, bent: Boolean = false,
    ) = object : FoldingFeature {
        override val bounds = overrideBounds
        override val orientation = if (vertical) FoldingFeature.Orientation.VERTICAL else FoldingFeature.Orientation.HORIZONTAL
        override val state = if (bent) FoldingFeature.State.HALF_OPENED else FoldingFeature.State.FLAT
        override val isSeparating = separating
        override val occlusionType = FoldingFeature.OcclusionType.NONE
    }

    @Test fun postureAndTheBandComeFromIndependentObservations() {
        fun report(vararg features: FoldingFeature) = windowFold(features.toList(), Rect(0, 0, 900, 1402), false)
        val flat = feature(Rect(420, -100, 450, 1500))
        val crease = feature(Rect(0, 700, 900, 700), vertical = false, bent = true)
        val outside = feature(Rect(920, 0, 940, 1402), bent = true)
        val nonSeparating = feature(Rect(200, 0, 200, 1402), separating = false)
        assertArrayEquals(intArrayOf(1, 1, 420, 450), report(flat))
        assertArrayEquals(intArrayOf(1, 1, 400, 400), report(feature(Rect(400, 0, 400, 1402))))
        assertArrayEquals(intArrayOf(2, 2, 700, 700), report(nonSeparating, crease))
        assertArrayEquals(intArrayOf(2, 1, 420, 450), report(outside, flat))
        assertArrayEquals(intArrayOf(2, 0, 0, 0), report(outside))
        assertArrayEquals(intArrayOf(1, 0, 0, 0), report(nonSeparating))
        assertArrayEquals(intArrayOf(1, 0, 0, 0), windowFold(emptyList(), Rect(0, 0, 466, 800), true))
        assertArrayEquals(intArrayOf(1, 0, 0, 0), report(feature(Rect(-5, 0, 5, 1402))))
        assertArrayEquals(intArrayOf(1, 0, 0, 0), report(feature(Rect(0, 0, 10, 1402))))
        assertNull(report())
    }
}
