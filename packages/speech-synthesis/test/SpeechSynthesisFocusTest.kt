package exact.speechsynthesis

import android.media.AudioManager
import android.os.Bundle
import android.os.Looper
import android.speech.tts.TextToSpeech
import dev.exact.canvas.AudioFocus
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowTextToSpeech
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE, shadows = [
    SpeechSynthesisFocusTest.Host::class,
    SpeechSynthesisFocusTest.FakeTts::class,
])
class SpeechSynthesisFocusTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun changed(topic: String) {}
            @JvmStatic @Implementation fun now(): Double = 0.0
        }
    }

    @Implements(TextToSpeech::class)
    class FakeTts : ShadowTextToSpeech() {
        companion object {
            val spoken = ArrayList<String>()
            fun reset() { spoken.clear() }
        }

        @Implementation override fun __constructor__(
            context: android.content.Context,
            listener: TextToSpeech.OnInitListener?,
            engine: String?,
            packageName: String?,
            useFallback: Boolean,
        ) {
            super.__constructor__(context, listener, engine, packageName, useFallback)
            listener?.onInit(TextToSpeech.SUCCESS)
        }

        @Implementation override fun speak(text: CharSequence, queueMode: Int, params: Bundle?, utteranceId: String?): Int {
            spoken += text.toString()
            return TextToSpeech.SUCCESS
        }

        @Implementation override fun stop(): Int = TextToSpeech.SUCCESS
        @Implementation override fun shutdown() {}
        @Implementation fun setSpeechRate(speechRate: Float): Int = TextToSpeech.SUCCESS
        @Implementation fun setPitch(pitch: Float): Int = TextToSpeech.SUCCESS
    }

    private lateinit var audioManager: AudioManager
    private lateinit var module: SpeechSynthesisModule

    @Before fun setUp() {
        FakeTts.reset()
        val app = RuntimeEnvironment.getApplication()
        audioManager = app.getSystemService(AudioManager::class.java)
        shadowOf(audioManager).setNextFocusRequestResponse(AudioManager.AUDIOFOCUS_REQUEST_GRANTED)
        AudioFocus.application = app
        module = SpeechSynthesisModule(ExtensionContext(app, "speech-synthesis"))
    }

    @After fun tearDown() {
        module.destroy {}
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun ask(name: String, args: JSONObject = JSONObject()): JSONObject {
        var answer: JSONObject? = null
        module.later(name, args) { answer = it }
        shadowOf(Looper.getMainLooper()).idle()
        return answer!!
    }

    private fun focusChange(change: Int) {
        ReflectionHelpers.callInstanceMethod<Unit>(AudioFocus, "changed", ClassParameter.from(Int::class.javaPrimitiveType, change))
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun reasons(state: JSONObject): List<String> {
        val arr = state.getJSONArray("pauseReasons")
        return (0 until arr.length()).map { arr.getString(it) }
    }

    @Test fun cancelWhilePausedForFocusTransientClearsReasonAndAllowsNewSpeak() {
        ask("speak", JSONObject().put("text", "first"))
        assertEquals(listOf("first"), FakeTts.spoken)

        focusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        val paused = ask("speechSynthesisState")
        assertTrue(paused.getBoolean("paused"))
        assertEquals(listOf("focus-transient"), reasons(paused))
        assertEquals("transient", paused.getString("audioSessionType"))

        val cancelled = ask("cancelSpeech")
        assertFalse(cancelled.getBoolean("paused"))
        assertEquals(emptyList<String>(), reasons(cancelled))
        assertEquals("auto", cancelled.getString("audioSessionType"))
        assertNotNull(shadowOf(audioManager).lastAbandonedAudioFocusRequest)

        ask("speak", JSONObject().put("text", "second"))
        val after = ask("speechSynthesisState")
        assertFalse(after.getBoolean("paused"))
        assertEquals(emptyList<String>(), reasons(after))
        assertTrue(after.getBoolean("speaking"))
        assertEquals("transient", after.getString("audioSessionType"))
        assertEquals(listOf("first", "second"), FakeTts.spoken)
    }

    @Test fun backgroundAndDisposeWhilePausedForFocusTransientDoNotBrickTts() {
        // 1. Backgrounding cancels a transient utterance and clears focus-transient.
        ask("speak", JSONObject().put("text", "bg-one"))
        focusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK)
        module.foreground(false)
        shadowOf(Looper.getMainLooper()).idle()
        val afterBg = ask("speechSynthesisState")
        assertEquals(emptyList<String>(), reasons(afterBg))
        assertEquals("auto", afterBg.getString("audioSessionType"))

        module.foreground(true)
        val id2 = ask("speak", JSONObject().put("text", "dispose-one")).getString("id")
        assertEquals(listOf("bg-one", "dispose-one"), FakeTts.spoken)

        // 2. Disposing the sole utterance while paused for focus-transient clears focus-transient.
        focusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        module.dispose(listOf(id2))
        shadowOf(Looper.getMainLooper()).idle()
        val afterDispose = ask("speechSynthesisState")
        assertEquals(emptyList<String>(), reasons(afterDispose))
        assertEquals("auto", afterDispose.getString("audioSessionType"))

        ask("speak", JSONObject().put("text", "after-dispose"))
        assertEquals(listOf("bg-one", "dispose-one", "after-dispose"), FakeTts.spoken)
    }

    @Test fun pauseSpeechWhileFocusTransientDropsFocusTransientSoResumeWorks() {
        ask("speak", JSONObject().put("text", "pausable"))
        focusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        val paused = ask("pauseSpeech")
        assertEquals(listOf("requested"), reasons(paused))
        assertEquals("auto", paused.getString("audioSessionType"))

        val resumed = ask("resumeSpeech")
        assertFalse(resumed.getBoolean("paused"))
        assertEquals(emptyList<String>(), reasons(resumed))
        assertEquals("transient", resumed.getString("audioSessionType"))
        assertEquals(listOf("pausable", "pausable"), FakeTts.spoken)
    }

    @Test fun cancelAllClearsFocusReasonWhileKeepingBackground() {
        ask("speak", JSONObject().put("text", "lost").put("audioSessionType", "playback"))
        focusChange(AudioManager.AUDIOFOCUS_LOSS)
        assertEquals(listOf("focus"), reasons(ask("speechSynthesisState")))

        val cancelled = ask("cancelSpeech")
        assertEquals(emptyList<String>(), reasons(cancelled))
        ask("speak", JSONObject().put("text", "restarted").put("audioSessionType", "playback"))
        assertEquals(listOf("lost", "restarted"), FakeTts.spoken)

        // Background pause reason stays set across cancelSpeech until foreground(true).
        module.foreground(false)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf("background"), reasons(ask("speechSynthesisState")))
        val cancelledBg = ask("cancelSpeech")
        assertEquals(listOf("background"), reasons(cancelledBg))
        module.foreground(true)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(emptyList<String>(), reasons(ask("speechSynthesisState")))
    }
}
