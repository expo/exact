package dev.exact.canvas

import android.net.Uri
import android.webkit.WebView
import android.webkit.CookieManager
import androidx.webkit.ProfileStore
import androidx.webkit.WebStorageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONObject

/**
 * An iframe's `partition` (LLP 1020.002 D1): a name, `persist:` and then 1 to 64 of `[A-Za-z0-9._-]`, or the same
 * without `persist:`. A `persist:` partition is an AndroidX WebKit profile of the app's, which keeps its cookies,
 * storage and cache across launches. Android has no profile in memory, so a name without `persist:` is refused, as
 * is any partition where the WebView has no profiles. A refused frame shows `about:blank` and fires `navigateerror`,
 * and is never loaded in the default store.
 */
internal object Partitions {
    private val NAME = Regex("[A-Za-z0-9._-]{1,64}")
    private val FEATURES = listOf(
        WebViewFeature.MULTI_PROFILE, WebViewFeature.NAVIGATION_LISTENER, WebViewFeature.JS_INJECTION_IN_FRAME_AND_WORLD,
        WebViewFeature.DOCUMENT_START_SCRIPT, WebViewFeature.WEB_MESSAGE_LISTENER,
    )

    /** Whether this device's WebView keeps partitions: profiles, and the navigation listener their frames report
     * by. Asking loads the WebView provider, so only a frame with a `partition`, or a plan with an iframe, asks. */
    val kept: Boolean
        get() = WebViewFeature.isFeatureSupported(WebViewFeature.MULTI_PROFILE) &&
            WebViewFeature.isFeatureSupported(WebViewFeature.NAVIGATION_LISTENER)

    /** `exactPage().iframePartition`, asked by the host at boot (`canvas/frame_partitions.rs`), with what the
     * WebView supports in the log. */
    @JvmStatic fun keeps(): Boolean {
        PaintedOrigins.planHasFrames = true
        val supported = FEATURES.associateWith { WebViewFeature.isFeatureSupported(it) }
        android.util.Log.i("exact", "iframe partitions: WebView ${WebView.getCurrentWebViewPackage()?.versionName}; $supported")
        return kept
    }

    fun valid(name: String) = NAME.matches(name.removePrefix("persist:"))

    /** Why a frame with these props is not shown: a DOMException name and a message, or null. */
    fun refusal(props: JSONObject): Pair<String, String>? {
        val partition = props.partition() ?: return null
        val src = props.text("src")
        return when {
            !valid(partition) -> "SyntaxError" to "partition \"$partition\" is not a partition name: persist: and 1 " +
                "to 64 of A-Z a-z 0-9 . _ -, or the same without persist:"
            !partition.startsWith("persist:") -> "NotSupportedError" to "partition \"$partition\": Android keeps " +
                "only persist: partitions; it has no profile in memory"
            !kept -> "NotSupportedError" to "partition \"$partition\": this device's WebView has no profiles " +
                "(MULTI_PROFILE) or no navigation listener (NAVIGATION_LISTENER)"
            !props.isNull("sandbox") -> "NotSupportedError" to "an iframe with a partition takes no sandbox: a " +
                "top-level WebView cannot sandbox its page"
            !web(src) -> "NotSupportedError" to "a partitioned frame shows web pages; $src is not an http, https or " +
                "about:blank URL"
            else -> null
        }
    }

    /** The schemes a partitioned frame shows: http and https with a host, and `about:blank` (an empty `src`). */
    fun web(src: String): Boolean {
        if (src.isEmpty() || src.equals("about:blank", ignoreCase = true)) return true
        val uri = Uri.parse(src)
        return uri.scheme?.lowercase() in setOf("http", "https") && !uri.host.isNullOrEmpty()
    }

    /** The profile a frame with these props shows its pages in, or null for the default store. Under the agent a
     * profile of its own, so a drive never reads or writes a person's saved logins. */
    fun profile(props: JSONObject): String? {
        return props.partition()?.takeIf { refusal(props) == null }?.let(::profile)
    }

    /** A `persist:` partition's profile. */
    fun profile(partition: String) = (if (agentCarried()) "exact.agent." else "exact.persist.") + partition.removePrefix("persist:")

    /** The profiles frames were made in since [flush] last ran, and whether any frame was made: what it writes. */
    private val entered = mutableSetOf<String>()
    var made = false

    /** [web]'s profile, made if new, before anything else touches the view (`WebViewCompat.setProfile`). */
    fun enter(web: WebView, profile: String) {
        WebViewCompat.setProfile(web, profile)
        entered += profile
    }

    /**
     * Writes the cookies of every store a frame used to disk, at the activity's stop, before Android may end the
     * process. Chromium commits its cookie store on a timer, so a cookie set in the seconds before the process
     * ends is otherwise lost. [CookieManager.flush] blocks until it is written. A profile no [live] frame shows is
     * not flushed again.
     */
    fun flush(live: Set<String>) {
        if (!made) return
        CookieManager.getInstance().flush()
        if (entered.isEmpty()) return
        val store = ProfileStore.getInstance()
        for (name in entered) store.getProfile(name)?.cookieManager?.flush()
        entered.retainAll(live)
    }
}

/**
 * `clearSiteData` (LLP 1020.002 D4) over a `persist:` partition's profile, for `@exact/browser`'s Android
 * implementation, on the main thread. `done` is called with null, or a DOMException name and message.
 *
 * - `"cookies"`: the profile's `CookieManager.removeAllCookies`, settled in its callback.
 * - `"*"`: `WebStorageCompat.deleteBrowsingData`, everything the profile holds, settled in its callback where the
 *   WebView has `DELETE_BROWSING_DATA`.
 * - `"storage"` and `"cache"` without `"*"`: `NotSupportedError`. WebView clears neither alone: `WebStorage.deleteAllData`
 *   reports no completion and leaves service workers, and a profile's cache has no clear of its own.
 */
object FrameStorage {
    @JvmStatic fun clearSiteData(partition: String, types: List<String>, done: (Pair<String, String>?) -> Unit) {
        fun fail(name: String, message: String) = done(name to "clearSiteData: $message")
        try {
            if (!partition.startsWith("persist:") || !Partitions.valid(partition)) {
                return fail("NotSupportedError", "Android keeps only persist: partitions; $partition is not one")
            }
            if (!Partitions.kept) return fail("NotSupportedError", "this device's WebView keeps no partitions")
            // Looked up by name first: `getProfile` loads a profile for the rest of the process.
            val name = Partitions.profile(partition)
            if (name !in ProfileStore.getInstance().allProfileNames) {
                return fail("NotFoundError", "no frame of this app has named the partition $partition")
            }
            types.firstOrNull { it !in setOf("cookies", "storage", "cache", "*") }?.let {
                return fail("SyntaxError", "$it is not a type; the types are \"cookies\", \"storage\", \"cache\" and \"*\"")
            }
            val profile = ProfileStore.getInstance().getProfile(name) ?: return fail("NotFoundError", "the profile of $partition is gone")
            if ("*" in types) {
                if (!WebViewFeature.isFeatureSupported(WebViewFeature.DELETE_BROWSING_DATA)) {
                    return fail("NotSupportedError", "this device's WebView cannot clear a profile (DELETE_BROWSING_DATA)")
                }
                return WebStorageCompat.deleteBrowsingData(profile.webStorage) { done(null) }
            }
            types.firstOrNull { it == "storage" || it == "cache" }?.let {
                return fail("NotSupportedError", "Android's WebView clears a profile's $it only with all its data (\"*\")")
            }
            if ("cookies" !in types) return done(null)
            profile.cookieManager.removeAllCookies { profile.cookieManager.flush(); done(null) }
        } catch (e: RuntimeException) {
            // The WebView provider failed to load or answer.
            fail("NotSupportedError", "the WebView could not clear: $e")
        }
    }
}

/** A string prop, empty where the painted host sent `null`. */
internal fun JSONObject.text(key: String): String = if (isNull(key)) "" else optString(key)

/** An iframe's `partition`: null for none, as an empty name is a name, refused. */
internal fun JSONObject.partition(): String? = if (isNull("partition")) null else optString("partition")

/** A `FrameNavigation` record for a frame with no navigation of its own to report (LLP 1020.002 D2). */
internal fun refusalRecord(error: String, message: String, partitioned: Boolean): String = JSONObject()
    .put("url", "").put("destination", "").put("navigationId", 0).put("navigationType", "push")
    .put("title", "").put("icons", org.json.JSONArray()).put("canGoBack", false).put("canGoForward", false)
    .put("loading", false).put("partitioned", partitioned).put("controller", "person")
    .put("error", error).put("message", message).put("recovering", false)
    .toString()
