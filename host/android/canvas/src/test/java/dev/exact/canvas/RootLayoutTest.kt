package dev.exact.canvas

import android.content.pm.ApplicationInfo
import android.content.res.Configuration
import android.graphics.Rect
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.PopupWindow
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.util.Locale

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, qualifiers = "ar-rEG-ldrtl-mdpi")
class RootLayoutTest {
    private fun activity(): ExactActivity = Robolectric.buildActivity(ExactActivity::class.java).get().also {
        it.applicationInfo.flags = it.applicationInfo.flags or ApplicationInfo.FLAG_SUPPORTS_RTL
        it.window.decorView.layoutDirection = View.LAYOUT_DIRECTION_RTL
    }

    private fun layout(view: View) {
        view.measure(View.MeasureSpec.makeMeasureSpec(360, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(640, View.MeasureSpec.EXACTLY))
        view.layout(0, 0, 360, 640)
    }

    @Test fun rtlLocaleKeepsTheRefreshDiscInItsPhysicalClipAcrossConfigurationChanges() {
        val activity = activity()
        assertEquals(View.LAYOUT_DIRECTION_RTL, activity.resources.configuration.layoutDirection)
        val root = Root(activity)
        activity.setContentView(root)
        val refresh = Refresh(activity)
        root.addView(refresh, FrameLayout.LayoutParams(200, 300))
        refresh.translationX = 60f
        refresh.pull(160f)
        for (locale in listOf(Locale("ar"), Locale.ENGLISH, Locale("ur"))) {
            val config = Configuration(activity.resources.configuration).apply { setLocale(locale) }
            root.dispatchConfigurationChanged(config)
            activity.window.decorView.layoutDirection = config.layoutDirection
            root.requestLayout()
            layout(root)
            val disc = refresh.getChildAt(0)
            assertEquals("the disc stays centered inside its clip", 80f, disc.x, 0f)
            assertEquals("the refresh container's physical x", 60f, refresh.x, 0f)
            assertEquals(40f, disc.y, 0f)
            assertEquals(View.LAYOUT_DIRECTION_LTR, root.layoutDirection)
        }
    }

    @Test fun rtlLocaleKeepsTheMenusPhysicalX() {
        val activity = activity()
        val root = Root(activity)
        activity.setContentView(root)
        layout(root)
        val surfaces = Surfaces(activity, root)
        surfaces.told(intArrayOf(1, Surfaces.MENU, 0, 60, 100, 120, 80, 0, 0, 0), 0)
        assertTrue(surfaces.open)
        val shown = Surfaces::class.java.getDeclaredField("shown").apply { isAccessible = true }.get(surfaces)!!
        val popup = shown.javaClass.getDeclaredField("popup").apply { isAccessible = true }.get(shown) as PopupWindow
        val params = popup.contentView.rootView.layoutParams as WindowManager.LayoutParams
        val placed = Rect()
        Gravity.apply(params.gravity, params.width, params.height, Rect(0, 0, 360, 640),
            params.x, params.y, placed, View.LAYOUT_DIRECTION_RTL)
        assertEquals("the window manager uses the presenter's physical x", 60, placed.left)
        surfaces.told(intArrayOf(1, Surfaces.CLOSED, 0, 0, 0, 0, 0, 0, 0, 0), 0)
    }
}
