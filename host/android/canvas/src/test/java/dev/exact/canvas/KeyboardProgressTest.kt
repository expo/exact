package dev.exact.canvas

import android.graphics.Insets
import android.view.WindowInsets
import android.view.WindowInsetsAnimation
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** LLP 1123 D7: the keyboard's inset animation tells its depth each frame while the keyboard moves, and its end. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class KeyboardProgressTest {
    @Test fun eachFrameOfTheKeyboardsAnimationIsToldAndItsEnd() {
        val told = mutableListOf<Int>()
        val progress = KeyboardProgress { told += it }
        val keyboard = WindowInsetsAnimation(WindowInsets.Type.ime(), null, 250)
        val bars = WindowInsetsAnimation(WindowInsets.Type.systemBars(), null, 250)
        val at = { depth: Int -> WindowInsets.Builder().setInsets(WindowInsets.Type.ime(), Insets.of(0, 0, 0, depth)).build() }
        // Halfway and then whole, as the frames run; the insets go on to the subtree unchanged
        assertEquals(at(150), progress.onProgress(at(150), mutableListOf(keyboard)))
        progress.onProgress(at(300), mutableListOf(keyboard))
        // An animation of the system bars alone says nothing of the keyboard
        progress.onProgress(at(300), mutableListOf(bars))
        progress.onEnd(bars)
        progress.onEnd(keyboard)
        assertEquals(listOf(150, 300, -1), told)
    }
}
