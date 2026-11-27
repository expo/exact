package exact.signin

import android.app.Activity
import android.content.res.Configuration
import android.view.View
import android.widget.TextView
import androidx.credentials.CredentialManager
import androidx.credentials.CredentialOption
import androidx.credentials.CredentialManagerCallback
import androidx.credentials.GetCredentialRequest
import androidx.credentials.GetCredentialResponse
import androidx.credentials.exceptions.GetCredentialException
import androidx.credentials.exceptions.NoCredentialException
import com.google.android.libraries.identity.googleid.GetGoogleIdOption
import dev.exact.canvas.Cause
import dev.exact.canvas.Exact
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import dev.exact.canvas.Extensions
import java.lang.reflect.Proxy
import org.robolectric.Robolectric
import org.robolectric.annotation.Implements
import org.robolectric.annotation.Implementation
import org.robolectric.util.ReflectionHelpers
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** What the Android implementation answers before Credential Manager is asked (LLP 1133.014). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], manifest = Config.NONE, shadows = [Host::class, SignInTest.PlayServices::class])
class SignInTest {
    @Implements(com.google.android.gms.common.GoogleApiAvailabilityLight::class, isInAndroidSdk = false)
    class PlayServices {
        @Implementation fun isGooglePlayServicesAvailable(app: android.content.Context) = com.google.android.gms.common.ConnectionResult.SUCCESS
    }

    private fun provider(config: String = SignInModule.GOOGLE, clientId: String = "web.apps.googleusercontent.com") =
        JSONObject().put("configURL", config).put("clientId", clientId).put("params", JSONObject().put("nonce", "n"))

    private fun request(mode: String? = "active", vararg providers: JSONObject) = JSONObject().put("identity",
        JSONObject().apply { if (mode != null) put("mode", mode) }.put("providers", JSONArray(providers.toList())))

    @Test fun capabilitiesOfferGoogleAloneAndNeverBesideAPasskey() {
        val on = SignInModule.capabilities(true)
        assertTrue(on.getBoolean("available"))
        val google = on.getJSONObject("providers").getJSONObject("google")
        assertTrue(google.getBoolean("active"))
        assertTrue(google.getBoolean("passive"))
        assertFalse("this implementation does not join passkeys' sheet", google.getBoolean("withPublicKey"))
        val apple = on.getJSONObject("providers").getJSONObject("apple")
        assertFalse(apple.getBoolean("active") || apple.getBoolean("passive") || apple.getBoolean("withPublicKey"))
        val off = SignInModule.capabilities(false).getJSONObject("providers").getJSONObject("google")
        assertFalse(off.getBoolean("active") || off.getBoolean("passive"))
    }

    @Test fun onlySupportedRequestsReachGoogle() = withSheet {
        fun refused(error: String, args: JSONObject) {
            val before = options.size
            get(args)
            assertEquals(before, options.size)
            assertEquals(error, replies.last().getString("error"))
        }
        refused("NotSupportedError", request("passive", provider()).put("publicKey", JSONObject()))
        refused("NotSupportedError", request("active", provider(SignInModule.APPLE)))
        refused("TypeError", request("silent", provider()))
        refused("TypeError", request("active", provider(clientId = "")))
        refused("TypeError", JSONObject())
        val typo = request("active", provider())
        typo.getJSONObject("identity").put("context", "typo")
        refused("TypeError", typo)
        val google = provider().put("domainHint", "example.com")
        get(request(null, provider(SignInModule.APPLE), google).put("publicKey", JSONObject.NULL))
        assertEquals("n", passive.nonce)
        assertTrue(passive.filterByAuthorizedAccounts)
        assertFalse(passive.autoSelectEnabled)
        get(request("active", google))
        val button = options.last() as com.google.android.libraries.identity.googleid.GetSignInWithGoogleOption
        assertEquals("n", button.nonce)
        assertEquals("example.com", button.hostedDomainFilter)
    }

    @Test fun failuresCarryCredentialManagersType() {
        val f = SignInModule.refusal(androidx.credentials.exceptions.GetCredentialCancellationException(), false)
        val none = SignInModule.refusal(androidx.credentials.exceptions.NoCredentialException(), false)
        assertEquals("android.credentials.GetCredentialException.TYPE_NO_CREDENTIAL", none.getJSONObject("android").getString("type"))
        assertEquals("NetworkError", none.getString("error"))
        assertEquals("AbortError", SignInModule.refusal(androidx.credentials.exceptions.NoCredentialException(), true).getString("error"))
        assertEquals("NetworkError", f.getString("error"))
        assertEquals("android.credentials.GetCredentialException.TYPE_USER_CANCELED", f.getJSONObject("android").getString("type"))
        assertFalse(SignInModule.failure("TypeError", "x").has("android"))
    }

    @Test fun theAnswerIsFedCMsIdentityCredentialWithTheJwt() {
        val a = SignInModule.answer("header.claims.signature", "h")
        assertEquals("identity", a.getString("type"))
        assertEquals("", a.getString("id"))
        assertEquals(SignInModule.GOOGLE, a.getString("configURL"))
        assertFalse(a.getBoolean("isAutoSelected"))
        assertEquals("header.claims.signature", a.getString("token"))
        assertEquals("h", a.getString("handle"))
        assertFalse(a.has("apple"))
    }

    private class Sheet {
        val controller = Robolectric.buildActivity(Activity::class.java).setup()
        val activity = controller.get()
        val module = SignInModule(ExtensionContext(RuntimeEnvironment.getApplication(), "sign-in"))
        val keys = exact.passkeys.PasskeysModule(ExtensionContext(RuntimeEnvironment.getApplication(), "passkeys"))
        val options = mutableListOf<CredentialOption>()
        val replies = mutableListOf<JSONObject>()
        lateinit var callback: CredentialManagerCallback<GetCredentialResponse, GetCredentialException>
        var clear: android.os.CancellationSignal? = null

        init {
            Extensions.front = { activity }
            Extensions.started()
            Extensions.resumed()
            val manager = Proxy.newProxyInstance(CredentialManager::class.java.classLoader, arrayOf(CredentialManager::class.java)) { _, method, args ->
                when (method.name) {
                    "clearCredentialStateAsync" -> clear = args.filterIsInstance<android.os.CancellationSignal>().single()
                    "getCredentialAsync" -> {
                        options += args.filterIsInstance<GetCredentialRequest>().single().credentialOptions.single()
                        @Suppress("UNCHECKED_CAST")
                        callback = args.filterIsInstance<CredentialManagerCallback<*, *>>().single() as CredentialManagerCallback<GetCredentialResponse, GetCredentialException>
                    }
                }
                null
            } as CredentialManager
            ReflectionHelpers.setField(module, "manager\$delegate", lazyOf(manager))
            ReflectionHelpers.setField(keys, "manager\$delegate", lazyOf(manager))
        }

        fun get(args: JSONObject) = module.later("identity", args, null, Cause("app", request = 1)) { replies += it }
        fun passkey(id: Int, mediation: String = "required") {
            val options = JSONObject().put("rpId", "example.com").put("challenge", "Y2hhbGxlbmdl")
            keys.later("get", JSONObject().put("publicKey", options).put("mediation", mediation), null, Cause("app", request = id)) { replies += it }
        }
        fun signOut() = module.later("preventSilentAccess", JSONObject()) {}
        val passive get() = options.last() as GetGoogleIdOption
        fun success(recipient: CredentialManagerCallback<GetCredentialResponse, GetCredentialException> = callback) = recipient.onResult(GetCredentialResponse(GoogleIdTokenCredential("fixture", "synthetic-token", null, null, null, null, null)))
    }

    private fun withSheet(test: Sheet.() -> Unit) {
        val sheet = Sheet()
        try { sheet.test() } finally {
            sheet.module.destroy {}
            sheet.keys.destroy {}
            Extensions.paused()
            Extensions.stopped(false)
            Extensions.front = { Exact.reader?.get() }
        }
    }

    private fun optedIn() = request("passive", provider().put("android", JSONObject().put("autoSelectEnabled", true)))

    @Test fun anInteractiveRetryAfterSignOutAllowsAutomaticSelectionAgain() = withSheet {
        get(optedIn())
        assertTrue(passive.autoSelectEnabled && passive.filterByAuthorizedAccounts)
        signOut()
        assertNotNull(clear)
        Extensions.paused()
        Extensions.stopped(false)
        callback.onError(NoCredentialException())
        assertEquals(1, options.size)
        Extensions.started()
        Extensions.resumed()
        module.resumed()
        assertFalse(passive.autoSelectEnabled || passive.filterByAuthorizedAccounts)
        success()
        assertEquals("identity", replies.single().getString("type"))
        get(optedIn())
        assertTrue(passive.autoSelectEnabled)
        module.destroy {}
        assertTrue(clear!!.isCanceled)
    }

    @Test fun anEarlierSheetCannotUndoALaterSignOut() = withSheet {
        get(optedIn().put("mediation", "required"))
        assertFalse(passive.autoSelectEnabled)
        signOut()
        success()
        get(optedIn())
        assertFalse(passive.autoSelectEnabled)
        callback.onError(NoCredentialException())
        callback.onError(NoCredentialException())
        assertEquals("NetworkError", replies.last().getString("error"))
        get(optedIn())
        assertFalse(passive.autoSelectEnabled)
    }

    @Test fun aLateAutomaticResultCannotSignThePersonBackIn() = withSheet {
        get(optedIn())
        assertTrue(passive.autoSelectEnabled)
        signOut()
        success()
        assertEquals("AbortError", replies.single().getString("error"))
        get(optedIn())
        assertFalse(passive.autoSelectEnabled)
    }

    @Test fun aReleasedCallCannotRetryOrDeliverALateCredential() = withSheet {
        get(optedIn())
        Extensions.paused()
        callback.onError(NoCredentialException())
        module.cancel(1)
        Extensions.resumed()
        module.resumed()
        success()
        assertEquals(1, options.size)
        assertEquals("AbortError", replies.single().getString("error"))
    }

    @Test fun finishingActivityDiscardsResult() = withSheet {
        get(optedIn())
        activity.finish()
        assertTrue(activity.isFinishing)
        success()
        success()
        assertEquals("AbortError", replies.single().optString("error"))
    }

    @Test fun destroyedActivityDiscardsResult() = withSheet {
        get(optedIn())
        controller.pause().stop().destroy()
        assertTrue(activity.isDestroyed)
        success()
        assertEquals("AbortError", replies.single().optString("error"))
    }

    @Test fun rotatingTheCoveredActivityKeepsItsSheet() = withSheet {
        get(optedIn())
        Extensions.paused()
        val landscape = Configuration(activity.resources.configuration).apply { orientation = Configuration.ORIENTATION_LANDSCAPE }
        activity.onConfigurationChanged(landscape)
        success()
        assertTrue(replies.isEmpty())
        Extensions.resumed()
        module.resumed()
        assertEquals("identity", replies.single().getString("type"))
    }

    @Test fun aCredentialWaitsForTheInitiatingActivityToResume() = withSheet {
        get(optedIn())
        Extensions.paused()
        Extensions.stopped(false)
        success()
        assertTrue(replies.isEmpty())
        signOut()
        Extensions.started()
        Extensions.resumed()
        module.resumed()
        assertEquals("AbortError", replies.single().optString("error"))
    }

    @Test fun aNewRequestReplacesAPassiveSheetAndWaitsBehindAnActiveOne() = withSheet {
        get(optedIn())
        val old = callback
        get(request("active", provider()))
        assertEquals("AbortError", replies.single().optString("error"))
        success(old)
        get(request("active", provider()))
        assertEquals(2, options.size)
        success()
        assertEquals(3, options.size)
        success()
        assertEquals(listOf("AbortError", "identity", "identity"), replies.map { it.optString("error", it.optString("type")) })
    }

    @Test fun theAllAccountsFallbackRequiresAChooser() = withSheet {
        get(optedIn())
        assertTrue(passive.autoSelectEnabled)
        callback.onError(NoCredentialException())
        assertFalse(passive.filterByAuthorizedAccounts)
        assertFalse(passive.autoSelectEnabled)
    }

    @Test fun aReplacementActivityCanStartWithoutTheOldCallback() = withSheet {
        get(request("active", provider()))
        val old = callback
        controller.pause().stop().destroy()
        val next = Robolectric.buildActivity(Activity::class.java).setup().get()
        Extensions.front = { next }
        Extensions.resumed()
        module.resumed()
        get(request("active", provider()))
        assertEquals(2, options.size)
        success(old)
        success()
        assertEquals(listOf("AbortError", "identity"), replies.map { it.optString("error", it.optString("type")) })
    }

    @Test fun aModalPasskeySupersedesPassiveGoogleAndHoldsTheNextGoogleSheet() = withSheet {
        get(optedIn())
        val old = callback
        passkey(2)
        assertEquals("AbortError", replies.single().optString("error"))
        success(old)
        get(request("active", provider()))
        assertEquals(2, options.size)
        callback.onError(NoCredentialException())
        assertEquals(3, options.size)
        success()
        assertEquals("identity", replies.last().getString("type"))
    }

    @Test fun aQueuedPasskeyWaitsWhileThePreviousProviderStillCoversTheActivity() = withSheet {
        passkey(1)
        passkey(2)
        assertEquals(1, options.size)
        Extensions.paused()
        Extensions.stopped(false)
        callback.onError(NoCredentialException())
        assertEquals(listOf("NotAllowedError"), replies.map { it.optString("error") })
        assertEquals(1, options.size)
        Extensions.started()
        Extensions.resumed()
        assertEquals(2, options.size)
        callback.onError(NoCredentialException())
        assertEquals(listOf("NotAllowedError", "NotAllowedError"), replies.map { it.optString("error") })
    }

    @Test fun anExceptionAtTheReplyCannotStrandANewSheet() = withSheet {
        val args = request("active", provider())
        module.later("identity", args, null, Cause("app", request = 3)) { error("test reply") }
        passkey(4)
        try { success(); fail("the first reply should throw") }
        catch (e: IllegalStateException) { assertEquals("test reply", e.message) }
        assertEquals("the queued passkey must reach the manager", 2, options.size)
        callback.onError(NoCredentialException())
        get(args)
        assertEquals("the next Google request must reach the manager", 3, options.size)
    }

    @Test fun aCompletedGoogleSheetCanYieldToAPasskeyBeforeTokenDelivery() = withSheet {
        get(request("active", provider()))
        passkey(2)
        Extensions.paused()
        Extensions.stopped(false)
        success()
        assertTrue(replies.isEmpty())
        Extensions.started()
        assertEquals(2, options.size)
        assertTrue(replies.isEmpty())
        Extensions.resumed()
        module.resumed()
        assertEquals("identity", replies.single().optString("type"))
    }

    @Test fun aPasskeyWaitsForACanceledGoogleSheetToUncoverTheActivity() = withSheet {
        get(optedIn())
        Extensions.paused()
        passkey(2)
        assertEquals("AbortError", replies.single().optString("error"))
        assertEquals(1, options.size)
        Extensions.resumed()
        assertEquals(2, options.size)
    }

    @Test fun sessionEndCancelsBothPackagesBeforePromotingAnySheet() = withSheet {
        get(request("active", provider()))
        passkey(2)
        module.destroy {}
        assertEquals(1, options.size)
        assertEquals(listOf("AbortError", "AbortError"), replies.map { it.optString("error") })
    }

    @Test fun anAbortReplyCannotOpenAnotherSheetInTheEndingSession() = withSheet {
        val args = request("active", provider())
        module.later("identity", args, null, Cause("app", request = 1)) { answer ->
            replies += answer
            get(args)
            passkey(2)
        }
        module.destroy {}
        assertEquals(1, options.size)
        assertEquals(List(3) { "AbortError" }, replies.map { it.optString("error") })
        passkey(3)
        assertEquals(1, options.size)
        assertEquals("AbortError", replies.last().optString("error"))
    }

    @Test fun aConditionalPasskeyNeedsAFieldWhenItsTurnComes() = withSheet {
        val field = android.widget.EditText(activity)
        keys.webauthnField(field, true)
        get(request("active", provider()))
        passkey(2, "conditional")
        keys.webauthnField(field, false)
        success()
        assertEquals(listOf("identity", "NotSupportedError"), replies.map { it.optString("error", it.optString("type")) })
    }

    @Test fun aFailedGoogleResultStillReleasesTheQueue() = withSheet {
        get(request("active", provider()))
        passkey(2)
        val settings = ReflectionHelpers.getField<Lazy<android.content.SharedPreferences>>(module, "settings\$delegate").value
        settings.edit().putString("signOut", "invalid").commit()
        try {
            success()
            assertEquals("IdentityCredentialError", replies.single().optString("error"))
            assertEquals(2, options.size)
        } finally { settings.edit().remove("signOut").commit() }
    }

    @Test fun aSecondModalSubmittedUnderTheFirstSheetSupersedesPassiveGoogle() = withSheet {
        get(optedIn())
        Extensions.paused()
        get(request("active", provider()))
        assertEquals("the live passive get must be superseded", "AbortError", replies.single().optString("error"))
        Extensions.resumed()
        module.resumed()
        assertEquals(2, options.size)
    }

    @Test fun releasingTheRealReaderAbortsWithoutACredentialManagerCallback() = withSheet {
        val host = Robolectric.buildActivity(dev.exact.canvas.ExactActivity::class.java).get()
        ReflectionHelpers.setField(host, "requests", dev.exact.canvas.Requests(host))
        Extensions.front = { host }
        val active = request("active", provider())
        module.later("identity", active, null, Cause("app", request = 3)) {
            get(active)
            throw AssertionError("test reply")
        }
        passkey(4)
        get(active)
        assertEquals(1, options.size)
        val old = callback
        ReflectionHelpers.callInstanceMethod<Unit>(host, "release")
        assertEquals(List(3) { "AbortError" }, replies.map { it.optString("error") })
        success(old)
        assertEquals(3, replies.size)
        assertEquals(1, options.size)
        Extensions.front = { activity }
        get(active)
        assertEquals(2, options.size)
    }

    @Test fun androidWordsTakeOnlyBooleans() = withSheet {
        for (name in listOf("autoSelectEnabled", "filterByAuthorizedAccounts", "requestVerifiedPhoneNumber", "unknown")) {
            for (wrong in listOf("true", 1, JSONObject(), JSONObject.NULL)) {
                get(request("passive", provider().put("android", JSONObject().put(name, wrong))))
                assertTrue("Android forwarded $name=$wrong", options.isEmpty())
                val failure = replies.removeAt(0)
                assertEquals("TypeError", failure.getString("error"))
                assertTrue(failure.getString("message").contains(name))
            }
        }
    }

    @Test fun theVerifiedPhoneOptionIsOnlyForPassiveSignupWithAllAccounts() = withSheet {
        val words = JSONObject().put("requestVerifiedPhoneNumber", true)
        val args = request("passive", provider().put("android", words))
        val identity = args.getJSONObject("identity")
        for (context in listOf(null, "signin", "continue", "use")) {
            identity.put("context", context)
            get(args)
            assertTrue(options.isEmpty())
            assertTrue(replies.last().getString("message").contains("requestVerifiedPhoneNumber"))
        }
        identity.put("context", "signup").put("mode", "active")
        get(args)
        assertTrue(options.isEmpty())
        identity.put("mode", "passive")
        words.put("filterByAuthorizedAccounts", true)
        get(args)
        assertEquals("TypeError", replies.last().getString("error"))
        assertTrue(options.isEmpty())
        for (filter in listOf(null, false)) {
            words.put("filterByAuthorizedAccounts", filter)
            get(args)
            assertTrue(passive.requestVerifiedPhoneNumber)
            assertFalse(passive.filterByAuthorizedAccounts)
            callback.onError(NoCredentialException())
        }
    }

    @Test fun theButtonSaysItsContextAndTakesItsTheme() {
        val button = GoogleButton(RuntimeEnvironment.getApplication())
        button.set(JSONObject())
        assertEquals(View.GONE, button.visibility)
        fun props() = JSONObject().put("provider", "google")
        button.set(props())
        assertEquals(View.VISIBLE, button.visibility)
        assertEquals("android.widget.Button", button.accessibilityClassName)
        assertEquals("Sign in with Google", button.contentDescription)
        button.set(props().put("text", "signin"))
        assertEquals("Sign in", button.contentDescription)
        val contexts = mapOf("signin" to "Sign in with Google", "use" to "Sign in with Google",
            "signup" to "Sign up with Google", "continue" to "Continue with Google")
        val texts = mapOf("signin" to "Sign in", "signin_with" to "Sign in with Google",
            "signup_with" to "Sign up with Google", "continue_with" to "Continue with Google")
        for ((context, byContext) in contexts) {
            for (text in listOf(null, JSONObject.NULL, "unknown") + texts.keys) {
                button.set(props().put("context", context).put("text", text))
                val expected = texts[text as? String] ?: byContext
                assertEquals(expected, button.contentDescription)
                assertEquals(expected, (button.getChildAt(1) as TextView).text.toString())
            }
        }
        var presses = 0
        button.setOnClickListener { presses++ }
        assertTrue(button.performClick())
        assertEquals(1, presses)
        button.set(props().put("context", "continue").put("theme", "filled_blue").put("shape", "pill"))
        assertEquals("Continue with Google", button.contentDescription)
        button.set(props().put("context", "signup").put("theme", "filled_black"))
        assertEquals("Sign up with Google", button.contentDescription)
    }
}
