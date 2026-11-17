package dev.exact.widgets

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class WidgetsTest {
    @Test
    fun a_far_countdown_is_approached_by_halves_and_a_near_one_alarmed_at_its_instant() {
        val instant = 15 * 60_000.0
        // Each alarm runs on time, or as late as Android allows an inexact one: three quarters of its lead.
        for (late in listOf(0.0, 0.75)) {
            var now = 0.0
            var wakes = 0
            while (true) {
                val at = Widgets.approach(instant, now)
                if (at == instant) break
                now = at + late * (at - now)
                assertTrue("still before the instant", now < instant)
                wakes++
            }
            assertTrue("the last alarm's lead is a minute or less", instant - now <= 60_000.0)
            assertTrue("$wakes wakes", wakes in 1..4)
        }
        assertEquals(instant, Widgets.approach(instant, instant - 30_000.0), 0.0)
        assertEquals(Double.POSITIVE_INFINITY, Widgets.approach(Double.POSITIVE_INFINITY, 0.0), 0.0)
    }
}
