package exact.languagemodel

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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [BackgroundTest.Host::class])
class BackgroundTest {
    /** The agent model on a session clock that stands still, so a response generates until something ends it. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = true
            @JvmStatic @Implementation fun changed(topic: String) {}
            @JvmStatic @Implementation fun now() = 0.0
        }
    }

    private val context = ExtensionContext(RuntimeEnvironment.getApplication(), "language-model")
    private val module = LanguageModelModule(context)

    private fun call(name: String, args: JSONObject): JSONObject {
        var answer: JSONObject? = null
        module.later(name, args) { answer = it }
        return answer!!
    }

    /** Leaving the foreground fails the queued prompt as well as the generating one, and begins neither again. */
    @Test fun leavingTheForegroundFailsAQueuedPrompt() {
        val session = call("createLanguageModel", JSONObject()).getString("id")
        val first = call("promptStreaming", JSONObject().put("session", session).put("input", "one")).getString("id")
        val queued = call("promptStreaming", JSONObject().put("session", session).put("input", "two")).getString("id")

        context.foreground = false
        module.foreground(false)

        for (id in listOf(first, queued)) {
            val r = call("languageModelResponse", JSONObject().put("response", id))
            assertEquals(id, "failed", r.getString("state"))
            assertEquals(id, "NotAllowedError", r.getString("error"))
            assertEquals(id, "background", r.getString("reason"))
        }
        assertEquals("the queued prompt never reached the session", listOf("one"), module.sessions.getValue(session).turns.map { it.input })
    }
}
