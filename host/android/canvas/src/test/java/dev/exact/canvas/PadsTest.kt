package dev.exact.canvas

import android.content.Context
import android.hardware.input.InputManager
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import java.lang.ref.WeakReference
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowInputDevice
import org.robolectric.util.ReflectionHelpers

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [PadsTest.Host::class])
class PadsTest {
    @Implements(Exact::class, isInAndroidSdk = false)
    class Host {
        @Implementation fun pad(index: Int, connected: Boolean, values: FloatArray, at: Long) {
            reports += Report(index, connected, values.toList())
            times += at
        }
        @Implementation fun hidden(hidden: Boolean) { hiddens += hidden }
        @Implementation fun drainStore(ms: Long) = true
        data class Report(val index: Int, val connected: Boolean, val values: List<Float>)
        companion object {
            val reports = ArrayList<Report>()
            val times = ArrayList<Long>()
            /** The page's visibility as each activity's start and stop told the host. */
            val hiddens = ArrayList<Boolean>()
        }
    }

    @Before fun connect() {
        Host.reports.clear()
        Host.times.clear()
        Host.hiddens.clear()
        val context = RuntimeEnvironment.getApplication()
        val manager = context.getSystemService(Context.INPUT_SERVICE) as InputManager
        val device = ShadowInputDevice.makeInputDeviceNamed("Controller")
        ReflectionHelpers.setField(device, "mId", 7)
        ReflectionHelpers.setField(device, "mDescriptor", "controller-7")
        ReflectionHelpers.setField(device, "mSources", InputDevice.SOURCE_GAMEPAD or InputDevice.SOURCE_JOYSTICK)
        ReflectionHelpers.setField(device, "mMotionRanges", ArrayList<InputDevice.MotionRange>())
        shadowOf(manager).addInputDevice(device)
        Pads.attach(context)
    }

    @After fun disconnect() {
        Exact.reader = null
        Pads.onInputDeviceRemoved(7)
    }

    private fun key(code: Int, action: Int = KeyEvent.ACTION_DOWN, repeat: Int = 0, flags: Int = 0, at: Long = 0) =
        KeyEvent(0, at, action, code, repeat, 0, 7, 0, flags, InputDevice.SOURCE_GAMEPAD)

    private fun motion(): MotionEvent {
        val coordinates = MotionEvent.PointerCoords().apply {
            setAxisValue(MotionEvent.AXIS_X, 0.75f)
            setAxisValue(MotionEvent.AXIS_HAT_Y, -1f)
        }
        return MotionEvent.obtain(0, 0, MotionEvent.ACTION_MOVE, 1,
            arrayOf(MotionEvent.PointerProperties().apply { id = 0 }), arrayOf(coordinates),
            0, 0, 1f, 1f, 7, 0, InputDevice.SOURCE_JOYSTICK, 0)
    }

    private fun activity() = Robolectric.buildActivity(ExactActivity::class.java).get().apply {
        ReflectionHelpers.setField(this, "view", ExactView(this, false) {})
        ReflectionHelpers.setField(this, "requests", Requests(this).apply { perform(intArrayOf(14, 1, 1, 1)) })
    }

    @Test fun buttonsRemainUnhandledWithoutACanvasButTheirStateIsObserved() {
        assertFalse("B must reach Android's Back fallback", Pads.key(key(KeyEvent.KEYCODE_BUTTON_B), false))
        assertEquals(1f, Pads.values(0)!![1])
        assertEquals(0, Pads.current)
        assertEquals(1f, Host.reports.last().values[1])
    }

    @Test fun canvasRoutingControlsButtonsReleasesAndRepeats() {
        val requests = Requests(Robolectric.buildActivity(ExactActivity::class.java).get())
        assertFalse(requests.padInput)
        for (takes in listOf(true, false)) {
            requests.perform(intArrayOf(14, 1, 1, if (takes) 1 else 0))
            for (code in listOf(KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_BUTTON_B, KeyEvent.KEYCODE_DPAD_LEFT)) {
                assertEquals(takes, Pads.key(key(code), requests.padInput))
                val count = Host.reports.size
                assertEquals(takes, Pads.key(key(code, repeat = 1), requests.padInput))
                assertEquals("a repeat changes no reading", count, Host.reports.size)
                assertEquals(takes, Pads.key(key(code, action = KeyEvent.ACTION_UP), requests.padInput))
            }
        }
        assertTrue(Pads.values(0)!!.all { it == 0f })
    }

    @Test fun fallbackKeysDoNotBecomePadReadingsEvenIfCanvasFocusReturns() {
        for (takes in listOf(false, true)) for (code in listOf(
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN, KeyEvent.KEYCODE_DPAD_LEFT,
            KeyEvent.KEYCODE_DPAD_RIGHT, KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_BACK,
        )) for (action in listOf(KeyEvent.ACTION_DOWN, KeyEvent.ACTION_UP)) {
            assertFalse(Pads.key(key(code, action = action, flags = KeyEvent.FLAG_FALLBACK), takes))
        }
        assertTrue(Host.reports.isEmpty())
        assertEquals(-1, Pads.current)
    }

    @Test fun remoteAndKeyboardKeysAreNotPadEvents() {
        val manager = RuntimeEnvironment.getApplication().getSystemService(Context.INPUT_SERVICE) as InputManager
        val device = ShadowInputDevice.makeInputDeviceNamed("Remote or keyboard")
        ReflectionHelpers.setField(device, "mId", 8)
        ReflectionHelpers.setField(device, "mSources", InputDevice.SOURCE_DPAD or InputDevice.SOURCE_KEYBOARD)
        shadowOf(manager).addInputDevice(device)
        for (code in listOf(KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_A)) {
            val event = KeyEvent(0, 0, KeyEvent.ACTION_DOWN, code, 0, 0, 8, 0, 0, InputDevice.SOURCE_DPAD)
            assertFalse(Pads.key(event, true))
        }
        assertTrue(Host.reports.isEmpty())
    }

    /** A press and release delivered late together keep the times the events carried (LLP 1127.003's final
     * review): a 150 ms press's sends are 150 ms apart, a 210 ms one's 210, however long they waited. */
    @Test fun aDeliveredPressKeepsItsEventTimes() {
        for (span in listOf(150L, 210L)) {
            val first = Host.times.size
            Pads.key(key(KeyEvent.KEYCODE_BUTTON_A, KeyEvent.ACTION_DOWN, at = 5000), false)
            Pads.key(key(KeyEvent.KEYCODE_BUTTON_A, KeyEvent.ACTION_UP, at = 5000 + span), false)
            assertEquals(span, Host.times.last() - Host.times[first])
        }
    }

    @Test fun motionCarriesItsEventTime() {
        val coordinates = MotionEvent.PointerCoords().apply { setAxisValue(MotionEvent.AXIS_X, 0.5f) }
        val event = MotionEvent.obtain(0, 7000, MotionEvent.ACTION_MOVE, 1,
            arrayOf(MotionEvent.PointerProperties().apply { id = 0 }), arrayOf(coordinates),
            0, 0, 1f, 1f, 7, 0, InputDevice.SOURCE_JOYSTICK, 0)
        try {
            Pads.motion(event, false)
            assertEquals(7000L, Host.times.last())
        } finally { event.recycle() }
    }

    @Test fun joystickMotionRemainsUnhandledWithoutACanvas() {
        val event = motion()
        try {
            assertFalse("Android must synthesize navigation keys", Pads.motion(event, false))
            assertEquals(0.75f, Pads.values(0)!![17])
            assertEquals(1f, Pads.values(0)!![12])
            assertTrue("a canvas consumes motion even when the reading is unchanged", Pads.motion(event, true))
            assertFalse("leaving a canvas restores navigation", Pads.motion(event, false))
        } finally { event.recycle() }
    }

    @Test fun windowFocusLossReleasesHeldReadingsUntilTheNextInput() {
        val reader = activity()
        Exact.reader = WeakReference(reader)
        val event = motion()
        try {
            assertTrue(reader.dispatchKeyEvent(key(KeyEvent.KEYCODE_BUTTON_A)))
            assertTrue(reader.dispatchGenericMotionEvent(event))
            val held = Pads.values(0)!!.toList()
            assertEquals(1f, held[0])
            assertEquals(1f, held[12])
            assertEquals(0.75f, held[17])
            val count = Host.reports.size
            reader.onWindowFocusChanged(true)
            assertEquals(count, Host.reports.size)
            assertEquals(held, Pads.values(0)!!.toList())

            reader.onWindowFocusChanged(false)
            assertFalse("focus loss does not stop the activity", ReflectionHelpers.getField(reader, "stopped"))
            assertEquals(Host.Report(0, true, List(21) { 0f }), Host.reports.last())
            assertTrue(Pads.values(0)!!.all { it == 0f })
            assertEquals(listOf(0 to 7), Pads.list().map { (slot, device) -> slot to device.id })
            assertEquals(0, Pads.current)

            val released = Host.reports.size
            reader.onWindowFocusChanged(true)
            assertTrue(reader.dispatchKeyEvent(key(KeyEvent.KEYCODE_BUTTON_A, repeat = 1)))
            assertTrue(reader.dispatchKeyEvent(key(KeyEvent.KEYCODE_BUTTON_A, action = KeyEvent.ACTION_UP)))
            assertEquals("focus gain and repeats restore no held input", released, Host.reports.size)
            assertTrue(Pads.values(0)!!.all { it == 0f })
            assertTrue(reader.dispatchGenericMotionEvent(event))
            assertEquals(0f, Pads.values(0)!![0])
            assertEquals(1f, Pads.values(0)!![12])
            assertEquals(0.75f, Pads.values(0)!![17])
            assertTrue(reader.dispatchKeyEvent(key(KeyEvent.KEYCODE_BUTTON_A)))
            assertEquals(held, Pads.values(0)!!.toList())
            assertTrue(Host.reports.all { it.index == 0 && it.connected })
        } finally { event.recycle() }
    }

    @Test fun repeatedFocusLossAndStopKeepThePadConnectedAndReleased() {
        val reader = activity()
        Exact.reader = WeakReference(reader)
        reader.dispatchKeyEvent(key(KeyEvent.KEYCODE_BUTTON_A))
        reader.onWindowFocusChanged(false)
        reader.onWindowFocusChanged(false)
        ReflectionHelpers.callInstanceMethod<Unit>(reader, "onStop")
        reader.onWindowFocusChanged(false)
        assertTrue(Pads.values(0)!!.all { it == 0f })
        assertTrue(Host.reports.all { it.index == 0 && it.connected })
        assertEquals(listOf(0 to 7), Pads.list().map { (slot, device) -> slot to device.id })
        assertEquals(0, Pads.current)

        reader.onWindowFocusChanged(true)
        reader.dispatchKeyEvent(key(KeyEvent.KEYCODE_BUTTON_B))
        assertEquals(1f, Pads.values(0)!![1])
        ReflectionHelpers.callInstanceMethod<Unit>(reader, "onStop")
        assertEquals(Host.Report(0, true, List(21) { 0f }), Host.reports.last())
    }

    @Test fun aReplacedActivitysStopLeavesTheActiveReadersPadsAndPage() {
        val replaced = activity()
        val reader = activity()
        Exact.reader = WeakReference(reader)
        reader.dispatchKeyEvent(key(KeyEvent.KEYCODE_BUTTON_A))
        val held = Pads.values(0)!!.toList()
        val count = Host.reports.size
        replaced.onWindowFocusChanged(false)
        ReflectionHelpers.callInstanceMethod<Unit>(replaced, "onStop")
        assertEquals(count, Host.reports.size)
        assertEquals(held, Pads.values(0)!!.toList())
        assertEquals("the replaced activity's stop leaves its successor's page shown", emptyList<Boolean>(), Host.hiddens)
        reader.onWindowFocusChanged(false)
        assertEquals(Host.Report(0, true, List(21) { 0f }), Host.reports.last())
        ReflectionHelpers.callInstanceMethod<Unit>(reader, "onStop")
        assertEquals("the reader's own stop hides the page once", listOf(true), Host.hiddens)
    }
}
