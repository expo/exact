package dev.exact.canvas

import android.graphics.Color
import android.os.Build
import android.view.Window
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager

/** Android's LIGHT appearance flags request dark icons, independently for each bar (LLP 1105 D7); [hidden] is
 * `system-bars-hidden`, resolved as the style is. */
internal data class BarAppearance(val statusLight: Boolean, val navigationLight: Boolean, val hidden: Boolean = false)

/** Chrome word 5: bit 0 requests light navigation icons; bit 1 requests light status bar icons; bit 2 hides the bars. */
internal fun barAppearance(flags: Int) = BarAppearance(
    statusLight = flags and 2 == 0,
    navigationLight = flags and 1 == 0,
    hidden = flags and 4 != 0,
)

/** Apply resolved status/navigation bar icon appearance and optional visibility transition to [window].
 * Returns false only when API 30+ has no [WindowInsetsController] attached yet. */
internal fun applyBarAppearance(window: Window, bars: BarAppearance, hideChange: Boolean?): Boolean {
    if (Build.VERSION.SDK_INT < 30) {
        Legacy.lightBars(window, bars.statusLight, bars.navigationLight)
        hideChange?.let { Legacy.hideBars(window, it) }
        return true
    }
    val controller = window.insetsController ?: return false
    val status = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
    val navigation = WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
    controller.setSystemBarsAppearance(
        (if (bars.statusLight) status else 0) or (if (bars.navigationLight) navigation else 0),
        status or navigation,
    )
    // Immersive mode as a hand-built app asks for it: both bars go, and a swipe from an edge shows them over the
    // page for a moment. Only a change is applied, so a chrome that moves the page does not hide bars the person
    // has just swiped in.
    if (hideChange == true) {
        controller.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller.hide(WindowInsets.Type.systemBars())
    } else if (hideChange == false) {
        controller.show(WindowInsets.Type.systemBars())
    }
    return true
}

/** Configure a fullscreen modal dialog [window] edge to edge so DisplayPolicy takes both bars' light appearance from it
 * across API 29-37 without painting opaque bar backgrounds or contrast scrims. */
internal fun prepareDialogBarWindow(window: Window) {
    if (Build.VERSION.SDK_INT >= 30) {
        @Suppress("DEPRECATION")
        window.setDecorFitsSystemWindows(false)
        window.attributes = window.attributes.apply {
            layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
        }
    } else {
        Legacy.edgeToEdge(window)
    }
    window.clearFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND)
    window.addFlags(WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS)
    @Suppress("DEPRECATION")
    window.clearFlags(
        WindowManager.LayoutParams.FLAG_TRANSLUCENT_STATUS or
            WindowManager.LayoutParams.FLAG_TRANSLUCENT_NAVIGATION,
    )
    @Suppress("DEPRECATION")
    window.statusBarColor = Color.TRANSPARENT
    @Suppress("DEPRECATION")
    window.navigationBarColor = Color.TRANSPARENT
    @Suppress("DEPRECATION")
    window.isStatusBarContrastEnforced = false
    window.isNavigationBarContrastEnforced = false
}
