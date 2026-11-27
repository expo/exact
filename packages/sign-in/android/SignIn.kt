// LLP 1133.014 D3 and D10. An identity-only request uses Credential Manager directly.
// The passkeys package owns combined requests. It has no Android request table yet.
package exact.signin

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.os.CancellationSignal
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.graphics.PathParser
import androidx.credentials.ClearCredentialStateRequest
import androidx.credentials.CredentialManager
import androidx.credentials.CredentialManagerCallback
import androidx.credentials.CustomCredential
import androidx.credentials.GetCredentialRequest
import androidx.credentials.GetCredentialResponse
import androidx.credentials.exceptions.ClearCredentialException
import androidx.credentials.exceptions.GetCredentialCancellationException
import androidx.credentials.exceptions.GetCredentialException
import androidx.credentials.exceptions.GetCredentialProviderConfigurationException
import androidx.credentials.exceptions.GetCredentialUnsupportedException
import androidx.credentials.exceptions.NoCredentialException
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailabilityLight
import com.google.android.libraries.identity.googleid.GetGoogleIdOption
import com.google.android.libraries.identity.googleid.GetSignInWithGoogleOption
import com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
import dev.exact.canvas.Cause
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

private class InvalidRequest(message: String, val error: String = "TypeError") : Exception(message)

private inline fun <reified T> JSONObject.word(key: String): T? {
    if (isNull(key)) return null
    return opt(key) as? T ?: throw InvalidRequest("invalid type for $key")
}

private class GoogleRequest(args: JSONObject) {
    init {
        if (!args.isNull("publicKey")) throw InvalidRequest("Google cannot share the Android passkey sheet", "NotSupportedError")
    }
    private val identity = args.word<JSONObject>("identity") ?: throw InvalidRequest("identity must be an object")
    val active = when (identity.word<String>("mode")) {
        null, "passive" -> false
        "active" -> true
        else -> throw InvalidRequest("identity.mode must be active or passive")
    }
    private val providers = identity.word<JSONArray>("providers") ?: throw InvalidRequest("identity.providers must be an array")
    init {
        if (active && providers.length() != 1) throw InvalidRequest("active identity requires one provider", "NotSupportedError")
    }
    val provider = (0 until providers.length()).map { providers.optJSONObject(it) ?: throw InvalidRequest("each provider must be an object") }
        .firstOrNull { it.word<String>("configURL") == SignInModule.GOOGLE }
        ?: throw InvalidRequest("Android offers Google alone", if (active) "NotSupportedError" else "NetworkError")
    private val client = provider.word<String>("clientId")?.takeIf { it.isNotBlank() }
        ?: throw InvalidRequest("clientId must be Google's web client ID")
    private val nonce = provider.word<JSONObject>("params")?.word<String>("nonce")
    val domain = provider.word<String>("domainHint")
    private val words = provider.word<JSONObject>("android")
    private fun flag(key: String): Boolean? {
        if (words == null || !words.has(key)) return null
        return words.opt(key) as? Boolean ?: throw InvalidRequest("invalid type for android.$key")
    }
    val filter = flag("filterByAuthorizedAccounts")
    private val auto = flag("autoSelectEnabled") == true
    val phone = flag("requestVerifiedPhoneNumber") == true
    private val required = when (args.word<String>("mediation")) {
        "required" -> true
        null, "optional" -> false
        else -> throw InvalidRequest("identity supports optional or required mediation", "NotSupportedError")
    }

    init {
        words?.keys()?.forEach { name ->
            if (name !in setOf("filterByAuthorizedAccounts", "autoSelectEnabled", "requestVerifiedPhoneNumber"))
                throw InvalidRequest("unknown android.$name")
        }
        val context = identity.word<String>("context")
        if (context != null && context !in setOf("signin", "signup", "continue", "use"))
            throw InvalidRequest("identity.context must be signin, signup, continue or use")
        if (phone && (active || context != "signup"))
            throw InvalidRequest("requestVerifiedPhoneNumber requires passive signup", "NotSupportedError")
        if (phone && filter == true)
            throw InvalidRequest("requestVerifiedPhoneNumber requires filterByAuthorizedAccounts false")
    }

    fun allowAuto(requireUI: Boolean) = !active && auto && !required && !requireUI

    fun option(authorized: Boolean, allowAuto: Boolean) =
        if (active) GetSignInWithGoogleOption.Builder(client).apply {
            nonce?.let(::setNonce)
            domain?.let(::setHostedDomainFilter)
        }.build() else GetGoogleIdOption.Builder()
            .setServerClientId(client).setFilterByAuthorizedAccounts(authorized)
            .setAutoSelectEnabled(allowAuto).setRequestVerifiedPhoneNumber(phone)
            .apply { nonce?.let(::setNonce) }.build()
}

class SignInModule(context: ExtensionContext) : ExactExtension(context) {
    companion object {
        const val GOOGLE = "https://accounts.google.com/gsi/fedcm.json"
        const val APPLE = "https://appleid.apple.com"

        /** Credential Manager needs Play services to offer Google. */
        fun capabilities(google: Boolean): JSONObject = JSONObject()
            .put("available", true)
            .put("providers", JSONObject()
                .put("apple", provider(false))
                .put("google", provider(google)))

        private fun hasGoogle(app: Context) =
            GoogleApiAvailabilityLight.getInstance().isGooglePlayServicesAvailable(app) == ConnectionResult.SUCCESS

        fun read(app: Context, request: JSONObject): JSONObject? =
            if (request.isNull("owner") && request.opt("sync") == "getIdentityCapabilitiesSync") capabilities(hasGoogle(app)) else null

        private fun provider(on: Boolean) =
            JSONObject().put("active", on).put("passive", on).put("withPublicKey", false)

        fun failure(error: String, message: String, type: String? = null): JSONObject {
            val answer = JSONObject().put("error", error).put("message", message)
            if (type != null) answer.put("android", JSONObject().put("type", type))
            return answer
        }

        internal fun refusal(e: GetCredentialException, cancelled: Boolean): JSONObject {
            val name = when {
                cancelled -> "AbortError"
                e is GetCredentialCancellationException || e is NoCredentialException -> "NetworkError"
                e is GetCredentialProviderConfigurationException || e is GetCredentialUnsupportedException -> "NotSupportedError"
                else -> "IdentityCredentialError"
            }
            return failure(name, e.javaClass.simpleName, e.type)
        }

        /** FedCM's IdentityCredential for Google's ID token (exact.json's `IdentityCredentialJSON`). */
        fun answer(idToken: String, handle: String): JSONObject = JSONObject()
            .put("type", "identity")
            .put("id", "")
            .put("configURL", GOOGLE)
            .put("isAutoSelected", false)
            .put("token", idToken)
            .put("handle", handle)
    }

    private val manager by lazy { CredentialManager.create(context.application) }
    /** Running requests by their send's request id, cancelled when let go or at the session's end. */
    private class Pending(val id: Int?, val reply: (JSONObject) -> Unit) {
        var providerActivity: android.app.Activity? = null
        var onResume: (() -> Unit)? = null
    }
    private val signals = linkedMapOf<CancellationSignal, Pending>()
    // clearCredentialState notifies providers but cannot require UI on a later get (D1).
    private val settings by lazy {
        // A bound drive has its own sign-out state, including after a relaunch.
        val name = if (context.scratch) "exact.sign-in.${UUID.nameUUIDFromBytes(context.data.path.toByteArray())}" else "exact.sign-in"
        context.application.getSharedPreferences(name, Context.MODE_PRIVATE)
    }
    private val generation get() = settings.getLong("signOut", 0L)

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) = later(name, args, null, Cause.none, reply)

    override fun later(name: String, args: JSONObject, preferred: Any?, cause: Cause, reply: (JSONObject) -> Unit) {
        when (name) {
            "getIdentityCapabilities" -> reply(capabilities(hasGoogle(context.application)))
            "preventSilentAccess" -> clear(cause, reply)
            "identity" -> signIn(args, cause, reply)
            else -> reply(failure("NotSupportedError", "$name is not built on Android (LLP 1133.014)"))
        }
    }

    /** Play services can be updated or enabled while the app is away. */
    override fun resumed() {
        context.changed("sign-in.getIdentityCapabilities")
        advance()
    }

    // Credential Manager may call back before its activity uncovers ours. Only the
    // first sign-in may start, retry or deliver. A clear never occupies the sheet.
    private fun advance() {
        if (!context.resumed) return
        for (pending in signals.values.toList()) {
            val next = pending.onResume ?: continue
            pending.onResume = null
            next()
        }
    }

    override fun cancel(request: Int) {
        cancelAll(signals.filterValues { it.id == request }.keys, "the call was let go")
    }

    override fun destroy(done: () -> Unit) {
        context.credentials.endSession { cancelAll(signals.keys, "the session ended") }
        done()
    }

    private fun cancelAll(selected: Collection<CancellationSignal>, message: String) =
        context.credentials.cancelAll(selected.map { { abort(it, message) } })

    private fun abort(signal: CancellationSignal, message: String) {
        val pending = signals.remove(signal) ?: return
        pending.providerActivity?.let { context.credentials.cancelingProvider(it) }
        context.credentials.finish(signal) {
            try { signal.cancel() } finally { pending.reply(failure("AbortError", message)) }
        }
    }

    private fun signIn(args: JSONObject, cause: Cause, reply: (JSONObject) -> Unit) {
        if (!cause.app) return reply(failure("NotAllowedError", "person-required"))
        val activity = context.activity
        if (activity == null || !context.visible || activity.isFinishing || activity.isDestroyed) return reply(failure("NotAllowedError", "the app is not in front"))
        val google = try { GoogleRequest(args) }
        catch (e: InvalidRequest) { return reply(failure(e.error, e.message.orEmpty())) }
        if (!hasGoogle(context.application)) return reply(failure(if (google.active) "NotSupportedError" else "NetworkError", "Google Play services cannot serve Sign in with Google here"))
        if (!google.provider.isNull("loginHint")) context.journal("sign-in: Android cannot apply loginHint")
        if (!google.provider.isNull("fields")) context.journal("sign-in: Google determines the ID token's fields")
        if (!google.active && google.domain != null) context.journal("sign-in: Google accepts domainHint only on its button flow")

        val signal = CancellationSignal()
        val pending = Pending(cause.request, reply)
        fun finish(value: JSONObject) {
            val settled = signals.remove(signal) ?: return
            context.credentials.finish(signal) { settled.reply(value) }
        }
        fun live(): Boolean {
            if (!signals.containsKey(signal)) return false
            if (context.activity === activity && !activity.isFinishing && !activity.isDestroyed) return true
            finish(failure("AbortError", "the initiating activity ended"))
            return false
        }
        fun credential(result: GetCredentialResponse, allowAuto: Boolean, started: Long): JSONObject {
            if (allowAuto && generation != started)
                return failure("AbortError", "sign-out superseded automatic selection")
            val credential = result.credential
            if (credential !is CustomCredential || credential.type != GoogleIdTokenCredential.TYPE_GOOGLE_ID_TOKEN_CREDENTIAL)
                return failure("IdentityCredentialError", "Credential Manager returned a different credential type")
            val token = GoogleIdTokenCredential.createFrom(credential.data)
            if (generation == started && !allowAuto) settings.edit().remove("requireUI").apply()
            return answer(token.idToken, UUID.randomUUID().toString())
        }
        fun ask(authorized: Boolean) {
            if (!live()) return
            try {
                // Sign-out can fall between the authorized-accounts attempt and its retry.
                val started = generation
                val allowAuto = authorized && google.allowAuto(settings.getBoolean("requireUI", false))
                val option = google.option(authorized, allowAuto)
                val request = GetCredentialRequest.Builder().addCredentialOption(option).build()
                pending.providerActivity = activity
                manager.getCredentialAsync(activity, request, signal, activity.mainExecutor, object : CredentialManagerCallback<GetCredentialResponse, GetCredentialException> {
                    override fun onResult(result: GetCredentialResponse) {
                        if (!live()) return
                        pending.providerActivity = null
                        pending.onResume = {
                            if (live()) {
                                val value = try { credential(result, allowAuto, started) }
                                catch (e: Exception) { failure("IdentityCredentialError", "could not process the Google credential", e.javaClass.name) }
                                finish(value)
                            }
                        }
                        if (context.resumed) advance() else context.credentials.answered(signal)
                    }
                    override fun onError(e: GetCredentialException) {
                        pending.providerActivity = null
                        if (!live()) return
                        if (e is NoCredentialException && !google.active && authorized && google.filter == null) {
                            // Credential Manager can answer before its activity uncovers ours.
                            // A release removes the pending retry along with the request.
                            pending.onResume = { ask(false) }
                            advance()
                        } else finish(refusal(e, signal.isCanceled))
                    }
                })
            } catch (e: IllegalArgumentException) { finish(failure("TypeError", "invalid Google sign-in options")) }
              catch (e: Exception) { finish(failure("IdentityCredentialError", e.javaClass.simpleName)) }
        }
        signals[signal] = pending
        context.credentials.enqueue(signal, activity, !google.active, { abort(signal, "the credential request ended") }) {
            pending.onResume = { ask(google.filter ?: !google.phone) }
            advance()
        }
    }

    private fun clear(cause: Cause, reply: (JSONObject) -> Unit) {
        if (context.credentials.ended) return reply(failure("AbortError", "the session ended"))
        if (!settings.edit().putLong("signOut", generation + 1).putBoolean("requireUI", true).commit())
            return reply(failure("IdentityCredentialError", "could not persist the requirement for interactive sign-in"))
        val signal = CancellationSignal().also { signals[it] = Pending(cause.request, reply) }
        fun finish(answer: JSONObject) { signals.remove(signal)?.reply?.invoke(answer) }
        try {
            manager.clearCredentialStateAsync(ClearCredentialStateRequest(), signal, context.application.mainExecutor,
                object : CredentialManagerCallback<Void?, ClearCredentialException> {
                    override fun onResult(result: Void?) = finish(JSONObject())
                    override fun onError(e: ClearCredentialException) {
                        val unsupported = e is androidx.credentials.exceptions.ClearCredentialProviderConfigurationException ||
                            e is androidx.credentials.exceptions.ClearCredentialUnsupportedException
                        finish(failure(if (unsupported) "NotSupportedError" else "IdentityCredentialError", e.javaClass.simpleName, e.type))
                    }
                })
        } catch (e: Exception) { finish(failure("IdentityCredentialError", e.javaClass.simpleName)) }
    }

    override fun dispose(ids: List<String>) {
        for (id in ids) context.journal("dispose sign-in $id: its reply was dropped")
    }

    override fun view(tag: String, session: String, id: String, context: Context, props: JSONObject, emit: (String, String) -> Unit): View {
        require(tag == "exact-sign-in-button") { "no view $tag" }
        return GoogleButton(context).also { it.setOnClickListener { emit("press", "") }; it.set(props) }
    }

    override fun update(view: View, props: JSONObject) { (view as? GoogleButton)?.set(props) }
}

/** Google's sign-in button by its branding guidelines: the four-colour mark, Roboto Medium 14, and the theme's colours. */
internal class GoogleButton(context: Context) : LinearLayout(context) {
    private val mark = ImageView(context)
    private val label = TextView(context)
    private fun dp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics)

    init {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER
        isClickable = true
        isFocusable = true
        minimumHeight = dp(40f).toInt()
        val pad = dp(12f).toInt()
        setPadding(pad, 0, pad, 0)
        mark.setImageDrawable(GoogleMark())
        val size = dp(18f).toInt()
        addView(mark, LayoutParams(size, size).apply { marginEnd = dp(10f).toInt() })
        label.typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
        label.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
        label.maxLines = 1
        addView(label, LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT))
    }

    override fun getAccessibilityClassName(): CharSequence = android.widget.Button::class.java.name

    fun set(props: JSONObject) {
        val google = props.opt("provider") == "google"
        visibility = if (google) View.VISIBLE else View.GONE
        isEnabled = google
        val words = when (props.opt("text") as? String) {
            "signin" -> "Sign in"
            "signin_with" -> "Sign in with Google"
            "signup_with" -> "Sign up with Google"
            "continue_with" -> "Continue with Google"
            else -> when (props.opt("context") as? String) {
                "signup" -> "Sign up with Google"
                "continue" -> "Continue with Google"
                else -> "Sign in with Google"
            }
        }
        label.text = words
        contentDescription = words
        // outline (the default), filled_blue, filled_black, as Google's web button names them.
        val (fill, text, stroke) = when (props.opt("theme") as? String) {
            "filled_blue" -> Triple(0xFF1A73E8.toInt(), 0xFFFFFFFF.toInt(), 0xFF1A73E8.toInt())
            "filled_black" -> Triple(0xFF131314.toInt(), 0xFFE3E3E3.toInt(), 0xFF8E918F.toInt())
            else -> Triple(0xFFFFFFFF.toInt(), 0xFF1F1F1F.toInt(), 0xFF747775.toInt())
        }
        label.setTextColor(text)
        val surface = GradientDrawable().apply {
            setColor(fill)
            setStroke(dp(1f).toInt().coerceAtLeast(1), stroke)
            cornerRadius = dp(if (props.opt("shape") == "pill") 999f else 4f)
        }
        background = RippleDrawable(ColorStateList.valueOf(if (fill == 0xFFFFFFFF.toInt()) 0x1F1F1F1F else 0x33FFFFFF), surface, null)
        // On a filled blue button the mark sits on a white circle.
        mark.background = if (props.opt("theme") == "filled_blue")
            GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(0xFFFFFFFF.toInt()) } else null
        val inset = if (props.opt("theme") == "filled_blue") dp(2f).toInt() else 0
        val size = dp(18f).toInt() + 2 * inset
        mark.layoutParams = mark.layoutParams.apply { width = size; height = size }
        mark.setPadding(inset, inset, inset, inset)
    }
}

/** Google's "G", from the same path data the web implementation draws (a 48-unit square). */
internal class GoogleMark : Drawable() {
    private val parts = listOf(
        0xFFEA4335.toInt() to "M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z",
        0xFF4285F4.toInt() to "M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z",
        0xFFFBBC05.toInt() to "M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z",
        0xFF34A853.toInt() to "M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z",
    ).map { (color, data) -> Paint(Paint.ANTI_ALIAS_FLAG).apply { this.color = color } to PathParser.createPathFromPathData(data) }

    override fun draw(canvas: Canvas) {
        val b = bounds
        canvas.save()
        canvas.translate(b.left.toFloat(), b.top.toFloat())
        canvas.scale(b.width() / 48f, b.height() / 48f)
        for ((paint, path) in parts) canvas.drawPath(path, paint)
        canvas.restore()
    }
    override fun setAlpha(alpha: Int) { parts.forEach { it.first.alpha = alpha } }
    override fun setColorFilter(colorFilter: ColorFilter?) { parts.forEach { it.first.colorFilter = colorFilter } }
    @Deprecated("Deprecated in Java")
    override fun getOpacity() = PixelFormat.TRANSLUCENT
    override fun getIntrinsicWidth() = 48
    override fun getIntrinsicHeight() = 48
}
