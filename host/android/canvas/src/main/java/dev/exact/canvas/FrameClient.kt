package dev.exact.canvas

import android.graphics.Bitmap
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.webkit.Navigation
import androidx.webkit.NavigationListener
import androidx.webkit.Page
import androidx.webkit.WebViewFeature
import java.io.File
import java.util.UUID

/** What a [FrameClient] asks of the elements that show its view ([Natives]). */
internal interface FrameHost {
    /** Where a frame's local files are. */
    val assets: File
    /** Whether an element shows [view] now. */
    fun shows(view: WebView): Boolean
    /** Whether [view] waits in the pool to be lent. */
    fun pooled(view: WebView): Boolean
    /** The `load` of the element showing [view] now, or null. */
    fun loadOf(view: WebView): FrameLoadReport?
    /** [view]'s renderer ended: it crashed, or Android reclaimed it. */
    fun rendererGone(view: WebView, crashed: Boolean)
}

/** One element's `load`, sent only while that element and its event binding are still the frame's. */
internal interface FrameLoadReport {
    fun send()
    /** Whether the element is still the one showing the view. */
    fun current(): Boolean
}

private class FrameLoad(var url: String, val silent: Boolean, val failed: Boolean = false)

/**
 * A frame WebView's client: it reports each document's `load` once (LLP 1020 D1), from [NavigationListener] where the
 * provider has it and from the WebViewClient callbacks otherwise, keeps the guest's navigations in its frame, and hands
 * a renderer's end to [host].
 */
internal class FrameClient(
    private val view: WebView,
    private val hasNavigationListener: Boolean,
    private val host: FrameHost,
) : WebViewClient(), NavigationListener {
    private val documents = if (!hasNavigationListener &&
        WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
        FrameDocuments(view, ::reportLoad)
    } else null
    private val loading = HashSet<Page>()
    private var recovery: Navigation? = null
    private var starting: FrameLoad? = null
    private var finishing: FrameLoad? = null
    private var fragmentFinish: String? = null
    private var cleanupUrl: String? = null
    private var recoveryRequested = false
    var recoveryPending = false
    var readyToLend = false
        private set

    fun requestLoad(silently: Boolean) {
        readyToLend = false
        recoveryPending = false
        cleanupUrl = null
        // The old callbacks can be queued even after WebView has committed the recovery page.
        recoveryRequested = silently || (!hasNavigationListener && recoveryRequested && view.originalUrl != null)
        if (!recoveryRequested) documents?.observeFromStart()
    }

    fun beginCleanup(): String {
        val url = "data:text/html,%3C!--${UUID.randomUUID()}--%3E"
        readyToLend = false
        recoveryPending = false
        loading.clear()
        documents?.reset()
        recoveryRequested = false
        starting = null
        finishing = null
        fragmentFinish = null
        cleanupUrl = url
        return url
    }

    // A guest's navigations stay in its frame, as in a browser's; a file only under the asset root.
    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
        !Sources.navigable(request.url.scheme, request.url.path, host.assets)

    override fun onNavigationStarted(navigation: Navigation) {
        if (recoveryRequested && !navigation.wasInitiatedByPage() && !navigation.isSameDocument) {
            recoveryRequested = false
            recovery = navigation
        }
    }

    override fun onNavigationCompleted(navigation: Navigation) {
        if (navigation === recovery) {
            recovery = null
            return
        }
        if (!navigation.isSameDocument && host.shows(view)) {
            navigation.page?.let(loading::add)
        }
    }

    override fun onPageDeleted(page: Page) {
        loading.remove(page)
    }

    override fun onPageLoadEvent(page: Page) {
        if (!loading.remove(page)) return
        reportLoad()
    }

    private fun reportLoad() {
        host.loadOf(view)?.send()
    }

    override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
        if (hasNavigationListener || cleanupUrl != null || !host.shows(view)) return
        starting = FrameLoad(url, recoveryRequested)
        recoveryRequested = false
    }

    override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
        if (hasNavigationListener || cleanupUrl != null) return
        val start = starting
        if (start != null) {
            starting = null
            start.url = url
            finishing = start
            fragmentFinish = null
        } else {
            val page = finishing ?: return
            // Chromium also sends onPageFinished after a change to the current document's fragment.
            fragmentFinish = url.takeIf { it != page.url && it.substringBefore('#') == page.url.substringBefore('#') }
            page.url = url
        }
        val page = finishing ?: return
        if (page.silent) {
            if (start != null) documents?.observeFromStart()
            return
        }
        if (page.failed) {
            // The error document's URL differs from the failed navigation's URL in WebViewClient.
            if (start != null) observeError(page)
        } else {
            documents?.observe(url)
        }
    }

    override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError?) {
        if (hasNavigationListener || cleanupUrl != null || !request.isForMainFrame || !host.shows(view)) return
        // The error's onPageFinished can precede the error document's commit.
        starting = FrameLoad(request.url.toString(), starting?.silent ?: recoveryRequested, failed = true)
        recoveryRequested = false
    }

    override fun onPageFinished(view: WebView, url: String) {
        if (cleanupUrl != null) {
            if (url == cleanupUrl && host.pooled(view)) readyToLend = true
            return
        }
        if (hasNavigationListener) return
        if (url == fragmentFinish) {
            fragmentFinish = null
            return
        }
        // Check readiness because an aborted navigation can finish with the pending document's URL.
        val page = finishing?.takeIf { it.url == url && starting?.url != url } ?: return
        if (page.failed) return
        if (page.silent) {
            finishing = null
        } else if (documents != null) {
            documents.observe(url, atFinish = true)
        } else {
            verifyLoad(page, url)
        }
    }

    private fun observeError(page: FrameLoad) {
        val url = "chrome-error://chromewebdata/"
        if (documents != null) documents.observe(url) else verifyLoad(page, url)
    }

    private fun verifyLoad(page: FrameLoad, url: String) {
        if (finishing !== page) return
        val load = host.loadOf(view) ?: return
        view.evaluateJavascript(frameDocumentReady(url)) {
            if (finishing !== page || (!page.failed && page.url != url)) {
                return@evaluateJavascript
            }
            if (it == "true") {
                finishing = null
                load.send()
            } else if (it == "false" && page.failed && load.current()) {
                // Older providers have no document message channel and can omit a second finish callback.
                view.postOnAnimation { verifyLoad(page, url) }
            }
        }
    }

    // Android ends the app with the renderer unless this answers true.
    override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
        host.rendererGone(view, detail.didCrash())
        return true
    }
}
