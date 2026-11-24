package exact.motion

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorManager
import android.os.SystemClock
import dev.exact.canvas.Cause
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowSensor
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [SensorClockTest.Host::class])
class SensorClockTest {
    data class Sample(val at: Double, val dropped: Int, val ended: Boolean)

    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            var clock = 0.0
            val samples = mutableListOf<Sample>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun now() = clock
            @JvmStatic @Implementation fun surfaceHas(surface: String) = surface == "ball"
            @JvmStatic @Implementation fun sensorInput(surface: String, sensor: String, values: DoubleArray,
                                                       dropped: Int, at: Double, ended: Boolean): Boolean {
                samples.add(Sample(at, dropped, ended))
                return false
            }
        }
    }

    /** A delayed delivery retains the sensor's age while a canvas receives its own 60 Hz grid. */
    @Test fun aCanvasReceivesSixtyHzSamplesAndCountsAPauseOnTheSameClock() {
        Host.samples.clear()
        Host.clock = 0.0
        val app = RuntimeEnvironment.getApplication()
        val manager = app.getSystemService(Context.SENSOR_SERVICE) as SensorManager
        val gravity = ShadowSensor.newInstance(Sensor.TYPE_GRAVITY)
        shadowOf(manager).addSensor(gravity)
        val module = MotionModule(ExtensionContext(app, "motion"))
        val args = JSONObject().put("surface", "ball").put("sensor", "gravity").put("frequency", 60)
        val result = ReflectionHelpers.callInstanceMethod<JSONObject>(module, "startInput",
            ClassParameter.from(JSONObject::class.java, args))
        assertEquals("gravity", result.getString("sensor"))
        val event = ReflectionHelpers.callConstructor(SensorEvent::class.java,
            ClassParameter.from(Int::class.javaPrimitiveType, 3))
        event.sensor = gravity
        event.values[2] = 9.81f
        fun sample(at: Double) {
            Host.clock = at + 5.0
            event.timestamp = SystemClock.elapsedRealtimeNanos() - 5_000_000
            ReflectionHelpers.callInstanceMethod<Unit>(module, "take",
                ClassParameter.from(Int::class.javaPrimitiveType, Sensor.TYPE_GRAVITY),
                ClassParameter.from(SensorEvent::class.java, event))
        }
        for (i in 1..60) sample(i * 1000.0 / 60)
        assertEquals(60, Host.samples.size)
        Host.samples.forEachIndexed { i, sample ->
            assertEquals((i + 1) * 1000.0 / 60, sample.at, 1e-6)
            assertEquals(0, sample.dropped)
        }
        Host.clock = 1005.0
        module.displayed("ball", false, Cause("app"))
        module.displayed("ball", true, Cause("app"))
        sample(1506.0)
        assertEquals(30, Host.samples.last().dropped)
        Host.clock = 1600.0
        ReflectionHelpers.callInstanceMethod<JSONObject>(module, "stopInput",
            ClassParameter.from(JSONObject::class.java, args))
        assertEquals(Sample(1600.0, 0, true), Host.samples.last())
        module.destroy {}
    }
}
