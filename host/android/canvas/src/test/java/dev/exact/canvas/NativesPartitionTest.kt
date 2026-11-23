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
            var profiles = false
            var navigation = true

            @JvmStatic @Implementation fun isFeatureSupported(feature: String) = when (feature) {
                WebViewFeature.NAVIGATION_LISTENER -> navigation
                WebViewFeature.MULTI_PROFILE -> profiles
                WebViewFeature.WEB_MESSAGE_LISTENER, WebViewFeature.DOCUMENT_START_SCRIPT -> !navigation
                else -> false
            }
        }
    }

    @Implements(WebViewCompat::class, isInAndroidSdk = false)
    class Compat {
        companion object {
            val listeners = HashMap<WebView, NavigationListener>()
            /** Each view's profile, set before anything else was done with it. */
            val profiles = LinkedHashMap<WebView, String>()
            fun last() = profiles.keys.last()

            /** The views given a page-world object, as the old-provider load observer gives one. */
            val pageWorld = HashSet<WebView>()

            @JvmStatic @Implementation
            fun addWebMessageListener(web: WebView, name: String, origins: Set<String>, listener: WebViewCompat.WebMessageListener) {
                pageWorld.add(web)
            }

            @JvmStatic @Implementation
            fun addDocumentStartJavaScript(web: WebView, script: String, origins: Set<String>): androidx.webkit.ScriptHandler =
                androidx.webkit.ScriptHandler { }

            @JvmStatic @Implementation fun setProfile(web: WebView, name: String) {
                assertNull(shadowOf(web).lastLoadedUrl)
                assertNull(shadowOf(web).webViewClient)
                assertNull(profiles.put(web, name))
            }


            @JvmStatic @Implementation
            fun addNavigationListener(web: WebView, executor: Executor, listener: NavigationListener) { listeners[web] = listener }
        }
    }

    private fun natives() = Natives(RuntimeEnvironment.getApplication(), { File(".") }, emptyMap())

    private fun props(partition: String?, sandbox: String? = null): String = JSONObject()
        .put("src", "https://example.com/").put("partition", partition ?: JSONObject.NULL)
        .put("sandbox", sandbox ?: JSONObject.NULL).toString()

    private fun settle() = shadowOf(Looper.getMainLooper()).idle()

    @Before fun clear() { Host.sent.clear(); Host.loads.clear(); Compat.profiles.clear(); Compat.pageWorld.clear(); Features.profiles = false; Features.navigation = true }

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

    private fun profiled(): Natives {
        Features.profiles = true
        return natives()
    }

    /** A `persist:` partition is its own profile in a view made for it, never a lent one, and never lent after. */
    @Test fun aPersistentPartitionIsAProfileOfItsOwn() {
        val natives = profiled()
        natives.ensure(9, 1, 0, 0, props(null), 0)
        natives.ensure(9, 1, 0, 0, JSONObject(props(null)).put("generation", 1).toString(), 0)
        val pooled = natives.getChildAt(0) as WebView
        natives.ensure(1, 1, 0, 0, props("persist:one"), 0)
        natives.ensure(2, 1, 0, 0, props("persist:two"), 0)
        natives.ensure(3, 1, 0, 0, props("persist:one"), 0)
        settle()
        val views = Compat.profiles.keys.toList()
        assertFalse(views.any { it === pooled })
        assertEquals(listOf("exact.persist.one", "exact.persist.two", "exact.persist.one"), views.map { Compat.profiles[it] })
        assertTrue(views.all { shadowOf(it).lastLoadedUrl == "https://example.com/" })
        assertTrue(Host.sent.isEmpty())
        // A partition that changes is a new view in the new profile; the old one is destroyed, not lent.
        natives.ensure(1, 1, 0, 0, props("persist:two"), 0)
        assertTrue(shadowOf(views[0]).wasDestroyCalled())
        assertEquals(4, Compat.profiles.size)
        assertEquals("exact.persist.two", Compat.profiles[Compat.last()])
        // A renderer's end makes the view again in its profile.
        assertTrue(gone(views[1]))
        settle()
        assertEquals(5, Compat.profiles.size)
        assertEquals("exact.persist.two", Compat.profiles[Compat.last()])
        natives.destroyAll()
    }

    /** What Android cannot keep is refused, never loaded in the default store or in a profile. */
    @Test fun whatAProfileCannotKeepIsRefused() {
        val natives = profiled()
        natives.ensure(1, 1, 0, 0, props("work"), 0)
        natives.ensure(2, 1, 0, 0, props("persist:a", sandbox = ""), 0)
        natives.ensure(3, 1, 0, 0, JSONObject(props("persist:a")).put("src", "file:///sdcard/x.html").toString(), 0)
        settle()
        assertTrue(Compat.profiles.isEmpty())
        assertEquals(listOf(1, 2, 3), Host.sent.map { it.id })
        assertTrue(Host.sent.all { it.record.getString("error") == "NotSupportedError" && it.record.getBoolean("partitioned") })
        // A src it can show makes the frame again in its profile.
        natives.ensure(3, 1, 0, 0, props("persist:a"), 0)
        assertEquals("exact.persist.a", Compat.profiles.values.single())
        assertEquals("https://example.com/", shadowOf(Compat.last()).lastLoadedUrl)
        natives.destroyAll()
    }

    /** On a provider without the navigation listener a refused partition is still a partition's view: never lent,
     * and never given the page-world object the load observer installs. */
    @Test fun aRefusedPartitionHasNoPageWorldObjectAndNoLentView() {
        Features.navigation = false
        val natives = natives()
        natives.ensure(9, 1, 0, 0, props(null), 0)
        natives.ensure(9, 1, 0, 0, JSONObject(props(null)).put("generation", 1).toString(), 0)
        val plain = Compat.pageWorld.toSet()
        assertFalse(plain.isEmpty())
        natives.ensure(1, 1, 0, 0, props("persist:a"), 0)
        settle()
        assertEquals("NotSupportedError", Host.sent.single().record.getString("error"))
        assertEquals(plain, Compat.pageWorld)
        natives.destroyAll()
    }
}
