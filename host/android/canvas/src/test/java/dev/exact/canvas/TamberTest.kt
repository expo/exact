package dev.exact.canvas

import android.app.Activity
import android.graphics.Color
import android.os.Looper
import android.view.View
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class TamberTest {
    /** A place exact2 refused (no host here answers the first ask) that answers after a turn: the colour kept is the
     * answer, and the view's observers are told, so it is drawn again. */
    @Test fun aRefusedPlaceThatAnswersIsDrawnAgain() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val tamber = ExactTamber(View(activity)) { request ->
            if (JSONObject(request).getJSONArray("names").length() == 0) """{"inputs":"a"}"""
            else """{"inputs":"a","colors":{"accent":${Color.RED}}}"""
        }
        assertEquals(Color.BLUE, tamber.color("accent", Color.BLUE))
        var told = 0
        tamber.observe { told++ }
        tamber.check()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, told)
        assertEquals(Color.RED, tamber.color("accent", Color.BLUE))
    }
}
