package dev.exact.canvas

import android.app.Activity
import android.view.MotionEvent
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * A touch on a [Pane], a surface's window or a shell bar, as exact2 hears it ([Native.surface], [Native.bar]): one
 * press from the first finger's down to the last one's lift. exact2 holds a replacement while it is down
 * (LLP 1029.000.002 D9), so a second finger is no cancel and a finger lifting with another still down no release.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class PaneTouchTest {
    private val told = ArrayList<Int>()
    private val pane by lazy {
        Pane(Robolectric.buildActivity(Activity::class.java).setup().get(), HashMap(), { action, _, _ -> told.add(action) }, null)
    }
    private var time = 0L

    private fun touch(action: Int, vararg fingers: Pair<Float, Float>, index: Int = 0) {
        val props = Array(fingers.size) { k -> MotionEvent.PointerProperties().apply { id = k; toolType = MotionEvent.TOOL_TYPE_FINGER } }
        val coords = Array(fingers.size) { k -> MotionEvent.PointerCoords().apply { x = fingers[k].first; y = fingers[k].second } }
        val e = MotionEvent.obtain(0, time, action or (index shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), fingers.size,
            props, coords, 0, 0, 1f, 1f, 0, 0, android.view.InputDevice.SOURCE_TOUCHSCREEN, 0)
        time += 10
        pane.onTouchEvent(e)
        e.recycle()
    }

    @Test fun thePressLastsFromTheFirstFingerDownToTheLastOnesLift() {
        touch(MotionEvent.ACTION_DOWN, 10f to 10f)
        touch(MotionEvent.ACTION_POINTER_DOWN, 10f to 10f, 60f to 10f, index = 1)
        touch(MotionEvent.ACTION_MOVE, 12f to 10f, 60f to 10f)
        touch(MotionEvent.ACTION_POINTER_UP, 12f to 10f, 60f to 10f, index = 0)
        assertEquals("one down, and neither a lift nor a cancel with a finger still down", listOf(0), told.filter { it != 2 })
        touch(MotionEvent.ACTION_UP, 60f to 10f)
        assertEquals(listOf(0, 1), told.filter { it != 2 })
    }

    @Test fun theSystemsCancelIsACancel() {
        touch(MotionEvent.ACTION_DOWN, 10f to 10f)
        touch(MotionEvent.ACTION_CANCEL, 10f to 10f)
        assertEquals(listOf(0, 3), told)
    }
}
