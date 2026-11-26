package dev.exact.canvas

import android.graphics.Insets
import android.graphics.Rect
import android.view.View
import android.view.WindowInsets
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** LLP 1123 D15: a hosted view is given the window's insets as it meets them, through its own dispatch. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class HostedInsetsTest {
    // A 1080 by 2400 window with a 120-pixel status bar, a 60-pixel navigation bar and an 800-pixel keyboard
    private val window = WindowInsets.Builder()
        .setInsets(WindowInsets.Type.statusBars(), Insets.of(0, 120, 0, 0))
        .setInsets(WindowInsets.Type.navigationBars(), Insets.of(0, 0, 0, 60))
        .setInsets(WindowInsets.Type.ime(), Insets.of(0, 0, 0, 800))
        .build()

    @Test fun aViewIsGivenTheBandsItLiesIn() {
        val bars = WindowInsets.Type.systemBars()
        // Across the top: the status bar's whole band, none of the navigation bar's
        val top = HostedInsets.of(window, Rect(0, 0, 1080, 600), 1080, 2400)
        assertEquals(Insets.of(0, 120, 0, 0), top.getInsets(bars))
        // 50 pixels down: 70 of the status bar's 120
        assertEquals(Insets.of(0, 70, 0, 0), HostedInsets.of(window, Rect(0, 50, 1080, 650), 1080, 2400).getInsets(bars))
        // In the middle: none of either bar, and 200 of the keyboard where it reaches 1800 to the window's 1600 from the top
        val middle = HostedInsets.of(window, Rect(0, 1000, 1080, 1800), 1080, 2400)
        assertEquals(Insets.NONE, middle.getInsets(bars))
        assertEquals(Insets.of(0, 0, 0, 200), middle.getInsets(WindowInsets.Type.ime()))
    }

    @Test fun theViewsOwnListenerSeesThemAndKeepsItsReturn() {
        val view = View(RuntimeEnvironment.getApplication())
        var seen: Insets? = null
        view.setOnApplyWindowInsetsListener { _, insets -> seen = insets.getInsets(WindowInsets.Type.systemBars()); WindowInsets.CONSUMED }
        val returned = HostedInsets.dispatch(view, window, Rect(0, 2000, 1080, 2400), 1080, 2400)
        assertEquals(Insets.of(0, 0, 0, 60), seen)
        assertEquals(WindowInsets.CONSUMED, returned)
    }
}
