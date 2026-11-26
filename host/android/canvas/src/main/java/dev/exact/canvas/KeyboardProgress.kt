package dev.exact.canvas

import android.view.WindowInsets
import android.view.WindowInsetsAnimation

/** The software keyboard's bottom inset, in pixels, as its inset animation shows it (LLP 1123 D7): [told] each frame
 * while an animation of the keyboard runs, and -1 once it ends, so an owner's update follows the keyboard as it moves
 * where the window's inset dispatch gives only its ends. It reads the insets and changes none, so it leaves every other
 * view's dispatch as it was. */
class KeyboardProgress(private val told: (Int) -> Unit) :
    WindowInsetsAnimation.Callback(DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
    override fun onProgress(insets: WindowInsets, running: MutableList<WindowInsetsAnimation>): WindowInsets {
        if (running.any { it.typeMask and WindowInsets.Type.ime() != 0 }) told(insets.getInsets(WindowInsets.Type.ime()).bottom)
        return insets
    }

    override fun onEnd(animation: WindowInsetsAnimation) {
        if (animation.typeMask and WindowInsets.Type.ime() != 0) told(-1)
    }
}
