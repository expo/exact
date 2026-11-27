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

            val journal = ArrayList<String>()
            @JvmStatic @Implementation fun journal(line: String) { journal.add(line) }

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
            var documentStart = true
            var worlds = false

            @JvmStatic @Implementation fun isFeatureSupported(feature: String) = when (feature) {
                WebViewFeature.NAVIGATION_LISTENER -> navigation
                WebViewFeature.MULTI_PROFILE -> profiles
                WebViewFeature.WEB_MESSAGE_LISTENER -> !navigation
                WebViewFeature.DOCUMENT_START_SCRIPT -> documentStart
                WebViewFeature.JS_INJECTION_IN_FRAME_AND_WORLD -> worlds
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

            /** Each view's listeners and document-start scripts in an execution world, by world name. */
            val worldListeners = HashMap<WebView, HashMap<String, WebViewCompat.WebMessageListener>>()
            val worldScripts = HashMap<WebView, ArrayList<Pair<String, String>>>()
            var refuseNextScript = false

            @JvmStatic @Implementation
            fun getExecutionWorld(web: WebView, name: String) = androidx.webkit.JavaScriptExecutionWorld(name, web).also { names[it] = name }
            private val names = java.util.IdentityHashMap<androidx.webkit.JavaScriptExecutionWorld, String>()

            @JvmStatic @Implementation
            fun addWebMessageListener(web: WebView, name: String, origins: Set<String>, world: androidx.webkit.JavaScriptExecutionWorld,
                                      listener: WebViewCompat.WebMessageListener) {
                worldListeners.getOrPut(web) { HashMap() }[names.getValue(world)] = listener
            }

            @JvmStatic @Implementation
            fun removeWebMessageListener(web: WebView, world: androidx.webkit.JavaScriptExecutionWorld, name: String) {
                worldListeners[web]?.remove(names.getValue(world))
            }

            @JvmStatic @Implementation
            fun addJavaScriptOnEvent(web: WebView, script: String, event: Int, origins: Set<String>,
                                     world: androidx.webkit.JavaScriptExecutionWorld): androidx.webkit.ScriptHandler {
                if (refuseNextScript) {
                    refuseNextScript = false
                    throw IllegalStateException("document-start script refused")
                }
                val entry = names.getValue(world) to script
                worldScripts.getOrPut(web) { ArrayList() } += entry
                return androidx.webkit.ScriptHandler { worldScripts[web]?.remove(entry) }
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

    @Test fun aFrameFollowsItsPaintedRowBetweenContacts() {
        val natives = natives()
        val graph = PaintedOrigins()
        natives.origins = graph
        natives.ensure(1, 1, 300, 200, props(null), 0)
        val web = natives.getChildAt(0) as WebView
        val row = android.graphics.RenderNode("row")
        graph.record(row, listOf(PaintedOrigins.Placement(natives.node(1)!!, 12f, 140f)))
        graph.record(null, listOf(PaintedOrigins.Placement(row, 0f, 15f)))
        natives.placeFrames()
        assertEquals(155f, origin(web)[1], 0f)
        graph.publishing = true
        graph.translate(row, 0f, -81f)
        natives.placeFrames()
        assertEquals(155f, origin(web)[1], 0f)
        graph.publishing = false
        natives.placeFrames()
        assertEquals(74f, origin(web)[1], 0f)
        val sibling = android.graphics.RenderNode("unrelated")
        graph.record(null, listOf(PaintedOrigins.Placement(row, 0f, 15f), PaintedOrigins.Placement(sibling, 0f, 0f)))
        natives.placeFrames()
        val version = graph.version
        repeat(120) {
            graph.translate(row, 0f, -81f)
            graph.translate(sibling, 0f, it.toFloat())
            natives.placeFrames()
        }
        assertEquals(version, graph.version)
        assertEquals(12f, origin(web)[0], 0f)
    }

    @Test fun aFrameUsesItsPaintedScaleAndClipAndLeavesTheWindowWhenPooled() {
        val natives = natives()
        val graph = PaintedOrigins()
        natives.origins = graph
        natives.ensure(1, 1, 200, 100, props(null), 0)
        natives.layoutDirection = android.view.View.LAYOUT_DIRECTION_RTL
        val exactSize = android.view.View.MeasureSpec.makeMeasureSpec(400, android.view.View.MeasureSpec.EXACTLY)
        natives.measure(exactSize, exactSize)
        natives.layout(0, 0, 400, 400)
        val web = natives.getChildAt(0) as WebView
        val row = android.graphics.RenderNode("clip").apply {
            setPosition(0, 0, 160, 40); setClipToBounds(true)
            pivotX = 0f; pivotY = 0f; scaleX = 2f; scaleY = 2f
        }
        graph.record(row, listOf(PaintedOrigins.Placement(natives.node(1)!!, 10f, 5f)))
        graph.record(null, listOf(PaintedOrigins.Placement(row, 20f, 10f)))
        natives.placeFrames()
        val point = floatArrayOf(0f, 0f, 200f, 100f)
        web.matrix.mapPoints(point)
        assertEquals(0, web.left)
        assertEquals(0, web.top)
        assertArrayEquals(floatArrayOf(40f, 20f, 440f, 220f), point, 0.01f)
        val visible = android.graphics.Rect(0, 0, 200, 100)
        assertTrue(natives.getChildVisibleRect(web, visible, null))
        assertEquals(android.graphics.Rect(40, 20, 340, 90), visible)
        row.scaleX = 0f
        graph.moved(row)
        natives.placeFrames()
        assertEquals(0f, web.scaleX, 0f)
        assertEquals(2f, web.scaleY, 0f)
        assertEquals(20f, web.translationX, 0f)
        natives.unrowed(emptySet())
        assertSame(natives, web.parent)
        assertFalse(natives.getChildVisibleRect(web, android.graphics.Rect(0, 0, 200, 100), null))
    }

    @Test fun aSmallBoxInARotatedFrameCanFallOutsideThePaintClip() {
        val natives = natives()
        val graph = PaintedOrigins()
        natives.origins = graph
        natives.ensure(1, 1, 100, 100, props(null), 0)
        val frame = natives.node(1)!!
        val row = android.graphics.RenderNode("rotated").apply {
            setClipToBounds(false); pivotX = 0f; pivotY = 0f; rotationZ = 45f
        }
        graph.record(row, listOf(PaintedOrigins.Placement(frame, 0f, 0f)))
        val band = android.graphics.Rect(0, 100, 300, 120)
        graph.record(null, listOf(PaintedOrigins.Placement(row, 100f, 100f, band)))
        natives.placeFrames()
        val web = natives.getChildAt(0)
        assertFalse(natives.getChildVisibleRect(web, android.graphics.Rect(39, 39, 41, 41), null))
        assertTrue(natives.getChildVisibleRect(web, android.graphics.Rect(4, 4, 6, 6), null))
    }

    @Test fun aKeptRoutePlacesAFrameOnlyWhenItsLiveRowHasGone() {
        val graph = PaintedOrigins()
        val frame = android.graphics.RenderNode("frame").apply { setClipToBounds(false) }
        val kept = android.graphics.Matrix().apply { setScale(0.5f, 0.5f); postTranslate(100f, 100f) }
        val copy = PaintedOrigins.Placement(frame, 0f, 0f, kept = kept)
        val live = PaintedOrigins.Placement(frame, 20f, 10f)
        val matrix = android.graphics.Matrix()
        val bounds = android.graphics.RectF(0f, 0f, 80f, 60f)
        graph.record(null, listOf(copy, live))
        assertTrue(graph.transform(frame, matrix, bounds))
        assertEquals(android.graphics.RectF(20f, 10f, 100f, 70f), bounds)
        graph.record(null, listOf(copy))
        assertFalse(graph.transform(frame, matrix))
        bounds.set(0f, 0f, 80f, 60f)
        assertTrue(graph.transform(frame, matrix, bounds))
        assertEquals(android.graphics.RectF(100f, 100f, 140f, 130f), bounds)
    }

    private fun origin(web: WebView) = floatArrayOf(0f, 0f).also { web.matrix.mapPoints(it) }

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
