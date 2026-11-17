package exact.motion

import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [OrientationTest.Host::class])
class OrientationTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
        }
    }

    /** A phone held upright, its rotation vector a little long: the matrix's sin(beta) element rounds past 1. */
    @Test fun anUprightPhoneReadsNinetyDegreesOfBeta() {
        val module = MotionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "motion"))
        val v = floatArrayOf(0.7072f, 0f, 0f, 0.7072f)
        val angles = ReflectionHelpers.callInstanceMethod<JSONObject>(module, "orientation",
            ClassParameter.from(FloatArray::class.java, v),
            ClassParameter.from(Double::class.javaPrimitiveType, 0.0),
            ClassParameter.from(Boolean::class.javaPrimitiveType, false))
        assertEquals(90.0, angles.getDouble("beta"), 0.1)
    }
}
