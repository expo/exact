package dev.exact.canvas

import android.app.Activity
import android.hardware.display.DisplayManager
import android.os.PowerManager
import android.view.View
import android.view.Display
import java.io.File
import org.junit.After
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowDisplayManager
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE, shadows = [HostFactsTest.Frames::class])
class HostFactsTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Frames {
        companion object {
            var tick = emptyList<Long>()
            var buffered = 0
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun displayFrame(vsync: Long, arrival: Long, fastest: Long, thermal: Int, period: Long) {
                tick = listOf(vsync, arrival, fastest, thermal.toLong(), period)
            }
            @JvmStatic @Implementation fun attachBuffers(view: Int, width: Int, height: Int) { buffered = view }
            @JvmStatic @Implementation fun detachSurface(view: Int, window: Long) { if (view == buffered) buffered = 0 }
        }
    }

    private fun tick(period: Long = 0): List<Long> {
        HostFacts.turned(100, 105, period)
        return Frames.tick
    }

    private val context get() = RuntimeEnvironment.getApplication()
    private val power get() = shadowOf(context.getSystemService(PowerManager::class.java))

    private val views by lazy { List(3) { View(context) } }

    @After fun detach() { views.forEach(HostFacts::detached) }

    @Test fun androidsSevenStatusesGiveTheCanvasItsFourThermalLevels() {
        val codes = listOf(
            PowerManager.THERMAL_STATUS_NONE to 1, PowerManager.THERMAL_STATUS_LIGHT to 2,
            PowerManager.THERMAL_STATUS_MODERATE to 3, PowerManager.THERMAL_STATUS_SEVERE to 3,
            PowerManager.THERMAL_STATUS_CRITICAL to 4, PowerManager.THERMAL_STATUS_EMERGENCY to 4,
            PowerManager.THERMAL_STATUS_SHUTDOWN to 4,
        )
        for ((status, code) in codes) assertEquals("status $status", code, HostFacts.thermalCode(status))
        assertEquals(0, HostFacts.thermalCode(-1))
    }

    @Test fun theListenerLivesWhileACanvasHasAWindowAndFollowsTheStatus() {
        power.setCurrentThermalStatus(PowerManager.THERMAL_STATUS_SEVERE)
        HostFacts.attached(views[1])
        HostFacts.attached(views[2])
        assertEquals("read at once, not at the first change", 3, tick()[3].toInt())
        assertEquals(1, power.thermalStatusListeners.size)
        power.setCurrentThermalStatus(PowerManager.THERMAL_STATUS_LIGHT)
        assertEquals(2, tick()[3].toInt())
        val old = power.thermalStatusListeners.single() as PowerManager.OnThermalStatusChangedListener
        HostFacts.detached(views[1])
        assertEquals("another canvas still has one", 1, power.thermalStatusListeners.size)
        HostFacts.detached(views[2])
        assertTrue(power.thermalStatusListeners.isEmpty())
        assertEquals(0, tick()[3].toInt())
        power.setCurrentThermalStatus(PowerManager.THERMAL_STATUS_CRITICAL)
        assertEquals("a listener removed hears nothing", 0, tick()[3].toInt())
        HostFacts.attached(views[0])
        old.onThermalStatusChanged(PowerManager.THERMAL_STATUS_LIGHT)
        assertEquals(4, tick()[3].toInt())
    }

    @Test @Config(sdk = [29])
    fun aBufferedCanvasKeepsItsTickOutsideTheVisibleRowsAndReleasesTheLastListener() {
        val activity = Robolectric.buildActivity(Activity::class.java).get()
        val natives = Natives(activity, { File(".") }, emptyMap())
        try {
            power.setCurrentThermalStatus(PowerManager.THERMAL_STATUS_SEVERE)
            natives.ensure(17, 3, 64, 64, "{}", 1)
            natives.near(emptySet(), emptySet(), setOf(1))
            assertEquals(17, Frames.buffered)
            assertEquals(3, tick()[3].toInt())
        } finally { natives.destroyAll() }
        assertEquals(0, Frames.buffered)
        assertTrue(power.thermalStatusListeners.isEmpty())
    }

    /** `Display.Mode`'s constructor is hidden from apps. */
    private fun mode(id: Int, w: Int, h: Int, hz: Float): Display.Mode = ReflectionHelpers.callConstructor(
        Display.Mode::class.java, ClassParameter.from(Int::class.javaPrimitiveType, id),
        ClassParameter.from(Int::class.javaPrimitiveType, w), ClassParameter.from(Int::class.javaPrimitiveType, h),
        ClassParameter.from(Float::class.javaPrimitiveType, hz),
    )

    @Test fun theFastestPeriodIsTheFastestModeAtTheCurrentResolution() {
        val display = context.getSystemService(android.hardware.display.DisplayManager::class.java).getDisplay(Display.DEFAULT_DISPLAY)
        val w = display.mode.physicalWidth
        val h = display.mode.physicalHeight
        ShadowDisplayManager.setSupportedModes(
            Display.DEFAULT_DISPLAY, display.mode, mode(90, w, h, 120f), mode(91, w / 2, h / 2, 144f),
        )
        HostFacts.attached(views[0])
        val fastest = Math.round(1e9 / 120)
        assertEquals(listOf(100L, 105L, fastest, 1L, 20_000_000L), tick(20_000_000L))
        val displays = context.getSystemService(DisplayManager::class.java)
        val second = ShadowDisplayManager.addDisplay("w230dp-h320dp-mdpi")
        try {
            val external = displays.getDisplay(second)
            HostFacts.onScreen(external)
            assertEquals(Math.round(1e9 / external.mode.refreshRate), tick()[2])
            assertEquals(Math.round(1e9 / external.refreshRate), tick()[4])
        } finally { ShadowDisplayManager.removeDisplay(second) }
    }
}
