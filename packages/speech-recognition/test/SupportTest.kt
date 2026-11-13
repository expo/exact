package exact.speechrecognition

import android.os.Looper
import android.speech.RecognitionSupport
import android.speech.SpeechRecognizer
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
import org.robolectric.shadow.api.Shadow
import org.robolectric.shadows.ShadowSpeechRecognizer
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, shadows = [SupportTest.Host::class])
class SupportTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
        }
    }

    private fun threads() = Thread.getAllStackTraces().keys.filter { it.isAlive }.toSet()

    /** Each availability check is answered on main, and none leaves a thread behind. */
    @Test fun aSupportCheckLeavesNoThread() {
        val module = SpeechRecognitionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "speech-recognition"))
        val before = threads()
        var answered = 0
        repeat(3) {
            ReflectionHelpers.callInstanceMethod<Unit>(module, "support",
                ClassParameter.from(JSONObject::class.java, JSONObject()),
                ClassParameter.from(Function1::class.java, { s: RecognitionSupport? -> if (s != null) answered++ }))
            Shadow.extract<ShadowSpeechRecognizer>(ShadowSpeechRecognizer.getLatestSpeechRecognizer())
                .triggerSupportResult(RecognitionSupport.Builder().build())
            shadowOf(Looper.getMainLooper()).idle()
        }
        assertEquals(3, answered)
        assertEquals("threads left behind", emptySet<Thread>(), threads() - before)
    }
}
