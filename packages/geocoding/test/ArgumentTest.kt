package exact.geocoding

import android.os.Looper
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

/** A bad argument is `TypeError` on every API level: from API 33 the listener form throws it on the caller's thread. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [32, 33], manifest = Config.NONE, shadows = [ArgumentTest.Host::class])
class ArgumentTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
        }
    }

    private fun ask(name: String, args: JSONObject): String {
        val module = GeocodingModule(ExtensionContext(RuntimeEnvironment.getApplication(), "geocoding"))
        var answer: JSONObject? = null
        module.later(name, args) { answer = it }
        // The worker (before API 33) answers on the main thread.
        for (i in 0 until 200) {
            shadowOf(Looper.getMainLooper()).idle()
            if (answer != null) break
            Thread.sleep(10)
        }
        return answer?.optString("error") ?: "no answer"
    }

    @Test fun aLatitudeOutOfRangeIsATypeError() {
        assertEquals("TypeError", ask("reverseGeocode", JSONObject().put("latitude", 200.0).put("longitude", 0.0)))
    }
}
