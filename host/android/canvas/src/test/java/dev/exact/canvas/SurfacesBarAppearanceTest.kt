package dev.exact.canvas

import android.app.Dialog
import android.graphics.Color
import android.graphics.Insets
import android.graphics.drawable.ColorDrawable
import android.os.Build
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import android.widget.FrameLayout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.File

@RunWith(RobolectricTestRunner::class)
class SurfacesBarAppearanceTest {
    private fun makeActivity(): Pair<ExactActivity, Surfaces> {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        val root = Root(activity)
        activity.setContentView(root)
        val surfaces = Surfaces(activity, root)
        val requests = Requests(activity).also { it.surfaces = surfaces }
        ExactActivity::class.java.getDeclaredField("requests").apply {
            isAccessible = true
            set(activity, requests)
        }
        ExactActivity::class.java.getDeclaredField("view").apply {
            isAccessible = true
            set(activity, ExactView(activity, false) {})
        }
        ExactActivity::class.java.getDeclaredField("natives").apply {
            isAccessible = true
            set(activity, Natives(activity, { File(".") }, emptyMap()))
        }
        return activity to surfaces
    }

    /** Invoke production `ExactActivity.chrome(IntArray)` with bar-appearance flags in word 5. */
    private fun callChrome(activity: ExactActivity, flags: Int, bg: Int = Color.WHITE) {
        val method = ExactActivity::class.java.getDeclaredMethod("chrome", IntArray::class.java)
        method.isAccessible = true
        method.invoke(activity, intArrayOf(0, 0, 360, 640, bg, flags))
    }

    private fun openDialog(surfaces: Surfaces, flags: Int = Surfaces.BACK_DISMISSES): Dialog {
        surfaces.told(intArrayOf(1, Surfaces.ALERT, flags, 0, 0, 280, 180, 0, 0, 0), 0)
        assertTrue(surfaces.open)
        val shown = Surfaces::class.java.getDeclaredField("shown").apply { isAccessible = true }.get(surfaces)!!
        return shown.javaClass.getDeclaredField("dialog").apply { isAccessible = true }.get(shown) as Dialog
    }

    private fun closeDialog(surfaces: Surfaces) {
        surfaces.told(intArrayOf(1, Surfaces.CLOSED, 0, 0, 0, 0, 0, 0, 0, 0), 0)
    }

    @Test
    @Config(sdk = [33], manifest = Config.NONE)
    fun alertDialogUsesFullscreenNonDimmingWindowAndTracksChromeOnApi33() {
        val (activity, surfaces) = makeActivity()

        // flags = 0 -> dark status icons (statusLight = true) and dark nav icons (navigationLight = true).
        callChrome(activity, 0)
        val dialog = openDialog(surfaces)
        val window = dialog.window!!
        assertFalse("non-floating dialog covers both system bars", window.isFloating)
        assertEquals(0, window.attributes.flags and WindowManager.LayoutParams.FLAG_DIM_BEHIND)
        assertEquals(ViewGroup.LayoutParams.MATCH_PARENT, window.attributes.width)
        assertEquals(ViewGroup.LayoutParams.MATCH_PARENT, window.attributes.height)
        val backdrop = window.decorView.findViewById<ViewGroup>(android.R.id.content).getChildAt(0) as FrameLayout
        assertEquals(View.IMPORTANT_FOR_ACCESSIBILITY_NO, backdrop.importantForAccessibility)
        assertEquals(Color.argb(Math.round(Surfaces.SCRIM * 255f), 0, 0, 0), (backdrop.background as ColorDrawable).color)
        assertEquals(280, backdrop.getChildAt(0).layoutParams.width)
        assertEquals(180, backdrop.getChildAt(0).layoutParams.height)

        val controller = window.insetsController!!
        val status = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
        val nav = WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
        assertEquals(status or nav, controller.systemBarsAppearance and (status or nav))

        // Resizing the dialog updates the centered pane without shrinking the fullscreen window.
        surfaces.told(intArrayOf(1, Surfaces.ALERT, Surfaces.BACK_DISMISSES, 0, 0, 300, 220, 0, 0, 0), 0)
        assertEquals(300, backdrop.getChildAt(0).layoutParams.width)
        assertEquals(220, backdrop.getChildAt(0).layoutParams.height)
        assertEquals(ViewGroup.LayoutParams.MATCH_PARENT, window.attributes.width)
        assertEquals(ViewGroup.LayoutParams.MATCH_PARENT, window.attributes.height)

        // Live production chrome updates while dialog is open.
        callChrome(activity, 2) // light status icons, dark nav icons
        assertEquals(nav, controller.systemBarsAppearance and (status or nav))
        callChrome(activity, 3) // both light icons
        assertEquals(0, controller.systemBarsAppearance and (status or nav))
        callChrome(activity, 1) // dark status icons, light nav icons
        assertEquals(status, controller.systemBarsAppearance and (status or nav))
        closeDialog(surfaces)
    }

    @Test
    @Config(sdk = [29], manifest = Config.NONE)
    @Suppress("DEPRECATION")
    fun alertDialogUsesFullscreenEdgeToEdgeWindowAndTracksLegacyFlagsOnApi29() {
        val (activity, surfaces) = makeActivity()

        callChrome(activity, 0)
        val dialog = openDialog(surfaces)
        val window = dialog.window!!
        assertFalse(window.isFloating)
        assertEquals(0, window.attributes.flags and WindowManager.LayoutParams.FLAG_DIM_BEHIND)
        assertNotEquals(0, window.attributes.flags and WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS)
        val edgeMask = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        assertEquals(edgeMask, window.decorView.systemUiVisibility and edgeMask)
        val lightMask = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
        assertEquals(lightMask, window.decorView.systemUiVisibility and lightMask)

        // Switch to light status icons + dark nav icons while open.
        callChrome(activity, 2)
        assertEquals(View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR, window.decorView.systemUiVisibility and lightMask)

        // Hide system bars while open; unrelated chrome with same hidden=true must not re-hide transiently swiped bars.
        callChrome(activity, 4)
        assertNotEquals(0, window.decorView.systemUiVisibility and View.SYSTEM_UI_FLAG_FULLSCREEN)
        window.decorView.systemUiVisibility = window.decorView.systemUiVisibility and View.SYSTEM_UI_FLAG_FULLSCREEN.inv()
        callChrome(activity, 4)
        assertEquals(0, window.decorView.systemUiVisibility and View.SYSTEM_UI_FLAG_FULLSCREEN)
        callChrome(activity, 0)
        assertEquals(0, window.decorView.systemUiVisibility and View.SYSTEM_UI_FLAG_FULLSCREEN)
        closeDialog(surfaces)
    }

    @Test
    @Config(sdk = [33], manifest = Config.NONE)
    fun alertDialogHandlesOutsideTapAccordingToFlagsAndAdoptsLateChrome() {
        assertEquals(33, Build.VERSION.SDK_INT)
        val (activity, surfaces) = makeActivity()

        // Open before first chrome with OUTSIDE_DISMISSES off -> outside tap must NOT dismiss.
        val modal = openDialog(surfaces, Surfaces.BACK_DISMISSES)
        val modalBackdrop = modal.window!!.decorView.findViewById<ViewGroup>(android.R.id.content).getChildAt(0)
        val down = MotionEvent.obtain(0, 0, MotionEvent.ACTION_DOWN, 5f, 5f, 0)
        assertTrue(modalBackdrop.onTouchEvent(down))
        assertTrue(surfaces.open)
        assertTrue(modal.isShowing)
        down.recycle()

        // Late chrome arrives while open and sets dark status bar icons.
        callChrome(activity, 0)
        val status = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
        assertEquals(status, modal.window!!.insetsController!!.systemBarsAppearance and status)
        closeDialog(surfaces)

        // Open with OUTSIDE_DISMISSES -> tapping outside pane cancels and closes, while tapping inside pane stays open.
        val cancellable = openDialog(surfaces, Surfaces.BACK_DISMISSES or Surfaces.OUTSIDE_DISMISSES)
        val cancelBackdrop = cancellable.window!!.decorView.findViewById<ViewGroup>(android.R.id.content).getChildAt(0) as FrameLayout
        cancelBackdrop.measure(
            View.MeasureSpec.makeMeasureSpec(360, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(640, View.MeasureSpec.EXACTLY),
        )
        cancelBackdrop.layout(0, 0, 360, 640)
        // Intercept Pane's touch before JNI Exact.surface while still exercising FrameLayout hit-testing.
        var insideHits = 0
        cancelBackdrop.getChildAt(0).setOnTouchListener { _, event ->
            if (event.actionMasked == MotionEvent.ACTION_DOWN) insideHits++
            true
        }
        // Center (180, 320) hits the centered 280x180 pane and must NOT dismiss.
        val insideDown = MotionEvent.obtain(0, 0, MotionEvent.ACTION_DOWN, 180f, 320f, 0)
        val insideUp = MotionEvent.obtain(0, 10, MotionEvent.ACTION_UP, 180f, 320f, 0)
        assertTrue(cancelBackdrop.dispatchTouchEvent(insideDown))
        assertTrue(cancelBackdrop.dispatchTouchEvent(insideUp))
        insideDown.recycle()
        insideUp.recycle()
        assertEquals(1, insideHits)
        assertTrue(cancellable.isShowing)

        // Outside press alone, outside-to-inside release, or canceled outside press must NOT dismiss.
        val dragDown = MotionEvent.obtain(0, 20, MotionEvent.ACTION_DOWN, 5f, 5f, 0)
        val dragUpInside = MotionEvent.obtain(0, 30, MotionEvent.ACTION_UP, 180f, 320f, 0)
        assertTrue(cancelBackdrop.dispatchTouchEvent(dragDown))
        assertTrue(cancellable.isShowing)
        assertTrue(cancelBackdrop.dispatchTouchEvent(dragUpInside))
        assertTrue(cancellable.isShowing)
        dragDown.recycle()
        dragUpInside.recycle()

        val cancelDown = MotionEvent.obtain(0, 40, MotionEvent.ACTION_DOWN, 5f, 5f, 0)
        val cancelEvent = MotionEvent.obtain(0, 50, MotionEvent.ACTION_CANCEL, 5f, 5f, 0)
        assertTrue(cancelBackdrop.dispatchTouchEvent(cancelDown))
        assertTrue(cancelBackdrop.dispatchTouchEvent(cancelEvent))
        assertTrue(cancellable.isShowing)
        cancelDown.recycle()
        cancelEvent.recycle()

        // Complete outside tap (DOWN + UP outside pane) cancels the dialog.
        val tapDown = MotionEvent.obtain(0, 60, MotionEvent.ACTION_DOWN, 5f, 5f, 0)
        val tapUp = MotionEvent.obtain(0, 70, MotionEvent.ACTION_UP, 5f, 5f, 0)
        assertTrue(cancelBackdrop.dispatchTouchEvent(tapDown))
        assertTrue(cancellable.isShowing)
        assertTrue(cancelBackdrop.dispatchTouchEvent(tapUp))
        tapDown.recycle()
        tapUp.recycle()
        assertEquals(1, insideHits)
        assertFalse(cancellable.isShowing)
    }

    @Test
    @Config(sdk = [33], manifest = Config.NONE)
    fun alertDialogCentersPaneWithinSafeAreaWhileScrimStaysFullscreen() {
        val (_, surfaces) = makeActivity()
        // 728px tall pane in an 800px window with 24px status + 48px nav bar fits cleanly inside safe area.
        surfaces.told(intArrayOf(1, Surfaces.ALERT, Surfaces.BACK_DISMISSES, 0, 0, 280, 728, 0, 0, 0), 0)
        val shown = Surfaces::class.java.getDeclaredField("shown").apply { isAccessible = true }.get(surfaces)!!
        val dialog = shown.javaClass.getDeclaredField("dialog").apply { isAccessible = true }.get(shown) as Dialog
        val backdrop = dialog.window!!.decorView.findViewById<ViewGroup>(android.R.id.content).getChildAt(0) as FrameLayout
        val insets = WindowInsets.Builder()
            .setInsets(WindowInsets.Type.systemBars(), Insets.of(0, 24, 0, 48))
            .build()
        backdrop.dispatchApplyWindowInsets(insets)
        backdrop.measure(
            View.MeasureSpec.makeMeasureSpec(360, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(800, View.MeasureSpec.EXACTLY),
        )
        backdrop.layout(0, 0, 360, 800)
        val pane = backdrop.getChildAt(0)
        assertEquals(0, backdrop.top)
        assertEquals(800, backdrop.bottom)
        assertEquals(24, pane.top)
        assertEquals(752, pane.bottom)
        closeDialog(surfaces)
    }
}
