package exact.localauth

import android.app.Activity
import android.hardware.biometrics.BiometricPrompt
import dev.exact.canvas.Exact
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import dev.exact.canvas.Extensions
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** When `authenticate` shows its prompt (LLP 1133.010 D8): over the resumed activity, which a call made while the
 * activity shows paused (a permission dialog's answer) waits for. The hatch is given each prompt's builder as it is
 * shown, so it records the prompts. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class PromptResumeTest {
    private lateinit var module: LocalAuthModule
    private val shown = ArrayList<String>()
    private val answers = ArrayList<JSONObject>()

    @Before fun setUp() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        Extensions.front = { activity }
        Extensions.started()
        Extensions.resumed()
        LocalAuthModule.hatch = { _: BiometricPrompt.Builder -> shown.add("prompt") }
        module = LocalAuthModule(ExtensionContext(RuntimeEnvironment.getApplication(), "local-auth"))
    }

    @After fun tearDown() {
        module.destroy {}
        LocalAuthModule.hatch = null
        Extensions.paused()
        Extensions.stopped(false)
        Extensions.front = { Exact.reader?.get() }
    }

    private fun authenticate(reason: String = "Unlock", request: Int? = null) {
        module.context.request = request
        module.later("authenticate", JSONObject().put("reason", reason)) { answers += it }
    }
    private val errors get() = answers.map { it.optString("error") }

    @Test fun aCallWhileTheActivityShowsPausedIsShownAtTheResume() {
        Extensions.paused()
        authenticate()
        assertEquals(emptyList<String>(), shown)
        assertEquals(emptyList<String>(), errors)
        Extensions.resumed()
        assertEquals(listOf("prompt"), shown)
        assertFalse(errors.contains("SYSTEM_CANCEL"))
    }

    @Test fun aCallWhileResumedIsShownAtOnce() {
        authenticate()
        assertEquals(listOf("prompt"), shown)
    }

    @Test fun aWaitingCallFailsSystemCancelWhenTheAppStopsFirst() {
        Extensions.paused()
        authenticate()
        Extensions.stopped(false)
        assertEquals(listOf("SYSTEM_CANCEL"), errors)
        Extensions.started()
        Extensions.resumed()
        assertEquals(emptyList<String>(), shown)
        assertEquals(1, answers.size)
    }

    @Test fun aCallWithNoActivityShowingFailsSystemCancelAtOnce() {
        Extensions.paused()
        Extensions.stopped(false)
        authenticate()
        assertEquals(listOf("SYSTEM_CANCEL"), errors)
        assertEquals(emptyList<String>(), shown)
    }

    /** A prompt the platform refuses to build (an empty title) fails SYSTEM_CANCEL, though it is built from `onResume`,
     * where no caller would catch the throw. */
    @Test fun aPromptRefusedAtTheResumeFailsSystemCancel() {
        Extensions.paused()
        authenticate(reason = "")
        Extensions.resumed()
        assertEquals(listOf("SYSTEM_CANCEL"), errors)
        assertTrue(answers.single().optString("message").startsWith("BiometricPrompt refused the prompt"))
    }

    /** A send let go while its prompt waits is answered SYSTEM_CANCEL and shows nothing at the resume; another send's
     * prompt still shows. */
    @Test fun aWaitingCallLetGoShowsNothing() {
        Extensions.paused()
        authenticate(request = 7)
        authenticate(request = 8)
        module.cancel(7)
        assertEquals(listOf("SYSTEM_CANCEL"), errors)
        Extensions.resumed()
        assertEquals(listOf("prompt"), shown)
        assertEquals(1, answers.size)
    }

    @Test fun aWaitingCallEndsWithTheSession() {
        Extensions.paused()
        authenticate()
        module.destroy {}
        assertEquals(listOf("SYSTEM_CANCEL"), errors)
        Extensions.resumed()
        assertEquals(emptyList<String>(), shown)
        assertEquals(1, answers.size)
    }
}
