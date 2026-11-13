package dev.exact.canvas

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
    private fun natives() = Natives(RuntimeEnvironment.getApplication(), File("."), emptyMap())

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
        natives.destroyAll()
    }
}
