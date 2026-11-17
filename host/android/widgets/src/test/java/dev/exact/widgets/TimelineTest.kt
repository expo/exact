package dev.exact.widgets

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

// Robolectric for Android's org.json, which the JVM's android.jar only stubs.
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class TimelineTest {
    private fun entries(vararg dates: Double) = dates.joinToString(",", "[", "]") { """{"date":$it,"props":{"n":$it}}""" }

    @Test
    fun the_entry_shown_is_the_latest_not_after_now_else_the_first() {
        val t = Timeline.admit(entries(1_000.0, 2_000.0, 3_000.0))
        assertEquals(1_000.0, t.current(500.0).date, 0.0)
        assertEquals(2_000.0, t.current(2_000.0).date, 0.0)
        assertEquals(3_000.0, t.current(9_000.0).date, 0.0)
        assertEquals(2_000.0, t.next(1_500.0)!!, 0.0)
        assertNull(t.next(3_000.0))
        assertEquals(2_000.0, JSONObject(t.current(2_500.0).props).getDouble("n"), 0.0)
    }

    @Test
    fun the_next_refresh_is_past_the_last_entry_and_at_least_the_retry_on() {
        val t = Timeline.admit(entries(1_000.0, 2_000.0))
        assertEquals(10_000.0 + Timeline.RETRY, t.refreshDue(10_000.0), 0.0)
        val far = Timeline.RETRY * 3
        assertEquals(far + 1_000.0, Timeline.admit(entries(far)).refreshDue(0.0), 0.0)
    }

    @Test
    fun a_kept_timeline_reads_back_as_it_was_admitted() {
        val t = Timeline.admit(entries(1_000.0, 2_000.0))
        val again = Timeline.admit(t.json())
        assertEquals(t.entries.map { it.date }, again.entries.map { it.date })
        assertEquals(t.entries.map { JSONObject(it.props).toString() }, again.entries.map { it.props })
    }

    @Test
    fun admission_refuses_what_apple_refuses() {
        fun refused(json: String) = assertThrows(IllegalArgumentException::class.java) { Timeline.admit(json) }.message!!
        assertTrue(refused("[]").contains("1 to 100"))
        assertTrue(refused(entries(*DoubleArray(101) { it.toDouble() })).contains("1 to 100"))
        assertTrue(refused(entries(2_000.0, 1_000.0)).contains("strictly increasing"))
        assertTrue(refused(entries(1_000.0, 1_000.0)).contains("strictly increasing"))
        assertTrue(refused("""[{"date":"x","props":{}}]""").contains("not a date and props"))
        // A refresh that failed says why, which is what the log shows.
        assertEquals("refresh widgetLibrary: timed out", refused("""{"error":"refresh widgetLibrary: timed out"}"""))
        assertTrue(refused("[" + "{\"date\":1,\"props\":{\"s\":\"" + "x".repeat(1 shl 20) + "\"}}]").contains("1 MiB"))
    }
}
