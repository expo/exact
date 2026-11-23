package dev.exact.canvas

import android.os.Looper
import android.webkit.WebView
import android.webkit.RenderProcessGoneDetail
import androidx.webkit.Navigation
import androidx.webkit.NavigationListener
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.io.File
import java.lang.reflect.InvocationHandler
import java.lang.reflect.Method
import java.util.concurrent.Callable
import java.util.concurrent.Executor

/** An iframe's `partition` on Android (LLP 1020.002 D1): honoured as its profile, or refused where the app hears it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [
    NativesPartitionTest.Host::class, NativesPartitionTest.Compat::class, NativesPartitionTest.Features::class,
])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativesPartitionTest {
    data class Sent(val id: Int, val event: String, val record: JSONObject)

    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            val sent = ArrayList<Sent>()

            val loads = ArrayList<Int>()

            @JvmStatic @Implementation fun frameLoaded(id: Int, slot: Int, generation: Int, session: Long) { loads.add(id) }

            @JvmStatic @Implementation
            fun frameEvent(id: Int, slot: Int, generation: Int, session: Long, event: String, record: String) {
                sent.add(Sent(id, event, JSONObject(record)))
            }
        }
    }

    @Implements(WebViewFeature::class, isInAndroidSdk = false)
    class Features {
        companion object {
            @JvmStatic @Implementation fun isFeatureSupported(feature: String) = feature == WebViewFeature.NAVIGATION_LISTENER
        }
    }

    @Implements(WebViewCompat::class, isInAndroidSdk = false)
    class Compat {
        companion object {
            val listeners = HashMap<WebView, NavigationListener>()

            @JvmStatic @Implementation
            fun addNavigationListener(web: WebView, executor: Executor, listener: NavigationListener) { listeners[web] = listener }
        }
    }

    private fun natives() = Natives(RuntimeEnvironment.getApplication(), { File(".") }, emptyMap())

    private fun props(partition: String?, sandbox: String? = null): String = JSONObject()
        .put("src", "https://example.com/").put("partition", partition ?: JSONObject.NULL)
        .put("sandbox", sandbox ?: JSONObject.NULL).toString()

    private fun settle() = shadowOf(Looper.getMainLooper()).idle()

    @Before fun clear() { Host.sent.clear(); Host.loads.clear() }

    private class Peer(private val properties: Map<String, Any?> = emptyMap()) : InvocationHandler {
        private var peer: Any? = null

        override fun invoke(proxy: Any, method: Method, args: Array<out Any?>?): Any? {
            if (method.name != "getOrCreatePeer") return properties.getValue(method.name)
            return peer ?: (args!!.single() as Callable<*>).call().also { peer = it }
        }
    }

    /** A navigation the page did not start, committed and loaded, as WebView reports `loadUrl`'s. */
    private fun finish(web: WebView) = with(Compat.listeners.getValue(web)) {
        val page = Peer()
        val nav = Navigation.forInvocationHandler(Peer(mapOf(
            "getPage" to page, "isSameDocument" to false, "wasInitiatedByPage" to false,
        )))
        onNavigationStarted(nav)
        onNavigationCompleted(nav)
        onPageLoadEvent(androidx.webkit.Page.forInvocationHandler(page))
    }

    private fun gone(web: WebView) = shadowOf(web).webViewClient.onRenderProcessGone(web, object : RenderProcessGoneDetail() {
        override fun didCrash() = true
        override fun rendererPriorityAtExit() = WebView.RENDERER_PRIORITY_IMPORTANT
    })

    @Test fun aPartitionThisHostCannotKeepIsRefusedAndNeverLoaded() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("persist:work"), 0)
        // The sandbox's refusal is the partition's, heard by the app, never a frame silently dropped.
        natives.ensure(2, 1, 0, 0, props("work", sandbox = "allow-scripts"), 0)
        natives.ensure(3, 1, 0, 0, props("not a name"), 0)
        natives.ensure(4, 1, 0, 0, props(null), 0)
        settle()
        val loaded = (0 until natives.childCount).map { shadowOf(natives.getChildAt(it) as WebView).lastLoadedUrl }
        assertEquals(listOf("about:blank", "about:blank", "about:blank", "https://example.com/"), loaded)
        assertEquals(listOf(1, 2, 3), Host.sent.map { it.id })
        assertTrue(Host.sent.all { it.event == "navigateerror" && !it.record.getBoolean("partitioned") })
        assertEquals(listOf("NotSupportedError", "NotSupportedError", "SyntaxError"), Host.sent.map { it.record.getString("error") })
        natives.destroyAll()
    }

    /** The refusal's `about:blank` is no document of the app's, before or after its renderer ends; the refusal is
     * sent once, for a parked frame too, and a renderer's end or a frame shown again repeats none. */
    @Test fun aRefusedFrameStaysBlankAndSilent() {
        val natives = natives().apply { lazy = true }
        natives.ensure(1, 1, 0, 0, props("persist:work"), 10)
        settle()
        assertEquals(listOf("navigateerror"), Host.sent.map { it.event })
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        assertEquals("about:blank", shadowOf(web).lastLoadedUrl)
        finish(web)
        assertTrue(gone(web))
        settle()
        val fresh = natives.getChildAt(0) as WebView
        assertNotSame(web, fresh)
        assertEquals("about:blank", shadowOf(fresh).lastLoadedUrl)
        finish(fresh)
        natives.reroot()
        settle()
        assertEquals(1, Host.sent.size)
        assertTrue(Host.loads.isEmpty())
        natives.destroyAll()
    }

    /** An empty name is a name, refused as Apple refuses it; and a changed partition is a new web view. */
    @Test fun anEmptyPartitionIsRefusedAndAChangedOneReplacesTheView() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(""), 0)
        settle()
        assertEquals("SyntaxError", Host.sent.single().record.getString("error"))
        val refused = natives.getChildAt(0) as WebView
        natives.ensure(1, 1, 0, 0, props(null), 0)
        val shown = natives.getChildAt(natives.childCount - 1) as WebView
        assertNotSame(refused, shown)
        assertEquals("https://example.com/", shadowOf(shown).lastLoadedUrl)
        natives.destroyAll()
    }
}
