package dev.exact.widgets

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.Chronometer
import android.widget.FrameLayout
import android.widget.TextView
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** Snapshot trees, in the shape `host/painted/src/snapshot/tree.rs` writes, inflated and applied as a launcher does. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class InflaterTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val now = 1_000_000.0
    private val elapsed = 50_000L

    /** The links asked for, by URL. */
    private val asked = mutableListOf<String>()
    private val links = object : Links {
        private fun intent(code: Int, data: String?) = PendingIntent.getActivity(context, code,
            Intent(Intent.ACTION_VIEW).also { if (data != null) it.data = android.net.Uri.parse(data) }, PendingIntent.FLAG_IMMUTABLE)
        override fun open() = intent(0, null)
        override fun link(href: String, index: Int) = intent(1 + index, href).also { asked.add(href) }
    }

    private fun line(text: String, baseline: Float = 14f, color: String = """{"m":4,"a":100}""", extra: String = "") =
        """{"text":"$text","x":0,"top":0,"height":18,"baseline":$baseline,"width":${text.length * 8},"rtl":false,
            "runs":[{"start":0,"end":${text.length},"x":0,"width":${text.length * 8},"size":16,"weight":400,"color":$color$extra}]}"""

    private fun box(y: Float, more: String) = """{"x":0,"y":$y,"w":126,"h":18,$more}"""

    private fun root(children: String, more: String = "") =
        Node.parse(JSONObject("""{"x":0,"y":0,"w":126,"h":126,"background":{"m":3,"a":100}$more,"children":[$children]}"""))

    private fun apply(node: Node): View =
        Inflater(context, links, now, elapsed).widget(node, Widgets.INSET).apply(context, FrameLayout(context))

    private fun all(view: View): List<View> =
        listOf(view) + if (view is ViewGroup) (0 until view.childCount).flatMap { all(view.getChildAt(it)) } else emptyList()

    private fun texts(view: View) = all(view).filterIsInstance<TextView>().filter { it !is Chronometer }.map { it.text.toString() }

    @Test
    fun each_line_is_a_text_view_placed_at_its_box_inside_the_content_margin() {
        val view = apply(root(box(0f, """"heading":2,"lines":[${line("On your shelf")}]""") + "," +
            box(30f, """"lines":[${line("3", extra = ""","tabular":true""")}]""")))
        assertEquals(listOf("On your shelf", "3"), texts(view))
        val three = all(view).filterIsInstance<TextView>().single { it.text.toString() == "3" }
        val density = context.resources.displayMetrics.density
        val margins = three.layoutParams as ViewGroup.MarginLayoutParams
        assertEquals(Math.round(Widgets.INSET * density), margins.leftMargin)
        val heading = all(view).filterIsInstance<TextView>().single { it.text.toString() == "On your shelf" }
        val above = heading.layoutParams as ViewGroup.MarginLayoutParams
        assertEquals("the kernel's 30 dp between the lines", Math.round(30 * density).toFloat(), (margins.topMargin - above.topMargin).toFloat(), 1f)
        assertTrue(heading.isAccessibilityHeading)
    }

    @Test
    fun a_widget_without_a_link_opens_the_app_and_a_root_link_takes_the_whole_tap() {
        val plain = apply(root(box(0f, """"lines":[${line("7")}]""")))
        assertTrue(plain.findViewById<View>(android.R.id.background).hasOnClickListeners())
        assertTrue(asked.isEmpty())
        apply(root(box(0f, """"lines":[${line("7")}]"""), more = ""","href":"/stats""""))
        assertEquals(listOf("/stats"), asked)
    }

    @Test
    fun a_link_inside_is_its_own_tap_target() {
        val view = apply(root(box(40f, """"href":"/book/1","children":[${box(0f, """"lines":[${line("Kindred")}]""")}]""")))
        assertEquals(listOf("/book/1"), asked)
        val clickable = all(view).filter { it.hasOnClickListeners() && it.id != android.R.id.background }
        assertEquals(1, clickable.size)
        assertEquals("Kindred", texts(clickable.single()).single())
    }

    @Test
    fun an_update_reapplied_to_the_views_the_launcher_has_replaces_the_content() {
        val inflater = Inflater(context, links, now, elapsed)
        val views = inflater.widget(root(box(0f, """"lines":[${line("4")}]""")), Widgets.INSET)
        val view = views.apply(context, FrameLayout(context))
        Inflater(context, links, now, elapsed).widget(root(box(0f, """"lines":[${line("5")}]""")), Widgets.INSET).reapply(context, view)
        assertEquals(listOf("5"), texts(view))
    }

    @Test
    fun a_countdown_ticks_while_its_instant_is_ahead_and_shows_the_kernels_text_after() {
        val ahead = apply(root(box(0f, """"time":{"datetime":${now + 300_000},"format":"countdown"},"lines":[${line("5:00")}]""")))
        val clock = all(ahead).filterIsInstance<Chronometer>().single()
        assertTrue(clock.isCountDown)
        assertEquals(elapsed + 300_000, clock.base)
        val passed = apply(root(box(0f, """"time":{"datetime":${now - 1_000},"format":"countdown"},"lines":[${line("0:00")}]""")))
        assertTrue(all(passed).none { it is Chronometer })
        assertEquals(listOf("0:00"), texts(passed))
        // A timer is a duration: it counts up once its instant has passed.
        val timer = apply(root(box(0f, """"time":{"datetime":${now - 60_000},"format":"timer"},"lines":[${line("1:00")}]""")))
        assertFalse(all(timer).filterIsInstance<Chronometer>().single().isCountDown)
    }

    @Test
    fun a_role_is_the_modules_colour_resource_and_a_fixed_colour_its_light_value_by_day() {
        val view = apply(root(box(0f, """"lines":[${line("role")}]""") + "," +
            box(30f, """"lines":[${line("fixed", color = """{"l":-65536,"d":-16711936}""")}]""")))
        val role = all(view).filterIsInstance<TextView>().single { it.text.toString() == "role" }
        val expected = context.getColorStateList(R.color.exact_widget_role_4).defaultColor
        assertEquals(expected, role.currentTextColor)
        val fixed = all(view).filterIsInstance<TextView>().single { it.text.toString() == "fixed" }
        assertEquals(0xffff0000.toInt(), fixed.currentTextColor)
    }

    /** On Android 12 and 13 each role is a colour state list over the device's tonal palettes (`color-v31`), which
     * aapt does not check against the framework as it checks Android 14's aliases (`values-v34`). */
    @Test
    @Config(sdk = [31, 33])
    fun every_role_resolves_on_android_12_and_13_by_day_and_by_night() {
        for (night in listOf(false, true)) {
            val configuration = android.content.res.Configuration(context.resources.configuration)
            configuration.uiMode = (configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK.inv()) or
                if (night) android.content.res.Configuration.UI_MODE_NIGHT_YES else android.content.res.Configuration.UI_MODE_NIGHT_NO
            val themed = context.createConfigurationContext(configuration)
            for (i in 0 until 36) {
                val id = themed.resources.getIdentifier("exact_widget_role_$i", "color", context.packageName)
                assertTrue("role $i", id != 0)
                assertTrue("role $i opaque", (themed.getColorStateList(id).defaultColor ushr 24) == 0xff)
            }
        }
    }

    @Test
    fun paint_the_views_cannot_draw_fails_the_render_by_name() {
        fun refused(node: Node) = assertThrows(Unsupported::class.java) { apply(node) }.message!!
        assertTrue(refused(root(box(0f, """"picture":["background-image"]"""))).contains("background-image"))
        assertTrue(refused(root(box(0f, """"radius":4,"border":{"width":1,"color":{"m":4,"a":100}}"""))).contains("rounded border"))
        val split = """{"text":"redblue","x":0,"top":0,"height":18,"baseline":14,"width":56,"rtl":false,"runs":[
            {"start":0,"end":3,"x":0,"width":24,"size":16,"color":{"m":4,"a":100}},
            {"start":3,"end":7,"x":24,"width":32,"size":16,"color":{"m":5,"a":100}}]}"""
        assertTrue(refused(root(box(0f, """"lines":[$split]"""))).contains("inside a word"))
        val spaced = """{"text":"red blue","x":0,"top":0,"height":18,"baseline":14,"width":64,"rtl":false,"runs":[
            {"start":0,"end":4,"x":0,"width":32,"size":16,"color":{"m":4,"a":100}},
            {"start":4,"end":8,"x":32,"width":32,"size":16,"color":{"m":5,"a":100}}]}"""
        assertEquals(listOf("red ", "blue"), texts(apply(root(box(0f, """"lines":[$spaced]""")))))
    }
}
