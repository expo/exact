// @exact/push on Android (LLP 1133.004 §9 P3; LLP 1133.004.000 D6, D7): the app's Firebase Installation ID, once FCM
// has registered it, as the live value for the app to send to its own server; Exact sends and relays nothing.
//
// - Firebase starts at first use, from the string resources the build wrote from `google-services.json`: the build
//   removes its start-up provider and automatic registration is off, so none of it runs before first pixel. Without
//   that file, or on a device without Google Play services, `pushPermissionState().available` is false and
//   `subscribePush()` refuses `NotSupportedError`.
// - The permission is notifications' (`POST_NOTIFICATIONS` from Android 13), which a visible push shows under:
//   `subscribePush()` asks while `prompt` and refuses `NotAllowedError` while `denied`, then calls
//   `FirebaseMessaging.register()` and answers the ID [PushService.onRegistered] delivers, or `AbortError`.
//   `register()`'s task carries no ID; 26.0.1 calls `onRegistered` even when the registration is unchanged.
// - `pushSubscription()` never asks: while permission is granted it reads the kept ID, and null otherwise. Only an app
//   that has subscribed registers again, at its first granted read in each launch.
// - A different ID from `onRegistered` is kept and announced to a session's readers; `onUnregistered` clears it. With
//   no session, the next read answers.
// - A notification message, its tap and a data message are [PushService]'s.
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
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONObject
import java.lang.ref.WeakReference

class PushModule(context: ExtensionContext) : ExactExtension(context) {
    companion object {
        private const val KEPT = "exact.push"
        private const val ID = "installationId"
        private const val WAIT_MS = 60_000L

        /** The module a session made, which a registration is announced to. */
        @Volatile private var live: WeakReference<PushModule>? = null

        /** Whether this process has asked FCM to register, so a launch registers once. Main thread. */
        private var asked = false

        /** Firebase, started at first use from the build's resources; null without `google-services.json`. */
        fun firebase(app: Context): FirebaseApp? = FirebaseApp.getApps(app).firstOrNull() ?: FirebaseApp.initializeApp(app)

        private fun prefs(app: Context) = app.getSharedPreferences(KEPT, Context.MODE_PRIVATE)

        /** `onRegistered`: [id] is kept, and given to a session's waiters and readers. Main thread. */
        internal fun registered(app: Context, id: String) {
            if (id != prefs(app).getString(ID, null)) prefs(app).edit().putString(ID, id).apply()
            live?.get()?.delivered(id)
        }

        /** `onUnregistered`: the kept ID is cleared, and the source reads null. Main thread. */
        internal fun unregistered(app: Context) {
            prefs(app).edit().remove(ID).apply()
            asked = false
            live?.get()?.announce(null)
        }
    }

    private val app = context.application
    /** The ID last announced to `pushSubscription()`'s readers. */
    private var said: String? = null
    /** The permission state last read, to announce a change once. */
    private var saidState: String? = null
    /** Callers waiting for `onRegistered`, each answered the ID or a failure's message. Main thread. */
    private val waiting = mutableListOf<(String?, String?) -> Unit>()
    private val main = Handler(Looper.getMainLooper())

    init {
        live = WeakReference(this)
    }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
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
                said = kept().takeIf { status().getString("state") == "granted" }
                if (said != null && !asked) register { _, failure -> failure?.let { context.log("push: $it") } }
                reply(JSONObject().put("exact:answer", said?.let(::subscription) ?: JSONObject.NULL))
            }
            "subscribePush" -> ask {
                when (status().getString("state")) {
                    "granted" -> register { id, failure ->
                        reply(if (id != null) subscription(id) else refused("AbortError", failure ?: "FCM did not register this app"))
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

    /** The ID registered in this launch, else `register()`'s outcome: [done] gets the ID `onRegistered` delivers, or
     * the message of a failed registration, or of none within [WAIT_MS]. Main thread. */
    private fun register(done: (String?, String?) -> Unit) {
        if (asked && waiting.isEmpty()) kept()?.let { return done(it, null) }
        waiting += done
        main.postDelayed({ if (waiting.remove(done)) done(null, "FCM did not register this app within a minute") }, WAIT_MS)
        if (waiting.size > 1) return
        asked = true
        // Listeners run on the main thread.
        FirebaseMessaging.getInstance().register().addOnCompleteListener { task ->
            if (task.isSuccessful) return@addOnCompleteListener
            asked = false
            val message = task.exception?.message ?: "FCM did not register this app"
            waiting.toList().also { waiting.clear() }.forEach { it(null, message) }
        }
    }

    private fun kept(): String? = prefs(app).getString(ID, null)

    /** The waiters get [id]; a different one is announced. */
    private fun delivered(id: String) {
        waiting.toList().also { waiting.clear() }.forEach { it(id, null) }
        announce(id)
    }

    /** The subscription's readers read again when what they last read changed. */
    private fun announce(id: String?) {
        if (said != null && id != said) {
            said = id
            context.changed("push.pushSubscription")
        }
    }

    /** The value app code reads: the credential under the platform's key. */
    private fun subscription(id: String) = JSONObject().put("android", JSONObject().put(ID, id))

    /** The runtime permission's dialog while it is `prompt` (Android 13 and later), then [done]; at once otherwise. */
    private fun ask(done: () -> Unit) {
        if (Build.VERSION.SDK_INT >= 33 && context.permission(Manifest.permission.POST_NOTIFICATIONS).state == "prompt") {
            context.askPermission(Manifest.permission.POST_NOTIFICATIONS, done)
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
