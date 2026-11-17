package dev.exact.canvas

import android.os.Build

/**
 * What a finger's drag of a scroller does to the software keyboard: the scroller's `keyboardDismissMode` (LLP 1115.001
 * D12; [Scrolling.Scroller.dismiss]). Unsaid, nothing, as the platform's lists leave the keyboard up.
 *
 * - `on-drag`: as the drag starts the field that holds the focus lets go of it, which exact2 hears as the person's, and
 *   the keyboard goes ([Fields.dismiss]), as UIKit's `.onDrag` resigns the first responder.
 * - `interactive`: once the finger reaches the keyboard, the keyboard follows it down and back up, and at the lift goes
 *   or comes back, as UIKit's `.interactive` has it. From Android 11 the keyboard is moved by the platform's own control
 *   of its inset animation ([android.view.WindowInsetsController.controlWindowInsetsAnimation]); it goes when the
 *   finger was moving down or had pulled it more than half away, and the field then lets go of the focus. Before
 *   Android 11 nothing moves the keyboard by hand: the finger reaching it puts it away.
 */
internal class Dismissal(private val view: ExactView) {
    /** The drag that is down follows the keyboard. */
    private var following = false
    /** From Android 11, the platform's control of the keyboard's inset (a class an older release never loads). */
    private val control = if (Build.VERSION.SDK_INT >= 30) Control(view) else null

    /** A drag of a scroller whose [mode] is [Scrolling.Scroller.dismiss]'s began. */
    fun began(mode: Int) {
        following = mode == INTERACTIVE
        if (mode == ON_DRAG) view.fields?.dismiss()
    }

    /** The finger is at [y] in the view's pixels. */
    fun moved(y: Float) {
        if (!following) return
        val at = IntArray(2).also { view.getLocationInWindow(it) }
        val finger = y + at[1]
        val c = control
        if (c != null) { c.moved(finger); return }
        val insets = view.rootWindowInsets ?: return
        @Suppress("DEPRECATION")
        val bottom = insets.systemWindowInsetBottom
        @Suppress("DEPRECATION")
        if (bottom > insets.stableInsetBottom && finger >= view.rootView.height - bottom) {
            following = false
            view.fields?.dismiss()
        }
    }

    /** The finger lifted, moving down at [velocity] px/s (up: negative), or the system took the touch (0). */
    fun lifted(velocity: Float) {
        if (!following) return
        following = false
        if (control?.lifted(velocity) == true) view.fields?.dismiss()
    }

    @android.annotation.TargetApi(30)
    private class Control(private val view: ExactView) {
        private var asking: android.os.CancellationSignal? = null
        private var held: android.view.WindowInsetsAnimationController? = null
        private var finger = 0f

        fun moved(y: Float) {
            finger = y
            held?.let { follow(it); return }
            val ime = view.rootWindowInsets?.getInsets(android.view.WindowInsets.Type.ime())?.bottom ?: return
            if (ime > 0 && asking == null && finger >= view.rootView.height - ime) ask()
        }

        /** Whether the keyboard goes. */
        fun lifted(velocity: Float): Boolean {
            asking?.cancel(); asking = null
            val c = held ?: return false
            held = null
            val shown = c.shownStateInsets.bottom
            val fling = android.view.ViewConfiguration.get(view.context).scaledMinimumFlingVelocity
            val away = velocity > fling || (velocity > -fling && c.currentInsets.bottom < shown / 2)
            c.finish(!away)
            return away
        }

        private fun ask() {
            val controller = view.windowInsetsController ?: return
            val signal = android.os.CancellationSignal()
            asking = signal
            controller.controlWindowInsetsAnimation(android.view.WindowInsets.Type.ime(), -1L,
                android.view.animation.LinearInterpolator(), signal,
                object : android.view.WindowInsetsAnimationControlListener {
                    override fun onReady(c: android.view.WindowInsetsAnimationController, types: Int) {
                        if (asking !== signal) { c.finish(true); return }
                        asking = null
                        held = c
                        follow(c)
                    }
                    override fun onFinished(c: android.view.WindowInsetsAnimationController) { if (held === c) held = null }
                    override fun onCancelled(c: android.view.WindowInsetsAnimationController?) {
                        if (held === c) held = null
                        if (asking === signal) asking = null
                    }
                })
        }

        /** The keyboard's top where the finger is, between shown and hidden. */
        private fun follow(c: android.view.WindowInsetsAnimationController) {
            if (!c.isReady) return
            val shown = c.shownStateInsets.bottom
            val hidden = c.hiddenStateInsets.bottom
            val bottom = (view.rootView.height - finger).toInt().coerceIn(hidden, shown)
            val progress = if (shown == hidden) 1f else (bottom - hidden).toFloat() / (shown - hidden)
            c.setInsetsAndAlpha(android.graphics.Insets.of(0, 0, 0, bottom), 1f, progress)
        }
    }

    companion object {
        const val ON_DRAG = 1
        const val INTERACTIVE = 2
    }
}
