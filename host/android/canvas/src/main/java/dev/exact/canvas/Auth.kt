package dev.exact.canvas

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContract
import androidx.browser.auth.AuthTabIntent
import androidx.browser.customtabs.CustomTabsClient
import androidx.core.app.ActivityOptionsCompat

/** Only the wait lives in this process. Recreating the activity preserves it; killing the process does not. */
internal object Auth {
    private val handler = Handler(Looper.getMainLooper())
    private class Wait(val callback: Uri, val state: String, val logout: Boolean, val timeout: Runnable, val expires: Long,
                       var stopped: Boolean = false, var resultCode: Int? = null)
    private val waits = HashMap<Int, Wait>()
    private var nextResult = 0
    private val contract = object : ActivityResultContract<Intent, Uri?>() {
        override fun createIntent(context: Context, input: Intent) = input
        override fun parseResult(resultCode: Int, intent: Intent?) = intent?.data
    }

    fun open(activity: ExactActivity, id: Int, url: String, callback: String, state: String, logout: Boolean = false) {
        end(id)
        val timeout = Runnable { end(id); Exact.answer(id, "cancelled") }
        val bound = 10 * 60 * 1000L
        val wait = Wait(Uri.parse(callback), state, logout, timeout, SystemClock.elapsedRealtime() + bound)
        waits[id] = wait
        try {
            // HTTPS fallbacks need App Link navigation rather than a Custom Tab.
            SystemBrowser.launch(activity, Uri.parse(url), browserOnly = true, customTabs = wait.callback.scheme != "https", start = { intent, options ->
                val browser = intent.`package`
                if (wait.callback.scheme == "https" && browser != null && CustomTabsClient.isAuthTabSupported(activity, browser)) {
                    // The Activity host adapts its platform result callback to Auth Tab's AndroidX launcher.
                    val code = 0x6000 + (nextResult++ % 0x1000)
                    wait.resultCode = code
                    val launcher = object : ActivityResultLauncher<Intent>() {
                        override fun launch(input: Intent, options: ActivityOptionsCompat?) {
                            activity.startActivityForResult(input, code, options?.toBundle())
                        }
                        override fun unregister() {}
                        override val contract: ActivityResultContract<Intent, *> get() = Auth.contract
                    }
                    val tab = AuthTabIntent.Builder().build()
                    tab.intent.setPackage(browser)
                    tab.launch(launcher, Uri.parse(url), wait.callback.host!!, wait.callback.path!!)
                } else {
                    wait.resultCode = null
                    activity.startActivity(intent, options)
                }
            })
        } catch (_: android.content.ActivityNotFoundException) {
            end(id)
            Exact.answer(id, "unavailable")
            return
        } catch (_: RuntimeException) {
            // The exception may carry the authorization URL. It never enters a log or the answer.
            end(id)
            Exact.answer(id, "failed")
            return
        }
        handler.postDelayed(timeout, bound)
        Exact.answer(id, "opened")
    }

    fun end(id: Int) { waits.remove(id)?.let { handler.removeCallbacks(it.timeout) } }

    fun stopped() { waits.values.forEach { it.stopped = true } }

    private fun expire() {
        // Handler delays exclude deep sleep; the auth deadline includes it.
        val now = SystemClock.elapsedRealtime()
        val expired = waits.filterValues { now >= it.expires }.keys.toList()
        for (id in expired) { end(id); Exact.answer(id, "cancelled") }
    }

    /** Auth Tab settles through its result; other browsers cancel on return after onStop.
     * A pause alone can be a permission dialog or a focus change while the browser is still open.
     * A delivered callback leaves settlement to the runner; open and answer are ordered on its owner thread. */
    fun resumed() {
        expire()
        val cancelled = waits.filterValues { it.stopped && it.resultCode == null }.keys.toList()
        waits.values.forEach { it.stopped = false }
        for (id in cancelled) { end(id); Exact.answer(id, "cancelled") }
    }

    fun result(activity: ExactActivity, code: Int, result: Int, data: Intent?): Boolean {
        if (code !in 0x6000..0x6fff) return false
        expire()
        val id = waits.entries.firstOrNull { it.value.resultCode == code }?.key ?: return true
        when (result) {
            AuthTabIntent.RESULT_OK -> {
                val uri = data?.data
                if (uri == null || !endpoint(waits.getValue(id).callback, uri) || !receive(activity, Intent(Intent.ACTION_VIEW, uri))) {
                    end(id); Exact.answer(id, "failed")
                }
            }
            AuthTabIntent.RESULT_CANCELED -> { end(id); Exact.answer(id, "cancelled") }
            else -> { end(id); Exact.answer(id, "failed") }
        }
        return true
    }

    // Userinfo belongs to the endpoint but fails the runner's callback check.
    private fun endpoint(want: Uri, got: Uri): Boolean =
        want.scheme.equals(got.scheme, ignoreCase = true) &&
            want.encodedAuthority.equals(got.encodedAuthority?.substringAfterLast('@'), ignoreCase = true) &&
            want.encodedPath == got.encodedPath

    fun claims(context: Context, uri: Uri): Boolean {
        val text = context.packageManager.getApplicationInfo(context.packageName, PackageManager.GET_META_DATA)
            .metaData?.getString("dev.exact.auth.callbacks") ?: return false
        val callbacks = org.json.JSONArray(text)
        return (0 until callbacks.length()).any {
            val want = Uri.parse(callbacks.getString(it))
            endpoint(want, uri)
        }
    }

    /** Clear the activity's saved intent too: a callback is single use and must never become a launch location. */
    fun receive(activity: ExactActivity, intent: Intent): Boolean {
        val uri = intent.data ?: return false
        if (!claims(activity, uri)) return false
        intent.data = null
        if (Exact.live && intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY == 0) {
            expire()
            val waiting = waits.values.filter { endpoint(it.callback, uri) }
            // Even a stale tab's callback caused this return; it is not a browser dismissal.
            waiting.forEach { it.stopped = false }
            fun decode(text: String) = Uri.decode(text.replace("+", " "))
            val params = listOf(uri.encodedQuery, uri.encodedFragment).flatMap { part ->
                part.orEmpty().split('&').map { pair -> decode(pair.substringBefore('=')) to decode(pair.substringAfter('=', "")) }
            }
            val states = params.filter { it.first == "state" }.map { it.second }
            val hasResponse = params.any { it.first in setOf("code", "error", "id_token", "access_token") }
            // The runner selects the matching sign-in first and rejects a logout's sign-in answer with 502;
            // a state-less return without one goes to the oldest logout (LLP 1069.006 D5a).
            if (waiting.any { it.state in states } || states.isEmpty() && !hasResponse && waiting.any { it.logout }) {
                Exact.open(uri.toString())
            }
        }
        return true
    }
}
