package dev.exact.widgets

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class RowTest {
    @Test
    fun a_row_reads_its_titles_preview_and_parameters_and_defaults_each_parameter() {
        val row = Row.parse(JSONObject("""
            {"kind":"next-trains","title":"Next Trains","description":"Departures.","preview":{"station":"Mountain View"},
             "configuration":{"parameters":[{"name":"rows","title":"Departures","default":"two",
               "values":[{"value":"one","title":"1"},{"value":"two"}]}]}}
        """))
        assertEquals("Next Trains", row.title)
        assertEquals("Mountain View", JSONObject(row.preview).getString("station"))
        val p = row.parameters.single()
        assertEquals(listOf("one" to "1", "two" to "two"), p.values)
        assertEquals("""{"rows":"two"}""", row.defaults().toString())
    }

    @Test
    fun a_bare_row_takes_its_kind_as_its_title_and_has_no_parameters() {
        val row = Row.parse(JSONObject("""{"kind":"library"}"""))
        assertEquals("library", row.title)
        assertEquals("{}", row.preview)
        assertEquals(0, row.parameters.size)
        assertEquals("{}", row.defaults().toString())
    }
}
