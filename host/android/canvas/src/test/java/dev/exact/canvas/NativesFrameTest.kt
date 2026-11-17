package dev.exact.canvas

import android.os.Looper
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebView
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowLog
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativesFrameTest {
    private fun natives() = Natives(RuntimeEnvironment.getApplication(), { File(".") }, emptyMap())

    private fun props(sandbox: String? = null): String = JSONObject().put("src", "https://example.com/")
        .put("sandbox", sandbox ?: JSONObject.NULL).toString()

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
        natives.ensure(1, 1, 0, 0, props(), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        val web = natives.getChildAt(0) as WebView
        natives.near(emptySet(), emptySet(), emptySet())
        assertEquals("about:blank", shadowOf(web).lastLoadedUrl)
        assertFalse(web.settings.javaScriptEnabled)
        assertFalse(web.settings.allowFileAccess)
        natives.ensure(1, 1, 0, 0, props("allow-scripts"), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        assertNull(natives.node(1))
        assertFalse(web.settings.javaScriptEnabled)
        natives.ensure(2, 1, 0, 0, props(), 20)
        natives.near(setOf(20), setOf(20), setOf(20))
        assertSame(web, natives.getChildAt(0))
        assertTrue(web.settings.javaScriptEnabled)
        assertEquals("https://example.com/", shadowOf(web).lastLoadedUrl)
        assertEquals(mapOf("Referer" to "https://exact.invalid/"), shadowOf(web).lastAdditionalHttpHeaders)
        natives.destroyAll()
    }

    private fun gone(web: WebView): Boolean = shadowOf(web).webViewClient.onRenderProcessGone(web, object : RenderProcessGoneDetail() {
        override fun didCrash() = true
        override fun rendererPriorityAtExit() = WebView.RENDERER_PRIORITY_IMPORTANT
    })

    /** A frame outlives its renderer, as on Apple (LLP 1020 D2): the dead view goes and a new one loads `src`. */
    @Test fun aFrameWhoseRendererEndsLoadsItsSourceInANewView() {
        val natives = natives()
        natives.ensure(1, 1, 100, 50, props(), 0)
        val web = natives.getChildAt(0) as WebView
        assertTrue(gone(web))
        assertTrue(shadowOf(web).wasDestroyCalled())
        assertEquals(1, natives.childCount)
        val fresh = natives.getChildAt(0) as WebView
        assertNotSame(web, fresh)
        assertEquals(100, fresh.layoutParams.width)
        assertNull(shadowOf(fresh).lastLoadedUrl)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("https://example.com/", shadowOf(fresh).lastLoadedUrl)
        assertEquals(mapOf("Referer" to "https://exact.invalid/"), shadowOf(fresh).lastAdditionalHttpHeaders)
        assertTrue(ShadowLog.getLogsForTag("exact").any { it.msg == "iframe 1: the page's renderer crashed; loading its src again" })
        // A new `src` before the reload is loaded once, and the old one not after it.
        assertTrue(gone(fresh))
        val third = natives.getChildAt(0) as WebView
        natives.ensure(1, 1, 100, 50, JSONObject().put("src", "https://example.org/").put("sandbox", JSONObject.NULL).toString(), 0)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("https://example.org/", shadowOf(third).lastLoadedUrl)
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
