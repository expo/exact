package dev.exact.canvas

import android.app.Activity
import android.content.res.Configuration
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** A picker is drawn in its control's scheme, which can differ from the device's (expo/exact#1469). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, qualifiers = "notnight")
class PickerSchemeTest {
    private fun night(c: android.content.Context) =
        c.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK

    @Test fun aPickersContextIsInItsControlsScheme() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        assertEquals(Configuration.UI_MODE_NIGHT_NO, night(activity))
        assertSame("the device's scheme: the activity itself", activity, schemed(activity, null))
        assertEquals(Configuration.UI_MODE_NIGHT_YES, night(schemed(activity, true)))
        assertEquals(Configuration.UI_MODE_NIGHT_NO, night(schemed(activity, false)))
        // A dialog made from it opens, in that scheme.
        val dialog = android.app.DatePickerDialog(schemed(activity, true), null, 2026, 9, 16)
        dialog.show()
        assertEquals(Configuration.UI_MODE_NIGHT_YES, night(dialog.context))
        dialog.dismiss()
    }
}
