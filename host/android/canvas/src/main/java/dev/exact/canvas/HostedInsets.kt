package dev.exact.canvas

import android.graphics.Insets
import android.graphics.Rect
import android.view.View
import android.view.WindowInsets

/**
 * LLP 1123 D15 on Android: the window's insets as a hosted view meets them. A hosted view (a map, a WebView, a native
 * module's view) is drawn where its element is in the page, not where the window's bars and cutout are, so each edge
 * of each inset type it is given is the part of that type's band it lies in: a view clear of the status bar sees none
 * of it, one under it as much as it covers. Its own listeners and its own consumption stay its own; the host only gives
 * it the insets it would have as a view laid out there. Activation (stage 5) dispatches them as each view's frame in
 * the window moves; tests call it until then.
 */
internal object HostedInsets {
    private val types = intArrayOf(
        WindowInsets.Type.systemBars(), WindowInsets.Type.displayCutout(), WindowInsets.Type.ime(),
        WindowInsets.Type.systemGestures(), WindowInsets.Type.mandatorySystemGestures(), WindowInsets.Type.tappableElement(),
    )

    /** `insets`, the window's, as a view whose rectangle in the window is `frame`, in a window `width` by `height`
     * pixels, meets them: on each edge of each type, the band's depth less how far in from the window's edge the view's
     * edge lies, from 0 to the band's depth. */
    fun of(insets: WindowInsets, frame: Rect, width: Int, height: Int): WindowInsets {
        val builder = WindowInsets.Builder(insets)
        for (type in types) {
            val band = insets.getInsets(type)
            builder.setInsets(type, Insets.of(
                met(band.left, frame.left), met(band.top, frame.top),
                met(band.right, width - frame.right), met(band.bottom, height - frame.bottom),
            ))
        }
        return builder.build()
    }

    /** Give `view`, whose rectangle in the window is `frame`, the window's `insets` as it meets them, through its own
     * dispatch, so its listener and its children's see them; what it returns is its own. */
    fun dispatch(view: View, insets: WindowInsets, frame: Rect, width: Int, height: Int): WindowInsets =
        view.dispatchApplyWindowInsets(of(insets, frame, width, height))

    /** The part of a band `depth` deep from the window's edge that a view whose edge lies `at` in from it covers. */
    private fun met(depth: Int, at: Int): Int = (depth - at).coerceIn(0, depth)
}
