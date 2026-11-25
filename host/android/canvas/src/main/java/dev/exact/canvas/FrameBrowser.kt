package dev.exact.canvas

import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.webkit.WebView
import androidx.webkit.Navigation
import androidx.webkit.Page
import androidx.webkit.WebViewFeature
import org.json.JSONArray
import org.json.JSONObject

/**
 * The navigation state of one kept partitioned frame, reported as `FrameNavigation` records, and the four commands
 * that move it (LLP 1020.002 D2, D3): Apple's `FrameBrowser.swift` on AndroidX WebKit's `NavigationListener`. One per
 * element, so it outlives a web view a renderer's end replaces. [FrameClient] forwards its listener calls here.
 *
 * Each navigation ends in `navigatesuccess` (the page's `load`) or `navigateerror`. A same-document navigation
 * (`pushState`, a fragment) is its three events at once, at its commit, and is not reported while a cross-document
 * one is in flight, as on Apple.
 */
internal class FrameBrowser(
    /** The web view the element shows now, or null while it has none (parked). */
    private val web: () -> WebView?,
    /** The element's `src`. */
    private val source: () -> String,
    contextId: () -> String,
    /** Loads a URL in the element's view, as `src` is loaded. */
    private val load: (String) -> Unit,
    /** Sends an event and its record to the element's handler, while the element is current. */
    private val emit: (String, String) -> Unit,
) {
    private class Flight(val id: Int, var destination: String, val type: String, val navigation: Navigation?) {
        var committed = false
        /** The committed document, whose `load` ends the flight. */
        var page: Page? = null
    }

    private var nextId = 0
    private var flight: Flight? = null
    private var lastId = 0
    private var lastType = "push"
    private var committed = ""
    /** The history's current index at the last commit, to tell a push from a replace. */
    private var index = -1
    /** The host loads by URL what it reloads (a failed or pending load, a crash's recovery): that load is a reload. */
    private var reloading = false
    /** The pages that have fired `load`: a commit of one (a restore from the back-forward cache) has loaded already. */
    private val loadedPages = HashSet<Page>()
    private var lastSent: String? = null
    private var stateScheduled = false
    private val deaths = ArrayList<Long>()
    private var recovery: Runnable? = null
    /** The renderer ended since the last commit: the new view has no document to reload. */
    private var dead = false
    /** The frame shows Chromium's error page for a failed load, which is no page of the site's. */
    private var errorPage = false
    private var closed = false
    private val main = Handler(Looper.getMainLooper())
    /** `@exact/browser-automation` on this frame (D9); a change of its controller is a `currententrychange`. */
    val automation = FrameAutomation(web, contextId) { send("currententrychange") }

    /** The frame's view is gone ([reason]): a navigation in flight ends in `AbortError`, as a superseded one does. */
    fun end(reason: String) {
        cancelRecovery()
        automation.detached()
        flight?.let { flight = null; send("navigateerror", it, "AbortError", reason) }
    }

    fun close() {
        automation.close()
        end("the frame was removed or its partition changed")
        closed = true
    }

    // Records

    private fun record(ending: Flight? = null, error: String = "", message: String = "", recovering: Boolean = false): String {
        val flight = ending ?: this.flight
        val view = web()
        return JSONObject()
            .put("url", committed)
            .put("destination", if (ending == null && flight != null && !flight.committed) flight.destination else "")
            .put("navigationId", flight?.id ?: lastId)
            .put("navigationType", flight?.type ?: lastType)
            .put("title", title(view))
            .put("icons", JSONArray(icons()))
            .put("canGoBack", view?.canGoBack() ?: false)
            .put("canGoForward", view?.canGoForward() ?: false)
            .put("loading", ending == null && flight != null)
            .put("partitioned", true)
            .put("controller", automation.controller)
            .put("error", error)
            .put("message", message)
            .put("recovering", recovering)
            .toString()
    }

    /**
     * `document.title`. Where a page has none, `WebView.getTitle` is its URL without the scheme (Chromium's title for
     * display), which is not the page's title.
     */
    private fun title(view: WebView?): String {
        val title = view?.title?.takeIf { committed.isNotEmpty() && !errorPage } ?: return ""
        val bare = committed.substringAfter("://")
        val untitled = listOf(bare, bare.substringBefore('#')).flatMap { listOf(it, it.removeSuffix("/")) }
        return if (title in untitled) "" else title
    }

    /** The committed origin's `/favicon.ico`: the page's own `<link rel~=icon>` is read only in an isolated world. */
    private fun icons(): List<String> {
        val uri = Uri.parse(committed)
        val scheme = uri.scheme?.lowercase()
        if (scheme != "http" && scheme != "https" || uri.host.isNullOrEmpty()) return emptyList()
        val port = if (uri.port >= 0) ":${uri.port}" else ""
        return listOf("$scheme://${uri.host}$port/favicon.ico")
    }

    private fun send(event: String, ending: Flight? = null, error: String = "", message: String = "", recovering: Boolean = false) {
        if (closed) return
        val value = record(ending, error, message, recovering)
        if (event == "currententrychange" || event == "navigatesuccess") lastSent = stateKey(value)
        emit(event, value)
    }

    /** What a state change compares, so one change is one event. */
    private fun stateKey(record: String): String = JSONObject(record).let { r ->
        listOf("url", "title", "icons", "canGoBack", "canGoForward").joinToString("\u0000") { r.opt(it)?.toString() ?: "" }
    }

    private fun rememberHistory() {
        index = web()?.copyBackForwardList()?.currentIndex ?: return
    }

    // Navigations

    private fun begin(destination: String, type: String, navigation: Navigation?) {
        flight?.let { old ->
            // A navigation replacing one in flight ends the old one first, as the Navigation API aborts it.
            flight = null
            send("navigateerror", old, "AbortError", "superseded by a new navigation")
        }
        nextId += 1
        flight = Flight(nextId, destination, type, navigation)
        lastId = nextId
        lastType = type
        send("navigate")
    }

    fun started(navigation: Navigation) {
        if (closed || navigation.isSameDocument) return
        automation.starting()
        cancelRecovery()
        val reload = reloading && !navigation.wasInitiatedByPage()
        reloading = false
        val type = when {
            navigation.isHistory -> "traverse"
            navigation.isReload || reload -> "reload"
            else -> "push"
        }
        begin(navigation.url, type, navigation)
    }

    fun redirected(navigation: Navigation) {
        val current = flight?.takeIf { it.navigation === navigation } ?: return
        current.destination = navigation.url
        send("navigate")
    }

    fun completed(navigation: Navigation) {
        if (closed) return
        if (navigation.isSameDocument) return sameDocument(navigation)
        if (navigation.didCommit()) automation.committed(navigation.page)
        val current = flight?.takeIf { it.navigation === navigation } ?: return
        if (!navigation.didCommit()) automation.cancelled()
        if (navigation.didCommit() && !navigation.didCommitErrorPage()) {
            dead = false
            errorPage = false
            committed = navigation.url
            current.committed = true
            current.page = navigation.page
            rememberHistory()
            send("currententrychange")
            navigation.page?.takeIf { it in loadedPages }?.let(::loaded)
            return
        }
        flight = null
        // A failed load commits Chromium's error page in the frame: its entry is in the history, and `url` stays the
        // last document that loaded (LLP 1020.002 §11, Android as built).
        if (navigation.didCommitErrorPage()) { errorPage = true; rememberHistory() }
        val failure = if (RESOURCE_ERRORS) navigation.webResourceError else null
        // Cancelled (a new load, a stop, a response with nothing to show such as a 204) is the Navigation API's
        // AbortError; Chromium names it `net::ERR_ABORTED`.
        val aborted = !navigation.didCommitErrorPage() && (failure == null || failure.description == "net::ERR_ABORTED")
        val error = if (aborted) "AbortError" else "NetworkError"
        val message = failure?.let { "${it.errorCode}: ${it.description}" }
            ?: if (error == "AbortError") "the navigation ended before it committed" else "the page failed to load"
        send("navigateerror", current, error, message)
    }

    /** `pushState`, `replaceState`, a fragment or a traversal within the page: its three events at once. */
    private fun sameDocument(navigation: Navigation) {
        if (!navigation.didCommit() || committed.isEmpty()) return
        // While a document loads its own `replaceState` or fragment is not reported, as on Apple, but is its URL.
        flight?.let { if (it.committed) committed = navigation.url; return }
        val type = when {
            navigation.isHistory -> "traverse"
            navigation.isReload -> "reload"
            (web()?.copyBackForwardList()?.currentIndex ?: index) > index -> "push"
            else -> "replace"
        }
        reloading = false
        begin(navigation.url, type, null)
        committed = navigation.url
        flight?.committed = true
        rememberHistory()
        send("currententrychange")
        flight?.let { flight = null; send("navigatesuccess", it) }
    }

    fun loaded(page: Page) {
        loadedPages += page
        val current = flight?.takeIf { it.committed && it.page == page } ?: return
        flight = null
        web()?.url?.let { committed = it }
        rememberHistory()
        send("navigatesuccess", current)
    }

    fun deleted(page: Page) {
        loadedPages -= page
        automation.deleted(page)
    }

    /** A title, or the history with no navigation, changed: `currententrychange` if the record did. */
    fun stateChanged() {
        if (stateScheduled) return
        stateScheduled = true
        main.post {
            stateScheduled = false
            if (closed || flight != null || committed.isEmpty() || stateKey(record()) == lastSent) return@post
            send("currententrychange")
        }
    }

    // Commands (LLP 1020.002 D3)

    private fun refuse(error: String, message: String) {
        android.util.Log.w("exact", "iframe: $message")
        send("navigateerror", error = error, message = message)
    }

    /** One command from the frame's props: `[seq, name, args…]`. */
    fun command(op: JSONArray) {
        val view = web()?.takeIf { !closed } ?: return
        when (op.optString(1)) {
            "navigate" -> {
                val url = op.optString(2)
                if (url.isEmpty() || !Partitions.web(url)) {
                    val scheme = Uri.parse(url).scheme?.let { "$it:" } ?: "this URL"
                    return refuse("NotSupportedError", "navigate: $scheme is not a web page a partitioned frame shows (http, https or about:blank)")
                }
                cancelRecovery()
                load(url)
            }
            "traverseHistory" -> {
                val delta = op.optDouble(2, 0.0).toInt()
                if (delta == 0) return reload(false)
                if (!view.canGoBackOrForward(delta)) return send("navigateerror", error = "InvalidStateError", message = "traverseHistory: no history entry at $delta")
                cancelRecovery()
                view.goBackOrForward(delta)
            }
            "reload" -> reload(op.optBoolean(2, false))
            "stopLoading" -> {
                cancelRecovery()
                val current = flight
                flight = null
                // Also a load asked for whose start WebView has not reported: it then ends uncommitted, AbortError.
                automation.cancelled()
                view.stopLoading()
                current?.let { send("navigateerror", it, "AbortError", "stopLoading") }
            }
        }
    }

    /** `reload(id)`: the committed document again, or what failed or was pending, or `src` when nothing committed. */
    private fun reload(ignoreCache: Boolean) {
        if (ignoreCache) return refuse("NotSupportedError", "reload: Android's WebView has no reload that bypasses the cache (ignoreCache)")
        val view = web() ?: return
        deaths.clear()
        cancelRecovery()
        reloading = true
        val target = flight?.destination ?: if (committed.isEmpty()) source() else if (dead) committed else null
        if (target != null) load(target) else view.reload()
    }

    // Crashes

    /**
     * The renderer ended and [web] is a new view: `navigateerror` "crashed", then a load after 250, 500 and 1,000 ms;
     * a fourth end within 30 s stops until `reload` (LLP 1020.002 D2). The new view has no history.
     */
    fun crashed() {
        if (closed) return
        automation.detached()
        automation.attach()
        val now = SystemClock.uptimeMillis()
        dead = true
        deaths.removeAll { now - it >= 30_000 }
        deaths += now
        val target = flight?.destination ?: committed.ifEmpty { source() }
        val ending = flight ?: Flight(++nextId, "", "reload", null).also { lastId = it.id }
        flight = null
        val attempt = deaths.size
        if (attempt > RECOVERY_DELAYS.size) {
            return send("navigateerror", ending, "crashed", "the page's renderer ended $attempt times in 30 s", recovering = false)
        }
        send("navigateerror", ending, "crashed", "the page's renderer ended", recovering = true)
        cancelRecovery()
        val work = Runnable {
            recovery = null
            if (closed || deaths.lastOrNull() != now) return@Runnable
            reloading = true
            load(target)
        }
        recovery = work
        main.postDelayed(work, RECOVERY_DELAYS[attempt - 1])
    }

    /** A navigation, a new source or a stop owns the frame now: a crash's queued recovery never loads over it. */
    fun cancelRecovery() {
        recovery?.let(main::removeCallbacks)
        recovery = null
    }

    companion object {
        private val RECOVERY_DELAYS = longArrayOf(250, 500, 1_000)
        private val RESOURCE_ERRORS by lazy { WebViewFeature.isFeatureSupported(WebViewFeature.NAVIGATION_GET_WEB_RESOURCE_ERROR) }

        /** The last command each frame allocation ran, so an element made again for it repeats none: the 4,096 that
         * ran one last, as a live frame's record is kept fresh by each props it is read for. */
        private val ran = object : LinkedHashMap<String, Long>(16, 0.75f, true) {
            override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, Long>) = size > 4_096
        }

        /** The commands in [props] that the frame allocation [key] has not run, marked run. */
        fun unrun(key: String, props: JSONObject): List<JSONArray> {
            val all = props.optJSONArray("commands") ?: return emptyList()
            val last = ran[key] ?: 0L
            val out = (0 until all.length()).mapNotNull { all.optJSONArray(it) }.filter { it.optLong(0) > last }
            out.lastOrNull()?.let { ran[key] = it.optLong(0) }
            return out
        }
    }
}
