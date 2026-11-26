package dev.exact.canvas

import android.graphics.Rect
import android.view.View
import android.view.WindowInsets

/**
 * LLP 1123 D15 on Android: the window's insets as a hosted view meets them. A hosted view (a map, a WebView, a native
 * module's view) is drawn where its element is in the page, not where the window's bars and cutout are, so it is given
 * the window's insets inset by how far in from each of the window's edges it lies (`WindowInsets.inset`, from Android
 * 10), as Android gives a view laid out there: every type, its maximum and the display cutout move together, so a view
 * clear of the status bar sees none of it and one under it as much as it covers. Its own listeners and its own
 * consumption stay its own. Activation (stage 5) dispatches them as each view's frame in the window moves; tests call
 * it until then.
 */
internal object HostedInsets {
    /** `insets`, the window's, as a view whose rectangle in the window is `frame`, in a window `width` by `height`
     * pixels, meets them. */
    fun of(insets: WindowInsets, frame: Rect, width: Int, height: Int): WindowInsets = insets.inset(
        frame.left.coerceAtLeast(0), frame.top.coerceAtLeast(0),
        (width - frame.right).coerceAtLeast(0), (height - frame.bottom).coerceAtLeast(0),
    )

    /** Give `view`, whose rectangle in the window is `frame`, the window's `insets` as it meets them, through its own
     * dispatch, so its listener and its children's see them; what it returns is its own. */
    fun dispatch(view: View, insets: WindowInsets, frame: Rect, width: Int, height: Int): WindowInsets =
        view.dispatchApplyWindowInsets(of(insets, frame, width, height))
}
