package dev.exact.canvas

import org.junit.Assert.assertEquals
import org.junit.Test

class BarAppearanceTest {
    @Test fun theStatusAndNavigationBarsDecodeIndependently() {
        assertEquals(BarAppearance(true, true), barAppearance(0))
        assertEquals(BarAppearance(true, false), barAppearance(1))
        assertEquals(BarAppearance(false, true), barAppearance(2))
        assertEquals(BarAppearance(false, false), barAppearance(3))
        assertEquals(BarAppearance(false, true, hidden = true), barAppearance(6))
        assertEquals(BarAppearance(true, true, hidden = true), barAppearance(4))
    }
}
