package exact.passkeys

import android.app.Activity
import android.os.CancellationSignal
import android.widget.EditText
import androidx.credentials.CreateCredentialResponse
import androidx.credentials.CreatePublicKeyCredentialRequest
import androidx.credentials.CreatePublicKeyCredentialResponse
import androidx.credentials.CredentialManager
import androidx.credentials.CredentialManagerCallback
import androidx.credentials.GetCredentialResponse
import androidx.credentials.PublicKeyCredential
import androidx.credentials.exceptions.CreateCredentialException
import androidx.credentials.pendingGetCredentialRequest
import dev.exact.canvas.Cause
import dev.exact.canvas.Exact
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import dev.exact.canvas.Extensions
import java.lang.reflect.Proxy
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
import org.robolectric.util.ReflectionHelpers

/** When a ceremony starts Credential Manager's sheet: from an activity that shows, resumed or paused under a dialog
 * (a permission dialog's answer comes before `onResume`), since the sheet is an activity Android lets a visible app
 * start. The create hatch is given each request as the ceremony hands it to Credential Manager, so it records them. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class CeremonyStartTest {
    private lateinit var activity: Activity
    private lateinit var module: PasskeysModule
    private val started = ArrayList<String>()
    private val answers = ArrayList<JSONObject>()
    private val options = JSONObject()
        .put("rp", JSONObject().put("id", "example.com").put("name", "Example"))
        .put("user", JSONObject().put("id", "dXNlcg").put("name", "person").put("displayName", "Person"))
        .put("challenge", "Y2hhbGxlbmdl")
        .put("pubKeyCredParams", org.json.JSONArray().put(JSONObject().put("type", "public-key").put("alg", -7)))
    private val getOptions = JSONObject()
        .put("rpId", "example.com")
        .put("challenge", "Y2hhbGxlbmdl")
    private val assertionJson = JSONObject()
        .put("id", "cred-1")
        .put("rawId", "cred-1")
        .put("type", "public-key")
        .put("response", JSONObject()
            .put("clientDataJSON", "e30")
            .put("authenticatorData", "AAAA")
            .put("signature", "BBBB")
            .put("userHandle", "dXNlcg"))
        .toString()

    @Before fun setUp() {
        activity = Robolectric.buildActivity(Activity::class.java).setup().get()
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

    private fun create(request: Int? = null) =
        module.later("create", JSONObject().put("publicKey", options), null, Cause("app", request = request)) { answers += it }
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

    @Test @Config(sdk = [35]) fun staleConditionalCallbackLeavesReplacementActiveAndAnswerable() {
        val field = EditText(activity)
        module.webauthnField(field, true)
        val firstAnswers = ArrayList<JSONObject>()
        val secondAnswers = ArrayList<JSONObject>()

        module.later("get", JSONObject().put("publicKey", getOptions).put("mediation", "conditional"), null, Cause("app", request = 1)) { firstAnswers += it }
        val pending1 = field.pendingGetCredentialRequest
        assertNotNull(pending1)

        module.later("get", JSONObject().put("publicKey", getOptions).put("mediation", "conditional"), null, Cause("app", request = 2)) { secondAnswers += it }
        val pending2 = field.pendingGetCredentialRequest
        assertNotNull(pending2)
        assertNotSame(pending1, pending2)
        assertEquals(listOf("AbortError"), firstAnswers.map { it.optString("error") })

        // A late autofill callback from the superseded pending1 must not clear or detach pending2.
        pending1!!.callback.invoke(GetCredentialResponse(PublicKeyCredential(assertionJson)))
        assertSame(pending2, field.pendingGetCredentialRequest)
        assertEquals(1, firstAnswers.size)
        assertTrue(secondAnswers.isEmpty())

        // The replacement pending2 remains attached and settles normally when picked.
        pending2!!.callback.invoke(GetCredentialResponse(PublicKeyCredential(assertionJson)))
        assertNull(field.pendingGetCredentialRequest)
        assertEquals(1, secondAnswers.size)
        assertEquals("cred-1", secondAnswers.single().optString("id"))
    }

    @Test @Config(sdk = [35]) fun cancelDetachesMatchingConditionalGetAndIgnoresOtherRequests() {
        val field = EditText(activity)
        module.webauthnField(field, true)
        val replies = ArrayList<JSONObject>()
        module.later("get", JSONObject().put("publicKey", getOptions).put("mediation", "conditional"), null, Cause("app", request = 10)) { replies += it }
        assertNotNull(field.pendingGetCredentialRequest)

        module.cancel(99)
        assertNotNull(field.pendingGetCredentialRequest)
        assertTrue(replies.isEmpty())

        module.cancel(10)
        assertNull(field.pendingGetCredentialRequest)
        assertEquals(listOf("AbortError"), replies.map { it.optString("error") })
        assertEquals(listOf("the call was let go"), replies.map { it.optString("message") })
    }

    @Test @Config(sdk = [35]) fun registrationFinishingWithItsActivityIsDisposed() {
        lateinit var callback: CredentialManagerCallback<CreateCredentialResponse, CreateCredentialException>
        var disposals = 0
        val manager = Proxy.newProxyInstance(CredentialManager::class.java.classLoader, arrayOf(CredentialManager::class.java)) { _, method, args ->
            when (method.name) {
                "createCredentialAsync" -> {
                    @Suppress("UNCHECKED_CAST")
                    callback = args.filterIsInstance<CredentialManagerCallback<*, *>>().single() as
                        CredentialManagerCallback<CreateCredentialResponse, CreateCredentialException>
                }
                "signalCredentialStateAsync" -> disposals++
            }
            null
        } as CredentialManager
        ReflectionHelpers.setField(module, "manager\$delegate", lazyOf(manager))
        create(41)
        activity.finish()
        callback.onResult(CreatePublicKeyCredentialResponse(JSONObject().put("id", "Y3JlZA").toString()))
        assertEquals(listOf("AbortError"), answers.map { it.optString("error") })
        assertEquals(1, disposals)
    }

    @Test fun cancelCancelsMatchingModalCancellationSignal() {
        val captured = ArrayList<CancellationSignal>()
        val fakeManager = Proxy.newProxyInstance(
            CredentialManager::class.java.classLoader,
            arrayOf(CredentialManager::class.java),
        ) { _, method, args ->
            if (method.name == "createCredentialAsync" || method.name == "getCredentialAsync") {
                args?.filterIsInstance<CancellationSignal>()?.let { captured.addAll(it) }
            }
            null
        } as CredentialManager
        ReflectionHelpers.setField(module, "manager\$delegate", lazyOf(fakeManager))

        create(request = 41)
        module.later("get", JSONObject().put("publicKey", getOptions), null, Cause("app", request = 42)) { answers += it }
        val s41 = captured.single()
        module.cancel(99)
        assertFalse(s41.isCanceled)
        assertEquals(1, captured.size)

        module.cancel(41)
        assertTrue(s41.isCanceled)
        assertEquals(2, captured.size)
        val s42 = captured.last()
        assertFalse(s42.isCanceled)

        module.cancel(42)
        assertTrue(s42.isCanceled)
    }
}
