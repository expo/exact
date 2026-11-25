// Android push uses an FCM registration token for Expo's push service and keeps the Firebase Installations ID
// beside it. Firebase starts at first use; no registration or network request runs in the module's constructor.
package exact.push

import android.Manifest
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.installations.FirebaseInstallations
import dev.exact.canvas.Cause
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONObject
import java.lang.ref.WeakReference

class PushModule(context: ExtensionContext) : ExactExtension(context) {
    companion object {
        private const val KEPT = "exact.push"
        private const val ID = "installationId"
        private const val WAIT_MS = 60_000L

        /** The module the process's session made, including a headless one. */
        @Volatile internal var live: WeakReference<PushModule>? = null
        private var asked = false

        fun firebase(app: Context): FirebaseApp? = FirebaseApp.getApps(app).firstOrNull() ?: FirebaseApp.initializeApp(app)
        private fun prefs(app: Context) = app.getSharedPreferences(KEPT, Context.MODE_PRIVATE)

        /** Token mode still has a Firebase Installations ID, but servers address this subscription by its token. */
        internal fun kept(app: Context): JSONObject? {
            val id = prefs(app).getString(ID, null) ?: return null
            val token = prefs(app).getString("token", null) ?: return null
            return JSONObject().put("android", JSONObject().put(ID, id).put("token", token).put("appId", app.packageName))
        }

        internal fun rotated(app: Context, token: String) {
            prefs(app).edit().putString("token", token).apply()
            live?.get()?.announce(kept(app))
        }
    }

    private val app = context.application
    /** The subscription last announced to `pushSubscription()`'s readers. */
    private var said: String? = null
    /** The permission state last read, to announce a change once. */
    private var saidState: String? = null
    /** Concurrent subscribers, answered together. Main thread. */
    private val waiting = mutableListOf<(JSONObject?, String?) -> Unit>()
    private val main = Handler(Looper.getMainLooper())

    init {
        live = WeakReference(this)
    }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) = later(name, args, null, Cause.none, reply)

    override fun later(name: String, args: JSONObject, preferred: Any?, cause: Cause, reply: (JSONObject) -> Unit) {
        unavailable()?.let { why ->
            return when (name) {
                "pushPermissionState" -> reply(JSONObject().put("available", false).put("state", "unknown"))
                "pushSubscription" -> reply(JSONObject().put("exact:answer", JSONObject.NULL))
                else -> reply(refused("NotSupportedError", why))
            }
        }
        when (name) {
            "pushPermissionState" -> reply(status())
            "pushSubscription" -> {
                val granted = status().getString("state") == "granted"
                val value = kept(app).takeIf { granted }
                said = value?.toString()
                if (granted && prefs(app).contains(ID) && !asked) register { _, failure -> failure?.let { context.log("push: $it") } }
                reply(JSONObject().put("exact:answer", value ?: JSONObject.NULL))
            }
            "subscribePush" -> ask(cause) {
                when (status().getString("state")) {
                    "granted" -> register { id, failure ->
                        reply(if (id != null) id else refused("AbortError", failure ?: "FCM did not register this app"))
                    }
                    else -> reply(refused("NotAllowedError", "notifications are off for this app"))
                }
            }
            else -> reply(refused("NotSupportedError", "$name is not on Android"))
        }
    }

    /** Settings may have changed while the app was away. */
    override fun resumed() {
        if (saidState != null && unavailable() == null) status()
    }

    /** Why push is unavailable in this build or on this device, or null. */
    private fun unavailable(): String? = when {
        firebase(app) == null -> "this build has no google-services.json with a client for ${app.packageName}"
        GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(app) != ConnectionResult.SUCCESS -> "this device has no Google Play services"
        else -> null
    }

    /** Concurrent subscribers share one registration. A timed-out registration cannot overwrite a later attempt. */
    private var registration = 0

    @Suppress("DEPRECATION") // Expo exchanges registration tokens; Firebase's FID messaging mode disables getToken.
    private fun register(done: (JSONObject?, String?) -> Unit) {
        if (asked && waiting.isEmpty()) kept(app)?.let { return done(it, null) }
        waiting += done
        if (waiting.size > 1) return
        asked = true
        val attempt = ++registration
        val timeout = Runnable { complete(attempt, null, "FCM did not register this app within a minute") }
        main.postDelayed(timeout, WAIT_MS)
        FirebaseMessaging.getInstance().token.addOnCompleteListener { token ->
            if (attempt != registration) return@addOnCompleteListener
            if (!token.isSuccessful) {
                main.removeCallbacks(timeout)
                complete(attempt, null, token.exception?.message ?: "FCM did not register this app")
            } else FirebaseInstallations.getInstance().id.addOnCompleteListener installation@{ installation ->
                if (attempt != registration) return@installation
                main.removeCallbacks(timeout)
                if (!installation.isSuccessful) complete(attempt, null, installation.exception?.message ?: "Firebase has no installation ID")
                else {
                    prefs(app).edit().putString(ID, installation.result).putString("token", token.result).apply()
                    complete(attempt, kept(app), null)
                }
            }
        }
    }

    private fun complete(attempt: Int, subscription: JSONObject?, failure: String?) {
        if (attempt != registration) return
        registration++
        if (failure != null) asked = false
        waiting.toList().also { waiting.clear() }.forEach { it(subscription, failure) }
        if (subscription != null) announce(subscription)
    }

    private fun announce(subscription: JSONObject?) {
        val value = subscription?.toString()
        if (value != said) {
            said = value
            context.changed("push.pushSubscription")
        }
    }

    /** The runtime permission's dialog while it is `prompt` (Android 13 and later), then [done]; at once otherwise. */
    private fun ask(cause: Cause, done: () -> Unit) {
        if (Build.VERSION.SDK_INT >= 33 && context.permission(Manifest.permission.POST_NOTIFICATIONS).state == "prompt") {
            context.askPermission(cause, Manifest.permission.POST_NOTIFICATIONS, done)
        } else done()
    }

    /** The permission fact, notifications' as `@exact/notifications` reads it. A change is announced to both
     * sources' readers. */
    private fun status(): JSONObject {
        val on = app.getSystemService(android.app.NotificationManager::class.java).areNotificationsEnabled()
        val permission = if (Build.VERSION.SDK_INT >= 33) context.permission(Manifest.permission.POST_NOTIFICATIONS).state else null
        val state = when (permission) {
            null, "granted" -> if (on) "granted" else "denied"
            "denied" -> "denied"
            else -> "prompt"
        }
        if (saidState != null && saidState != state) {
            context.changed("push.pushPermissionState")
            context.changed("push.pushSubscription")
        }
        saidState = state
        return JSONObject().put("available", true).put("state", state)
    }

    private fun refused(code: String, message: String) = JSONObject().put("error", code).put("message", message)
}
