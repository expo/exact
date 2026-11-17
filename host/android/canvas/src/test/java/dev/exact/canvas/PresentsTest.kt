package dev.exact.canvas

import org.junit.Assert.assertEquals
import org.junit.Test

class PresentsTest {
    /** A frame handed over at a vsync shows at it; one handed over between vsyncs shows at the next. */
    @Test
    fun aPresentIsTheFirstVsyncAtOrAfterTheCommit() {
        val period = 16_666_667L
        assertEquals(0L, presentsAt(0L, period))
        assertEquals(0L, presentsAt(-5L, period))
        assertEquals(period, presentsAt(1L, period))
        assertEquals(period, presentsAt(period, period))
        assertEquals(2 * period, presentsAt(period + 1, period))
        assertEquals(0L, presentsAt(1_000L, 0L))
    }
}
