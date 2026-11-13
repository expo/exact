package dev.exact.canvas

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock

/** Only the wait lives in this process. Recreating the activity preserves it; killing the process does not. */
internal object Auth {
    private val handler = Handler(Looper.getMainLooper())
    private class Wait(val callback: Uri, val state: String, val timeout: Runnable, val expires: Long, var stopped: Boolean = false)
    private val waits = HashMap<Int, Wait>()

    fun open(activity: ExactActivity, id: Int, url: String, callback: String, state: String) {
        end(id)
        try {
            SystemBrowser.launch(activity, Uri.parse(url), browserOnly = true)
        } catch (_: android.content.ActivityNotFoundException) {
            Exact.answer(id, "unavailable")
            return
        } catch (_: RuntimeException) {
            // The exception may carry the authorization URL. It never enters a log or the answer.
            Exact.answer(id, "failed")
            return
        }
        val timeout = Runnable { waits.remove(id); Exact.answer(id, "cancelled") }
        val bound = 10 * 60 * 1000L
        waits[id] = Wait(Uri.parse(callback), state, timeout, SystemClock.elapsedRealtime() + bound)
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

    /** A return after onStop without a callback cancels. A pause alone can be a permission dialog
     * or a focus change while the browser is still open. A delivered callback leaves settlement
     * to the runner; its open and answer calls are ordered on the owner thread. */
    fun resumed() {
        expire()
        val cancelled = waits.filterValues { it.stopped }.keys.toList()
        waits.values.forEach { it.stopped = false }
        for (id in cancelled) { end(id); Exact.answer(id, "cancelled") }
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
            val states = uri.encodedQuery.orEmpty().split('&').mapNotNull { pair ->
                fun decode(text: String) = Uri.decode(text.replace("+", " "))
                if (decode(pair.substringBefore('=')) == "state") decode(pair.substringAfter('=', "")) else null
            }
            if (waiting.any { it.state in states }) Exact.open(uri.toString())
        }
        return true
    }
}
