package exact.gamepad

import android.os.Looper
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import dev.exact.canvas.PadDevice
import dev.exact.canvas.Pads
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter
import org.json.JSONObject

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [DisconnectTest.Host::class])
class DisconnectTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
        }
    }

    /** A pad connected before the module existed is named by its disconnect, not left with an empty id. */
    @Test fun aPadConnectedBeforeTheModuleIsNamedWhenItGoes() {
        val app = RuntimeEnvironment.getApplication()
        Pads.attach(app)
        val id = -1000
        val slots = ReflectionHelpers.getStaticField<IntArray>(Pads::class.java, "slots")
        val injected = ReflectionHelpers.getStaticField<HashMap<Int, PadDevice>>(Pads::class.java, "injected")
        injected[id] = PadDevice(id, "Test pad", 0x045E, 0x02EA, "test-0", true)
        slots[0] = id
        try {
            val module = GamepadModule(ExtensionContext(app, "gamepad"))
            shadowOf(Looper.getMainLooper()).idle()
            val gone = ReflectionHelpers.callInstanceMethod<JSONObject>(module, "gone",
                ClassParameter.from(Int::class.javaPrimitiveType, 0), ClassParameter.from(Int::class.javaPrimitiveType, id))
            assertEquals("Test pad", gone.getString("id"))
            assertEquals("xbox", gone.getString("family"))
        } finally {
            slots[0] = -1
            injected.remove(id)
        }
    }
}
