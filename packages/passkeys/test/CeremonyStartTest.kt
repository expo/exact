package exact.passkeys

import android.app.Activity
import androidx.credentials.CreatePublicKeyCredentialRequest
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

/** When a ceremony starts Credential Manager's sheet: from an activity that shows, resumed or paused under a dialog
 * (a permission dialog's answer comes before `onResume`), since the sheet is an activity Android lets a visible app
 * start. The create hatch is given each request as the ceremony hands it to Credential Manager, so it records them. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class CeremonyStartTest {
    private lateinit var module: PasskeysModule
    private val started = ArrayList<String>()
    private val answers = ArrayList<JSONObject>()
    private val options = JSONObject()
        .put("rp", JSONObject().put("id", "example.com").put("name", "Example"))
        .put("user", JSONObject().put("id", "dXNlcg").put("name", "person").put("displayName", "Person"))
        .put("challenge", "Y2hhbGxlbmdl")
        .put("pubKeyCredParams", org.json.JSONArray().put(JSONObject().put("type", "public-key").put("alg", -7)))

    @Before fun setUp() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        Extensions.front = { activity }
        Extensions.started()
        Extensions.resumed()
        PasskeysModule.createRequest = { request: CreatePublicKeyCredentialRequest -> started += "create"; request }
        module = PasskeysModule(ExtensionContext(RuntimeEnvironment.getApplication(), "passkeys"))
    }

    @After fun tearDown() {
        module.destroy {}
        PasskeysModule.createRequest = null
        Extensions.paused()
        Extensions.stopped(false)
        Extensions.front = { Exact.reader?.get() }
    }

    private fun create() = module.later("create", JSONObject().put("publicKey", options)) { answers += it }
    private val messages get() = answers.map { it.optString("message") }

    @Test fun aCeremonyWhileTheActivityShowsPausedStartsAtOnce() {
        Extensions.paused()
        create()
        assertEquals(listOf("create"), started)
        assertFalse(messages.contains("the app is not in front"))
    }

    @Test fun aCeremonyWithNoActivityShowingFailsNotAllowedError() {
        Extensions.paused()
        Extensions.stopped(false)
        create()
        assertEquals(emptyList<String>(), started)
        assertEquals(listOf("NotAllowedError"), answers.map { it.optString("error") })
        assertEquals(listOf("the app is not in front"), messages)
    }
}
