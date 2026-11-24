package dev.exact.canvas

import android.annotation.SuppressLint
import android.net.Uri
import android.os.Looper
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebView
import androidx.webkit.JavaScriptExecutionException
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.Navigation
import androidx.webkit.NavigationListener
import androidx.webkit.Page
import androidx.webkit.ScriptHandler
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import androidx.webkit.WebViewOutcomeReceiver
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowLog
import java.io.File
import java.lang.reflect.InvocationHandler
import java.lang.reflect.Method
import java.util.concurrent.Callable
import java.util.concurrent.Executor

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [
    NativesFrameTest.Host::class, NativesFrameTest.Compat::class, NativesFrameTest.Features::class,
])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@SuppressLint("RestrictedApi")
class NativesFrameTest {
    data class Load(val id: Int, val slot: Int, val generation: Int, val session: Long)

    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            val loads = ArrayList<Load>()

            @JvmStatic @Implementation fun frameLoaded(id: Int, slot: Int, generation: Int, session: Long) {
                loads.add(Load(id, slot, generation, session))
            }
        }
    }

    @Implements(WebViewFeature::class, isInAndroidSdk = false)
    class Features {
        companion object {
            var navigationListener = true
            var webMessages = false
            var documentStart = false

            @JvmStatic @Implementation fun isFeatureSupported(feature: String) = when (feature) {
                WebViewFeature.NAVIGATION_LISTENER -> navigationListener
                WebViewFeature.WEB_MESSAGE_LISTENER -> webMessages
                WebViewFeature.DOCUMENT_START_SCRIPT -> documentStart
                else -> false
            }
        }
    }

    @Implements(WebViewCompat::class, isInAndroidSdk = false)
    class Compat {
        companion object {
            val listeners = HashMap<WebView, NavigationListener>()
            val messages = HashMap<WebView, WebViewCompat.WebMessageListener>()
            val startScripts = HashMap<WebView, String>()

            @JvmStatic @Implementation
            fun addNavigationListener(web: WebView, executor: Executor, listener: NavigationListener) {
                listeners[web] = listener
            }

            @JvmStatic @Implementation
            fun addWebMessageListener(web: WebView, name: String, origins: Set<String>, listener: WebViewCompat.WebMessageListener) {
                messages[web] = listener
            }

            @JvmStatic @Implementation
            fun addDocumentStartJavaScript(web: WebView, script: String, origins: Set<String>): ScriptHandler {
                assertTrue(messages.containsKey(web))
                assertNull(startScripts.put(web, script))
                return ScriptHandler { startScripts.remove(web) }
            }
        }
    }

    private class Document : JavaScriptReplyProxy() {
        override fun postMessage(message: String) = error("Unexpected reply to page")
        override fun postMessage(message: ByteArray) = error("Unexpected reply to page")
        override fun executeJavaScript(
            script: String, receiver: WebViewOutcomeReceiver<String, JavaScriptExecutionException>?,
        ) = error("Unexpected script in reply world")
    }

    private class Peer(private val properties: Map<String, Any?> = emptyMap()) : InvocationHandler {
        private var peer: Any? = null

        override fun invoke(proxy: Any, method: Method, args: Array<out Any?>?): Any? {
            if (method.name != "getOrCreatePeer") return properties.getValue(method.name)
            return peer ?: (args!!.single() as Callable<*>).call().also { peer = it }
        }
    }

    private fun navigation(page: Peer? = null, sameDocument: Boolean = false): Navigation =
        Navigation.forInvocationHandler(Peer(mapOf(
            "getPage" to page, "isSameDocument" to sameDocument, "wasInitiatedByPage" to false,
        )))

    private fun listener(web: WebView) = Compat.listeners.getValue(web)

    private fun commit(web: WebView, page: Peer, sameDocument: Boolean = false) = with(listener(web)) {
        val nav = navigation(page, sameDocument)
        onNavigationStarted(nav)
        onNavigationCompleted(nav)
    }

    private fun loaded(web: WebView, page: Peer) = listener(web).onPageLoadEvent(Page.forInvocationHandler(page))

    private fun finish(web: WebView) = Peer().let { page ->
        commit(web, page)
        loaded(web, page)
    }

    private fun finishUrl(web: WebView, url: String, ready: Boolean = true) = with(shadowOf(web)) {
        val previous = lastEvaluatedJavascriptCallback
        webViewClient.onPageFinished(web, url)
        val current = lastEvaluatedJavascriptCallback
        if (current !== previous) current?.onReceiveValue(ready.toString())
    }

    private fun commitUrl(web: WebView, url: String) = with(shadowOf(web).webViewClient) {
        onPageStarted(web, url, null)
        doUpdateVisitedHistory(web, url, false)
    }

    private fun natives() = Natives(RuntimeEnvironment.getApplication(), { File(".") }, emptyMap())

    private fun props(sandbox: String? = null): String = JSONObject().put("src", "https://example.com/")
        .put("sandbox", sandbox ?: JSONObject.NULL).toString()

    @After fun resetFeature() {
        Features.navigationListener = true
        Features.webMessages = false
        Features.documentStart = false
        Compat.messages.clear()
        Compat.startScripts.clear()
    }

    @Test fun aFrameTakesTheContactsTheEngineNamesItFor() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(), 10)
        val web = natives.getChildAt(0) as WebView
        assertSame(web, natives.taker(1, 0, 0, 0L))
        assertNull("another allocation's", natives.taker(1, 0, 1, 0L))
        natives.destroyAll()
    }

    @Test fun documentStartReportsLoadsBeforeFinishAndKeepsRecoveryAndCleanupSilent() {
        Features.navigationListener = false
        Features.webMessages = true
        Features.documentStart = true
        Host.loads.clear()
        val natives = natives().apply { lazy = true }
        natives.ensure(1, 1, 0, 0, props(), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        fun token(view: WebView) = Regex("[0-9a-f-]{36}").find(Compat.startScripts.getValue(view))!!.value
        fun message(view: WebView, token: String, document: Document = Document()) =
            Compat.messages.getValue(view).onPostMessage(
                view, WebMessageCompat(token), Uri.parse("https://example.com"), true, document,
            )
        val firstToken = token(web)
        val firstDocument = Document()
        message(web, firstToken, firstDocument)
        message(web, firstToken, firstDocument)
        message(web, firstToken)
        shadowOf(Looper.getMainLooper()).idle()
        val first = Load(1, 0, 0, 0)
        assertEquals(listOf(first, first), Host.loads)

        natives.near(emptySet(), emptySet(), emptySet())
        assertFalse(Compat.startScripts.containsKey(web))
        finishUrl(web, shadowOf(web).lastLoadedUrl)
        natives.ensure(2, 1, 0, 0, props(), 20)
        natives.near(setOf(20), setOf(20), setOf(20))
        assertSame(web, natives.getChildAt(0))
        message(web, firstToken)
        message(web, token(web))
        shadowOf(Looper.getMainLooper()).idle()
        val second = Load(2, 0, 0, 0)
        assertEquals(listOf(first, first, second), Host.loads)

        assertTrue(gone(web))
        val fresh = natives.getChildAt(0) as WebView
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse(Compat.startScripts.containsKey(fresh))
        val recovered = shadowOf(fresh).lastLoadedUrl
        commitUrl(fresh, recovered)
        finishUrl(fresh, recovered)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(first, first, second), Host.loads)
        message(fresh, token(fresh))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(first, first, second, second), Host.loads)
        natives.destroyAll()
    }

    @Test fun aFailedNavigationWaitsForItsErrorDocumentToLoad() {
        Features.navigationListener = false
        Host.loads.clear()
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(), 0)
        val web = natives.getChildAt(0) as WebView
        val url = "https://example.org/unreachable"
        val client = shadowOf(web).webViewClient
        val request = object : WebResourceRequest {
            override fun getUrl() = Uri.parse(url)
            override fun isForMainFrame() = true
            override fun isRedirect() = true
            override fun hasGesture() = false
            override fun getMethod() = "GET"
            override fun getRequestHeaders() = emptyMap<String, String>()
        }
        client.onPageStarted(web, url, null)
        client.onReceivedError(web, request, null)
        finishUrl(web, url, ready = false)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())
        client.doUpdateVisitedHistory(web, url, false)
        shadowOf(web).lastEvaluatedJavascriptCallback!!.onReceiveValue("true")
        shadowOf(Looper.getMainLooper()).idle()
        finishUrl(web, url)
        assertEquals(listOf(Load(1, 0, 0, 0)), Host.loads)
        natives.destroyAll()
    }

    @Test fun documentLoadsWithTheNavigationListenerDisabled() {
        Features.navigationListener = false
        Features.webMessages = true
        Host.loads.clear()
        val natives = natives().apply { lazy = true }
        natives.ensure(1, 1, 0, 0, props(), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        val first = "https://example.com/credits"
        commitUrl(web, first)
        val channel = Compat.messages.getValue(web)
        fun currentToken() = Regex("[0-9a-f-]{36}").find(shadowOf(web).lastEvaluatedJavascript)!!.value
        val token = currentToken()
        fun message(document: Document, data: String = token, main: Boolean = true) =
            channel.onPostMessage(web, WebMessageCompat(data), Uri.parse("https://example.com"), main, document)

        // A retry that returns 204 can finish with the current document's URL while it is still loading.
        finishUrl(web, first)
        val credits = Document()
        message(credits, "guest")
        message(credits, main = false)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())

        val second = "https://example.com/next"
        commitUrl(web, second)
        message(credits)
        message(credits)
        message(Document())
        finishUrl(web, second)
        shadowOf(Looper.getMainLooper()).idle()
        val load = Load(1, 0, 0, 0)
        assertEquals(listOf(load, load), Host.loads)

        natives.near(emptySet(), emptySet(), emptySet())
        finishUrl(web, shadowOf(web).lastLoadedUrl)
        natives.ensure(2, 1, 0, 0, props(), 20)
        natives.near(setOf(20), setOf(20), setOf(20))
        assertSame(web, natives.getChildAt(0))
        commitUrl(web, first)
        message(Document())
        message(Document(), currentToken())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(load, load, Load(2, 0, 0, 0)), Host.loads)
        natives.destroyAll()
    }

    @Test fun aFrameLoadsOncePerDocumentWithoutTheNavigationListener() {
        Features.navigationListener = false
        Host.loads.clear()
        val natives = natives().apply { lazy = true }
        val load = Load(1, 3, 4, 0x12345678fedcba98L)
        val props = JSONObject(props()).put("slot", load.slot).put("generation", load.generation)
            .put("session", load.session).toString()
        natives.ensure(load.id, 1, 100, 50, props, 10)
        assertEquals(0, natives.childCount)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        assertFalse(Compat.listeners.containsKey(web))
        val url = "https://example.org/credits"
        val client = shadowOf(web).webViewClient
        client.onPageStarted(web, shadowOf(web).lastLoadedUrl, null)
        client.doUpdateVisitedHistory(web, url, false)
        val fragment = "$url#two"
        client.doUpdateVisitedHistory(web, fragment, false)
        finishUrl(web, fragment)
        finishUrl(web, fragment, ready = false)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())
        finishUrl(web, fragment)
        finishUrl(web, fragment)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(load), Host.loads)

        client.onPageStarted(web, fragment, null)
        client.doUpdateVisitedHistory(web, fragment, true)
        finishUrl(web, fragment)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(load, load), Host.loads)
        natives.destroyAll()
    }

    @Test fun aReusedFrameIgnoresPoolCleanupWithoutTheNavigationListener() {
        Features.navigationListener = false
        Host.loads.clear()
        val natives = natives().apply { lazy = true }
        val blank = "about:blank"
        val props = JSONObject(props()).put("src", blank).toString()
        natives.ensure(1, 1, 0, 0, props, 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        commitUrl(web, blank)
        natives.near(emptySet(), emptySet(), emptySet())
        val cleanup = shadowOf(web).lastLoadedUrl
        finishUrl(web, blank)
        finishUrl(web, cleanup)

        natives.ensure(2, 1, 0, 0, props, 20)
        natives.near(setOf(20), setOf(20), setOf(20))
        assertSame(web, natives.getChildAt(0))
        finishUrl(web, blank)
        commitUrl(web, blank)
        finishUrl(web, cleanup)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())
        finishUrl(web, blank)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(Load(2, 0, 0, 0)), Host.loads)
        natives.destroyAll()
    }

    @Test fun aRecoveredFrameLoadsSilentlyWithoutTheNavigationListener() {
        Features.navigationListener = false
        Host.loads.clear()
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(), 0)
        assertTrue(gone(natives.getChildAt(0) as WebView))
        val fresh = natives.getChildAt(0) as WebView
        shadowOf(Looper.getMainLooper()).idle()
        val url = shadowOf(fresh).lastLoadedUrl
        commitUrl(fresh, url)
        finishUrl(fresh, url)
        finishUrl(fresh, url)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())

        commitUrl(fresh, url)
        finishUrl(fresh, url)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(Load(1, 0, 0, 0)), Host.loads)
        natives.destroyAll()
    }

    @Test fun aLazyFrameReportsTheSameAllocationOncePerDocument() {
        val natives = natives().apply { lazy = true }
        Host.loads.clear()
        val load = Load(1, 3, 4, 0x12345678fedcba98L)
        val props = JSONObject(props()).put("slot", load.slot).put("generation", load.generation)
            .put("session", load.session).toString()
        natives.ensure(load.id, 1, 100, 50, props, 10)
        assertEquals(0, natives.childCount)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        val page = Peer()
        commit(web, page)
        loaded(web, page)
        commit(web, page, sameDocument = true)
        loaded(web, page)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(load), Host.loads)
        finish(web)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(load, load), Host.loads)
        natives.destroyAll()
    }

    @Test fun aFinishedDocumentCanReportAfterTheNextDocumentCommits() {
        val natives = natives()
        Host.loads.clear()
        natives.ensure(1, 1, 0, 0, props(), 0)
        val web = natives.getChildAt(0) as WebView
        val first = Peer()
        val second = Peer()
        commit(web, first)
        commit(web, second)
        loaded(web, first)
        loaded(web, second)
        shadowOf(Looper.getMainLooper()).idle()
        val load = Load(1, 0, 0, 0)
        assertEquals(listOf(load, load), Host.loads)
        natives.destroyAll()
    }

    @Test fun aFinishedDocumentCanReportAfterItsRendererEnds() {
        val natives = natives().apply { lazy = true }
        Host.loads.clear()
        natives.ensure(1, 1, 0, 0, props(), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        finish(web)
        assertTrue(gone(web))
        shadowOf(Looper.getMainLooper()).idle()
        val load = Load(1, 0, 0, 0)
        assertEquals(listOf(load), Host.loads)

        val fresh = natives.getChildAt(0) as WebView
        finish(fresh)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(load), Host.loads)

        finish(fresh)
        natives.near(emptySet(), emptySet(), emptySet())
        finishUrl(fresh, shadowOf(fresh).lastLoadedUrl)
        natives.near(setOf(10), setOf(10), setOf(10))
        assertSame(fresh, natives.getChildAt(0))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(load), Host.loads)
        natives.destroyAll()
    }

    @Test fun aVerifiedLoadCanReportAfterItsRendererEnds() {
        Features.navigationListener = false
        Host.loads.clear()
        val natives = natives().apply { lazy = true }
        val load = Load(1, 3, 4, 0x12345678fedcba98L)
        val props = JSONObject(props()).put("slot", load.slot).put("generation", load.generation)
            .put("session", load.session).toString()
        natives.ensure(load.id, 1, 100, 50, props, 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        val url = shadowOf(web).lastLoadedUrl
        commitUrl(web, url)
        val client = shadowOf(web).webViewClient
        client.onPageFinished(web, url)
        val first = shadowOf(web).lastEvaluatedJavascriptCallback!!
        client.onPageFinished(web, url)
        val duplicate = shadowOf(web).lastEvaluatedJavascriptCallback!!
        assertTrue(gone(web))
        shadowOf(Looper.getMainLooper()).idle()
        val fresh = natives.getChildAt(0) as WebView
        commitUrl(fresh, url)
        finishUrl(fresh, url)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())

        first.onReceiveValue("true")
        duplicate.onReceiveValue("true")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(load), Host.loads)

        commitUrl(fresh, url)
        shadowOf(fresh).webViewClient.onPageFinished(fresh, url)
        val retired = shadowOf(fresh).lastEvaluatedJavascriptCallback!!
        assertTrue(gone(fresh))
        natives.near(emptySet(), emptySet(), emptySet())
        natives.near(setOf(10), setOf(10), setOf(10))
        retired.onReceiveValue("true")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(load), Host.loads)
        natives.destroyAll()
    }

    @Test fun navigationWithoutANewDocumentLeavesThePendingLoadInPlace() {
        val natives = natives()
        Host.loads.clear()
        natives.ensure(1, 1, 0, 0, props(), 0)
        val web = natives.getChildAt(0) as WebView
        val page = Peer()
        commit(web, page)
        commit(web, page, sameDocument = true)
        with(listener(web)) {
            val canceled = navigation()
            onNavigationStarted(canceled)
            onNavigationCompleted(canceled)
        }
        finishUrl(web, "https://example.com/")
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())
        loaded(web, page)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(Load(1, 0, 0, 0)), Host.loads)
        natives.destroyAll()
    }

    @Test fun aPresentSandboxNeverCreatesAWebView() {
        val natives = natives()
        for (sandbox in listOf("", "allow-scripts", "allow-scripts allow-same-origin")) {
            natives.ensure(1, 1, 0, 0, props(sandbox), 0)
            assertNull("sandbox=$sandbox", natives.node(1))
            assertEquals(0, natives.childCount)
        }
        assertTrue(ShadowLog.getLogsForTag("exact").any { it.msg == "iframe 1: sandbox is unsupported on Android" })
    }

    /** A remote page is asked for with the origin a browser sends for a frame of a page at `exact.invalid`, in the
     * page's scheme, at its first load and when its `src` changes; a page that is no web URL is given none. */
    @Test fun aRemotePageIsAskedForWithItsParentsOriginAsReferer() {
        val natives = natives()
        fun src(url: String) = JSONObject().put("src", url).put("sandbox", JSONObject.NULL).toString()
        natives.ensure(1, 1, 0, 0, src("https://example.com/a?b"), 0)
        val web = natives.getChildAt(0) as WebView
        assertEquals("https://example.com/a?b", shadowOf(web).lastLoadedUrl)
        assertEquals(mapOf("Referer" to "https://exact.invalid/"), shadowOf(web).lastAdditionalHttpHeaders)
        natives.ensure(1, 1, 0, 0, src("http://example.com/"), 0)
        assertEquals("http://example.com/", shadowOf(web).lastLoadedUrl)
        assertEquals(mapOf("Referer" to "http://exact.invalid/"), shadowOf(web).lastAdditionalHttpHeaders)
        natives.ensure(1, 1, 0, 0, src("//example.com/c"), 0)
        assertEquals("https://example.com/c", shadowOf(web).lastLoadedUrl)
        assertEquals(mapOf("Referer" to "https://exact.invalid/"), shadowOf(web).lastAdditionalHttpHeaders)
        natives.ensure(1, 1, 0, 0, src(""), 0)
        assertEquals("about:blank", shadowOf(web).lastLoadedUrl)
        assertEquals(emptyMap<String, String>(), shadowOf(web).lastAdditionalHttpHeaders)
        natives.destroyAll()
    }

    @Test fun aSandboxOnlyUpdateDestroysTheUnsandboxedPage() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(), 0)
        val web = natives.getChildAt(0) as WebView
        assertEquals("https://example.com/", shadowOf(web).lastLoadedUrl)
        natives.ensure(1, 1, 0, 0, props(""), 0)
        assertTrue(shadowOf(web).wasDestroyCalled())
        assertNull(natives.node(1))
        assertEquals(0, natives.childCount)
        natives.ensure(1, 1, 0, 0, props(), 0)
        val replacement = natives.getChildAt(0) as WebView
        assertNotSame(web, replacement)
        assertTrue(replacement.settings.javaScriptEnabled)
        assertFalse(replacement.settings.allowFileAccess)
        assertEquals("https://example.com/", shadowOf(replacement).lastLoadedUrl)
        natives.destroyAll()
    }

    @Test fun aPooledPageIsInactiveAndCannotBeLentToASandboxedFrame() {
        val natives = natives().apply { lazy = true }
        Host.loads.clear()
        natives.ensure(1, 1, 0, 0, props(), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        val oldPage = Peer()
        commit(web, oldPage)
        finish(web)
        natives.near(emptySet(), emptySet(), emptySet())
        val cleanup = shadowOf(web).lastLoadedUrl
        assertFalse(web.settings.javaScriptEnabled)
        assertFalse(web.settings.allowFileAccess)
        natives.ensure(1, 1, 0, 0, props("allow-scripts"), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        assertNull(natives.node(1))
        assertFalse(web.settings.javaScriptEnabled)
        finishUrl(web, cleanup)
        natives.ensure(2, 1, 0, 0, props(), 20)
        natives.near(setOf(20), setOf(20), setOf(20))
        assertSame(web, natives.getChildAt(0))
        assertTrue(web.settings.javaScriptEnabled)
        assertEquals("https://example.com/", shadowOf(web).lastLoadedUrl)
        assertEquals(mapOf("Referer" to "https://exact.invalid/"), shadowOf(web).lastAdditionalHttpHeaders)
        finishUrl(web, cleanup)
        loaded(web, oldPage)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())
        finish(web)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(Load(2, 0, 0, 0)), Host.loads)
        natives.destroyAll()
    }

    @Test fun aFrameArrivingBeforePoolCleanupGetsItsOwnView() {
        val natives = natives().apply { lazy = true }
        Host.loads.clear()
        natives.ensure(1, 1, 0, 0, props(), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        natives.near(emptySet(), emptySet(), emptySet())
        val cleanup = shadowOf(web).lastLoadedUrl
        val blank = JSONObject(props()).put("src", "about:blank").toString()
        natives.ensure(2, 1, 0, 0, blank, 20)
        natives.near(setOf(20), setOf(20), setOf(20))
        val fresh = natives.getChildAt(1) as WebView
        finishUrl(web, cleanup)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())
        finish(fresh)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(Load(2, 0, 0, 0)), Host.loads)
        natives.destroyAll()
    }

    @Test fun aGuestBlankCompletionDoesNotMakeItsPooledViewReady() {
        val natives = natives().apply { lazy = true }
        Host.loads.clear()
        val blank = "about:blank"
        val blankProps = JSONObject(props()).put("src", blank).toString()
        natives.ensure(1, 1, 0, 0, blankProps, 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        val page = Peer()
        commit(web, page)
        natives.near(emptySet(), emptySet(), emptySet())
        val cleanup = shadowOf(web).lastLoadedUrl
        finishUrl(web, blank)

        natives.ensure(2, 1, 0, 0, blankProps, 20)
        natives.near(setOf(20), setOf(20), setOf(20))
        val borrower = natives.getChildAt(natives.childCount - 1) as WebView
        assertNotSame(web, borrower)
        finishUrl(web, cleanup)
        loaded(web, page)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())
        finish(borrower)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(Load(2, 0, 0, 0)), Host.loads)
        natives.destroyAll()
    }

    private fun gone(web: WebView): Boolean = shadowOf(web).webViewClient.onRenderProcessGone(web, object : RenderProcessGoneDetail() {
        override fun didCrash() = true
        override fun rendererPriorityAtExit() = WebView.RENDERER_PRIORITY_IMPORTANT
    })

    /** A frame outlives its renderer, as on Apple (LLP 1020 D2): the dead view goes and a new one loads `src`. */
    @Test fun aFrameWhoseRendererEndsLoadsItsSourceInANewView() {
        val natives = natives()
        Host.loads.clear()
        natives.ensure(1, 1, 100, 50, props(), 0)
        val web = natives.getChildAt(0) as WebView
        assertTrue(gone(web))
        assertTrue(shadowOf(web).wasDestroyCalled())
        assertEquals(1, natives.childCount)
        val fresh = natives.getChildAt(0) as WebView
        assertNotSame(web, fresh)
        assertEquals(100, fresh.layoutParams.width)
        assertNull(shadowOf(fresh).lastLoadedUrl)
        natives.ensure(1, 1, 100, 50, props(), 0)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("https://example.com/", shadowOf(fresh).lastLoadedUrl)
        assertEquals(mapOf("Referer" to "https://exact.invalid/"), shadowOf(fresh).lastAdditionalHttpHeaders)
        assertTrue(ShadowLog.getLogsForTag("exact").any { it.msg == "iframe 1: the page's renderer crashed; loading its src again" })
        val recovered = Peer()
        commit(fresh, recovered)
        val fragment = JSONObject(props()).put("src", "https://example.com#two").toString()
        natives.ensure(1, 1, 100, 50, fragment, 0)
        commit(fresh, recovered, sameDocument = true)
        val next = Peer()
        commit(fresh, next)
        loaded(fresh, recovered)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(Host.loads.isEmpty())
        loaded(fresh, next)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(Load(1, 0, 0, 0)), Host.loads)
        // A new `src` before the reload is loaded once, and the old one not after it.
        assertTrue(gone(fresh))
        val third = natives.getChildAt(0) as WebView
        natives.ensure(1, 1, 100, 50, JSONObject().put("src", "https://example.org/").put("sandbox", JSONObject.NULL).toString(), 0)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("https://example.org/", shadowOf(third).lastLoadedUrl)
        finish(third)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(Load(1, 0, 0, 0), Load(1, 0, 0, 0)), Host.loads)
        natives.destroyAll()
    }

    /** A pooled view shares the renderer: it is destroyed, never lent. */
    @Test fun aPooledViewWhoseRendererEndsIsNotLent() {
        val natives = natives().apply { lazy = true }
        natives.ensure(1, 1, 0, 0, props(), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        natives.near(emptySet(), emptySet(), emptySet())
        assertTrue(gone(web))
        assertTrue(shadowOf(web).wasDestroyCalled())
        assertEquals(0, natives.childCount)
        natives.ensure(2, 1, 0, 0, props(), 20)
        natives.near(setOf(20), setOf(20), setOf(20))
        assertNotSame(web, natives.getChildAt(0))
        natives.destroyAll()
    }
}
