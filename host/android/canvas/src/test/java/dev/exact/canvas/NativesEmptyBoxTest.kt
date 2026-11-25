package dev.exact.canvas

import android.content.Context
import android.view.View
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * A module view is made whatever its box, as iOS and the web make theirs (expo/exact#1467): an app's headless holder
 * (a player, a scanner that draws elsewhere) has an empty one, and one in a subtree that paints nothing is held.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativesEmptyBoxTest {
    private val calls = ArrayList<String>()
    private val holder = object : NativeModule {
        override fun create(context: Context, props: JSONObject): View = View(context).also { calls += "create" }
        override fun update(view: View, props: JSONObject) { calls += "update ${props.optString("cue")}" }
    }

    private fun natives() = Natives(RuntimeEnvironment.getApplication(), { File(".") }, mapOf("x-holder" to holder))
    private fun element(cue: Int) =
        JSONObject().put("module", "x-holder").put("props", JSONObject().put("cue", cue).toString()).toString()

    /** The parent's own layout pass, which lays a child out by its layout params. */
    private fun layout(natives: Natives) {
        natives.measure(View.MeasureSpec.makeMeasureSpec(100, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(100, View.MeasureSpec.EXACTLY))
        natives.layout(0, 0, 100, 100)
    }

    @Test fun anEmptyBoxMakesItsViewAndGrowsAndEmptiesWithIt() {
        val natives = natives()
        natives.ensure(1, 2, 0, 0, element(0), 0)
        natives.ensure(1, 2, 0, 0, JSONObject(element(0)).put("autoWidth", 200).put("autoHeight", true).toString(), 0)
        assertEquals(listOf("create", "update 0"), calls)
        val view = natives.getChildAt(0)
        layout(natives)
        assertEquals(0 to 0, view.width to view.height)
        natives.ensure(1, 2, 0, 0, element(1), 0)
        assertEquals("update 1", calls.last())
        natives.ensure(1, 2, 40, 30, element(1), 0)
        assertEquals(40 to 30, view.width to view.height)
        natives.ensure(1, 2, 0, 0, element(1), 0)
        layout(natives)
        assertEquals(0 to 0, view.width to view.height)
        assertSame("the same view throughout", view, natives.getChildAt(0))
        assertEquals(1, calls.count { it == "create" })
    }

    @Test fun aHeldModuleIsMadeAndUpdatedAndShownAtItsBoxAgain() {
        val natives = natives()
        natives.ensure(1, 2, 0, 0, element(0), 0, hidden = true)
        assertEquals(listOf("create", "update 0"), calls)
        val view = natives.getChildAt(0)
        assertEquals("out of touch and accessibility", View.INVISIBLE, view.visibility)
        natives.ensure(1, 2, 0, 0, element(2), 0, hidden = true)
        assertEquals("update 2", calls.last())
        natives.ensure(1, 2, 40, 30, element(2), 0)
        assertEquals(View.VISIBLE, view.visibility)
        assertEquals(40 to 30, view.width to view.height)
        natives.ensure(1, 2, 0, 0, element(2), 0, hidden = true)
        assertEquals("kept at its size, hidden", 40 to View.INVISIBLE, view.width to view.visibility)
        assertEquals(1, calls.count { it == "create" })
    }

    @Test fun preferredSizesCoalesceAndAnOldBindingCannotReport() {
        val activity = org.robolectric.Robolectric.buildActivity(android.app.Activity::class.java).setup().get()
        val view = View(activity)
        activity.setContentView(view)
        val sizes = ArrayList<android.util.SizeF?>()
        var inFlight: () -> Boolean = { false }
        val binding = NativeViewSizing(view) { size, valid -> inFlight = valid; sizes += size }
        val first = android.util.SizeF(80f, 20f)
        val second = android.util.SizeF(120f, 32f)
        fun frame() = org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(32))
        binding.intrinsicSize(first)
        binding.intrinsicSize(second)
        binding.intrinsicSize(android.util.SizeF(0f, 12f))
        frame()
        assertEquals(listOf(second), sizes)
        binding.intrinsicSize(first)
        binding.intrinsicSize(second)
        frame()
        assertEquals(listOf(second), sizes)
        binding.intrinsicSize(null)
        frame()
        assertEquals(listOf(second, null), sizes)
        assertTrue(inFlight())
        val oldAttachment = inFlight
        binding.intrinsicSize(first)
        activity.setContentView(View(activity))
        binding.intrinsicSize(second)
        activity.setContentView(view)
        frame()
        assertEquals("the same view resumes its last attached sample", listOf(second, null, first), sizes)
        assertFalse(oldAttachment())
        binding.intrinsicSize(null)
        frame()
        assertEquals(listOf(second, null, first, null), sizes)
        binding.intrinsicSize(first)
        binding.retire()
        val replacement = NativeViewSizing(view) { size, _ -> sizes += size }
        binding.intrinsicSize(second)
        replacement.intrinsicSize(first)
        frame()
        assertEquals(listOf(second, null, first, null, first), sizes)
    }

    @Test fun wrapContentMeasuresAtTheAssignedWidthAndKeepsTheOtherAxisAtZeroHeight() {
        val activity = org.robolectric.Robolectric.buildActivity(android.app.Activity::class.java).setup().get()
        var heightMode = -1
        val text = object : android.widget.TextView(activity) {
            override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
                heightMode = View.MeasureSpec.getMode(heightMeasureSpec)
                super.onMeasure(widthMeasureSpec, heightMeasureSpec)
            }
        }
        text.text = "several words ".repeat(12)
        activity.setContentView(text)
        val sizes = ArrayList<android.util.SizeF?>()
        val sizing = NativeViewSizing(text) { size, _ -> sizes += size }
        sizing.wrapContent()
        val scale = text.resources.displayMetrics.density
        fun measured(width: Int, mode: Int): Pair<Int, Int> {
            text.measure(View.MeasureSpec.makeMeasureSpec(width, mode), View.MeasureSpec.UNSPECIFIED)
            return text.measuredWidth to text.measuredHeight
        }
        val (preferred, _) = measured(80, View.MeasureSpec.AT_MOST)
        val (_, expectedHeight) = measured(160, View.MeasureSpec.EXACTLY)
        sizing.offer(JSONObject().put("autoWidth", 80.0 / scale).put("autoHeight", true))
        sizing.params(160, expectedHeight)
        sizing.measure()
        fun frame() = org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(32))
        frame()
        assertEquals(android.util.SizeF(preferred / scale, expectedHeight / scale), sizes.last())
        sizing.params(160, expectedHeight / 2)
        sizing.measure()
        assertEquals("a capped auto height can shrink when content changes", View.MeasureSpec.AT_MOST, heightMode)
        assertEquals(expectedHeight / 2, text.measuredHeight)
        sizing.offer(JSONObject().put("autoWidth", 80.0 / scale).put("autoHeight", false))
        sizing.params(preferred, 0)
        sizing.measure()
        frame()
        assertEquals(preferred / scale, sizes.last()!!.width)
        assertEquals(0, text.measuredHeight)
        val (_, atZero) = measured(0, View.MeasureSpec.EXACTLY)
        sizing.offer(JSONObject().put("autoWidth", 80.0 / scale).put("autoHeight", true).put("hasPreferred", true))
        sizing.params(0, 0)
        sizing.measure()
        frame()
        assertEquals(atZero / scale, sizes.last()!!.height)
        val count = sizes.size
        sizing.offer(JSONObject().put("autoWidth", 0).put("autoHeight", true))
        sizing.params(0, 0)
        sizing.measure()
        frame()
        assertEquals("a zero measurement does not clear a live preference", count, sizes.size)
        sizing.offer(JSONObject().put("autoHeight", false))
        sizing.params(160, 50)
        sizing.measure()
        frame()
        assertEquals("two authored axes need no preferred-size report", count, sizes.size)
    }
}
