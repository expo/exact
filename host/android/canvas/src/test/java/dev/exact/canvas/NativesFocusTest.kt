package dev.exact.canvas

import android.app.Activity
import android.os.Looper
import android.view.KeyEvent
import android.view.View
import android.widget.Button
import android.widget.FrameLayout
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.io.File

/** A media controller hands the window's focus back to the reader's view when exact2's focus leaves it, by a key
 * the controller does not take or by exact2's own move (LLP 1042.001 D4). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class NativesFocusTest {
    /** A controller with one button, standing in for Media3's. */
    private class Player(context: android.content.Context) : VideoPlayer {
        val button = Button(context).apply { isFocusable = true; isFocusableInTouchMode = true }
        override val view: View = FrameLayout(context).apply { addView(button, 100, 50) }
        override val lendable = false
        override val engaged = false
        override val controller: View get() = view
        override fun bind(sink: MediaSink) {}
        override fun update(props: JSONObject, width: Int, height: Int, from: MediaRecord.Resume?) {}
        override fun shown(inView: Boolean) {}
        override fun snapshot(): MediaRecord.Resume? = null
        override fun stop() {}
        override fun release() {}
    }

    /** The reader's view, which takes Tab, as exact2 moves its focus by it, and no arrow, as on a phone. */
    private class Reader(context: android.content.Context) : View(context) {
        val heard = ArrayList<Int>()
        init { isFocusable = true; isFocusableInTouchMode = true }
        override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
            if (keyCode != KeyEvent.KEYCODE_TAB) return false
            heard += keyCode
            return true
        }
    }

    private class Setup(val activity: Activity, val reader: Reader, val natives: Natives, val player: Player)

    private fun setup(): Setup {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        var made: Player? = null
        val natives = Natives(activity, { File(".") }, emptyMap(), { context -> Player(context).also { made = it } })
        val reader = Reader(activity)
        natives.reader = reader
        activity.setContentView(FrameLayout(activity).apply { addView(reader, 400, 400); addView(natives, 400, 400) })
        natives.ensure(1, 0, 100, 50, props(focused = true), 0)
        shadowOf(Looper.getMainLooper()).idle()
        val player = made!!
        player.button.requestFocus()
        return Setup(activity, reader, natives, player)
    }

    private fun props(focused: Boolean) = JSONObject()
        .put("src", "https://example.com/a.mp4").put("controls", true).put("focused", focused)
        .put("generation", 1).put("slot", 1).put("session", 1).toString()

    private fun press(s: Setup, code: Int, meta: Int = 0): Boolean {
        val down = KeyEvent(0, 0, KeyEvent.ACTION_DOWN, code, 0, meta)
        return s.activity.window.decorView.dispatchKeyEvent(down)
    }

    @Test fun tabOutOfTheControllerGoesToTheReader() {
        for (meta in listOf(0, KeyEvent.META_SHIFT_ON)) {
            val s = setup()
            assertTrue(s.player.button.isFocused)
            assertTrue(press(s, KeyEvent.KEYCODE_TAB, meta))
            assertTrue(s.reader.isFocused)
            assertEquals(listOf(KeyEvent.KEYCODE_TAB), s.reader.heard)
        }
    }

    @Test fun anArrowTheReaderDoesNotTakeLeavesTheFocusInTheController() {
        val s = setup()
        assertTrue(press(s, KeyEvent.KEYCODE_DPAD_LEFT))
        assertTrue(s.player.button.isFocused)
        assertEquals(emptyList<Int>(), s.reader.heard)
    }

    @Test fun exact2MovingItsFocusAwayGivesTheReaderTheWindowsFocus() {
        val s = setup()
        s.natives.ensure(1, 0, 100, 50, props(focused = false), 0)
        assertTrue(s.reader.isFocused)
    }
}
