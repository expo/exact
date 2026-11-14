package exact.gamepad

import android.os.VibrationEffect
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
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
@Config(sdk = [31], manifest = Config.NONE, shadows = [AmplitudeTest.Host::class])
class AmplitudeTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
        }
    }

    /** A motor asked for a magnitude below 1/255 still makes an effect the platform accepts. */
    @Test fun aSmallMagnitudeMakesAValidOneShot() {
        val module = GamepadModule(ExtensionContext(RuntimeEnvironment.getApplication(), "gamepad"))
        for (m in listOf(0.001, 0.5, 1.0)) {
            val amplitude = ReflectionHelpers.callInstanceMethod<Int>(module, "amplitude",
                ClassParameter.from(Double::class.javaPrimitiveType, m))
            VibrationEffect.createOneShot(100, amplitude)
        }
    }
}
