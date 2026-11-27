package dev.exact.widgets

import android.content.Context
import java.io.File
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class WidgetsTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val storeDir = File(context.noBackupFilesDir, "exact-widgets")

    @After
    fun tearDown() {
        storeDir.deleteRecursively()
    }

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

    @Test
    fun restore_remaps_overlapping_shifts_cycles_and_unconfigured_slots_in_parallel() {
        val provider = Provider()

        // Overlapping shift: [2, 3] -> [3, 4] must not overwrite 3 or chain 2 into 4.
        Store.configure(context, 2, JSONObject("""{"station":"Palo Alto"}"""))
        Store.configure(context, 3, JSONObject("""{"station":"San Jose"}"""))
        provider.onRestored(context, intArrayOf(2, 3), intArrayOf(3, 4))
        assertNull(Store.configuration(context, 2))
        assertEquals("""{"station":"Palo Alto"}""", Store.configuration(context, 3))
        assertEquals("""{"station":"San Jose"}""", Store.configuration(context, 4))
        assertTrue(storeDir.listFiles().orEmpty().none { it.name.endsWith(".tmp") })

        // Cycle: [3, 4] -> [4, 3] swaps configurations without clobbering either side.
        provider.onRestored(context, intArrayOf(3, 4), intArrayOf(4, 3))
        assertEquals("""{"station":"San Jose"}""", Store.configuration(context, 3))
        assertEquals("""{"station":"Palo Alto"}""", Store.configuration(context, 4))

        // Unconfigured source in an overlapping shift clears any stale file at its target and never chains.
        storeDir.deleteRecursively()
        Store.configure(context, 2, JSONObject("""{"station":"Sunnyvale"}"""))
        Store.configure(context, 4, JSONObject("""{"station":"Stale"}"""))
        provider.onRestored(context, intArrayOf(2, 3), intArrayOf(3, 4))
        assertNull(Store.configuration(context, 2))
        assertEquals("""{"station":"Sunnyvale"}""", Store.configuration(context, 3))
        assertNull(Store.configuration(context, 4))

        // Three-cycle with an unconfigured slot and a preserved identity entry.
        storeDir.deleteRecursively()
        Store.configure(context, 1, JSONObject("""{"station":"A"}"""))
        Store.configure(context, 3, JSONObject("""{"station":"C"}"""))
        Store.configure(context, 9, JSONObject("""{"station":"Keep"}"""))
        provider.onRestored(context, intArrayOf(1, 2, 3, 9), intArrayOf(2, 3, 1, 9))
        assertEquals("""{"station":"C"}""", Store.configuration(context, 1))
        assertEquals("""{"station":"A"}""", Store.configuration(context, 2))
        assertNull(Store.configuration(context, 3))
        assertEquals("""{"station":"Keep"}""", Store.configuration(context, 9))
        assertTrue(storeDir.listFiles().orEmpty().none { it.name.endsWith(".tmp") })
    }
}
