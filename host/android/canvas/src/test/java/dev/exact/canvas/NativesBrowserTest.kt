package dev.exact.canvas

import android.os.Looper
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebView
import androidx.webkit.Navigation
import androidx.webkit.Page
import org.json.JSONArray
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
import java.io.File
import java.lang.reflect.InvocationHandler
import java.lang.reflect.Method
import java.time.Duration
import java.util.concurrent.Callable

/** A kept partition's navigation events and commands (LLP 1020.002 D2, D3), and their refusal where none is kept. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [
    NativesPartitionTest.Host::class, NativesPartitionTest.Compat::class, NativesPartitionTest.Features::class,
])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativesBrowserTest {
    private val sent get() = NativesPartitionTest.Host.sent

    @Before fun clear() {
        sent.clear(); NativesPartitionTest.Compat.profiles.clear()
        NativesPartitionTest.Features.profiles = true; NativesPartitionTest.Features.navigation = true
    }

    private fun natives() = Natives(RuntimeEnvironment.getApplication(), { File(".") }, emptyMap())

    private fun props(slot: Int, vararg commands: JSONArray, partition: String = "persist:a"): String = JSONObject()
        .put("src", "https://example.com/").put("partition", partition).put("sandbox", JSONObject.NULL)
        .put("slot", slot).put("generation", 1).put("session", 7)
        .apply { if (commands.isNotEmpty()) put("commands", JSONArray(commands.toList())) }.toString()

    private fun command(seq: Int, vararg args: Any) = JSONArray(listOf(seq, *args))

    private fun settle() = shadowOf(Looper.getMainLooper()).idle()

    private class Peer(private val properties: Map<String, Any?> = emptyMap()) : InvocationHandler {
        private var peer: Any? = null
        override fun invoke(proxy: Any, method: Method, args: Array<out Any?>?): Any? {
            if (method.name != "getOrCreatePeer") return properties[method.name]
            return peer ?: (args!!.single() as Callable<*>).call().also { peer = it }
        }
    }

    /** A main-frame navigation as WebView's listener reports it. */
    private fun navigation(url: String, page: Peer? = Peer(), sameDocument: Boolean = false, commit: Boolean = true,
                           errorPage: Boolean = false, history: Boolean = false) =
        Navigation.forInvocationHandler(Peer(mapOf(
            "getUrl" to url, "getPage" to page, "isSameDocument" to sameDocument, "wasInitiatedByPage" to false,
            "isHistory" to history, "isReload" to false, "didCommit" to commit, "didCommitErrorPage" to errorPage,
            "getWebResourceError" to null,
        )))

    private fun events() = sent.map { s -> s.record.let {
        listOf(s.event, it.getInt("navigationId"), it.getString("url"), it.getString("destination"), it.getBoolean("loading"),
            it.getString("error")).joinToString(" ").trim()
    } }

    private fun listener(): androidx.webkit.NavigationListener =
        NativesPartitionTest.Compat.listeners.getValue(NativesPartitionTest.Compat.last())

    @Test fun aLoadIsNavigateCommitAndLoadAndANewOneSupersedesItWithAnAbort() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(1), 0)
        val listener = listener()
        val page = Peer()
        val first = navigation("https://example.com/", page)
        listener.onNavigationStarted(first)
        listener.onNavigationRedirected(first)
        listener.onNavigationCompleted(first)
        listener.onPageLoadEvent(Page.forInvocationHandler(page))
        val second = navigation("https://example.com/b")
        listener.onNavigationStarted(second)
        listener.onNavigationStarted(navigation("https://example.com/c"))
        // The superseded navigation's own end is not reported again.
        listener.onNavigationCompleted(second)
        settle()
        assertEquals(listOf(
            "navigate 1  https://example.com/ true",
            "navigate 1  https://example.com/ true",
            "currententrychange 1 https://example.com/  true",
            "navigatesuccess 1 https://example.com/  false",
            "navigate 2 https://example.com/ https://example.com/b true",
            "navigateerror 2 https://example.com/  false AbortError",
            "navigate 3 https://example.com/ https://example.com/c true",
        ), events())
        assertEquals("https://example.com/favicon.ico", sent[3].record.getJSONArray("icons").getString(0))
        natives.destroyAll()
    }

    @Test fun aSameDocumentNavigationIsThreeEventsAndAFailedLoadKeepsTheCommittedUrl() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(1), 0)
        val listener = listener()
        val page = Peer()
        val loaded = navigation("https://example.com/", page)
        listener.onNavigationStarted(loaded)
        listener.onNavigationCompleted(loaded)
        // While a document is in flight a same-document navigation is not reported, as on Apple.
        listener.onNavigationCompleted(navigation("https://example.com/#early", sameDocument = true))
        listener.onPageLoadEvent(Page.forInvocationHandler(page))
        listener.onNavigationStarted(navigation("https://example.com/#x", sameDocument = true))
        listener.onNavigationCompleted(navigation("https://example.com/#x", sameDocument = true))
        val failed = navigation("https://nowhere.invalid/", errorPage = true)
        listener.onNavigationStarted(failed)
        listener.onNavigationCompleted(failed)
        settle()
        assertEquals(listOf(
            "navigate 1  https://example.com/ true",
            "currententrychange 1 https://example.com/  true",
            "navigatesuccess 1 https://example.com/  false",
            "navigate 2 https://example.com/ https://example.com/#x true",
            "currententrychange 2 https://example.com/#x  true",
            "navigatesuccess 2 https://example.com/#x  false",
            "navigate 3 https://example.com/#x https://nowhere.invalid/ true",
            "navigateerror 3 https://example.com/#x  false NetworkError",
        ), events())
        assertEquals("", sent.last().record.getString("title"))
        natives.destroyAll()
    }

    /** Each command runs once: the same props again, or a reader made again for the frame, runs none. */
    @Test fun commandsRunOnceAndRefuseWhatAndroidCannotDo() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(2), 0)
        val web = NativesPartitionTest.Compat.last()
        val commands = arrayOf(command(1, "navigate", "https://example.com/next"), command(2, "navigate", "javascript:alert(1)"),
            command(3, "reload", true), command(4, "traverseHistory", -3.0), command(5, "stopLoading"))
        natives.ensure(1, 1, 0, 0, props(2, *commands), 0)
        assertEquals("https://example.com/next", shadowOf(web).lastLoadedUrl)
        natives.ensure(1, 1, 0, 0, props(2, *commands, command(6, "stopLoading")), 0)
        natives().ensure(1, 1, 0, 0, props(2, *commands), 0)
        settle()
        assertEquals(listOf("NotSupportedError", "NotSupportedError", "InvalidStateError"),
            sent.map { it.record.getString("error") })
        assertTrue(sent.all { it.event == "navigateerror" })
        assertTrue(sent[0].record.getString("message"), sent[0].record.getString("message").startsWith("navigate: javascript:"))
        natives.destroyAll()
    }

    /** Where partitions are not kept (an older WebView), a command to a partitioned frame is refused with the reason. */
    @Test fun aCommandToARefusedPartitionIsRefused() {
        NativesPartitionTest.Features.navigation = false
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(3, command(1, "reload")), 0)
        natives.ensure(2, 1, 0, 0, props(4, command(1, "navigate", "https://example.com/"), partition = "mem"), 0)
        settle()
        assertTrue(NativesPartitionTest.Compat.profiles.isEmpty())
        assertEquals(listOf(1, 1, 2, 2), sent.map { it.id })
        assertTrue(sent.all { it.event == "navigateerror" && it.record.getString("error") == "NotSupportedError" })
        assertTrue(sent[1].record.getString("message").startsWith("reload: partition \"persist:a\": this device's WebView"))
        assertTrue(sent[3].record.getString("message").startsWith("navigate: partition \"mem\": Android keeps only persist:"))
        natives.destroyAll()
    }

    /** A renderer's end: `crashed` and a load after 250, 500 and 1,000 ms; the fourth end within 30 s stops. */
    @Test fun aRendererThatKeepsEndingIsRecoveredThreeTimes() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props(5), 0)
        val listener = listener()
        val loaded = navigation("https://example.com/")
        listener.onNavigationStarted(loaded)
        listener.onNavigationCompleted(loaded)
        for (delay in listOf(250L, 500L, 1_000L, null)) {
            sent.clear()
            val web = NativesPartitionTest.Compat.last()
            assertTrue(shadowOf(web).webViewClient.onRenderProcessGone(web, object : RenderProcessGoneDetail() {
                override fun didCrash() = true
                override fun rendererPriorityAtExit() = WebView.RENDERER_PRIORITY_IMPORTANT
            }))
            settle()
            val fresh = NativesPartitionTest.Compat.last()
            assertNotSame(web, fresh)
            val crash = sent.single { it.event == "navigateerror" }.record
            assertEquals("crashed", crash.getString("error"))
            assertEquals(delay != null, crash.getBoolean("recovering"))
            if (delay == null) {
                shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
                assertNull(shadowOf(fresh).lastLoadedUrl)
            } else {
                shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(delay - 1))
                assertNull(shadowOf(fresh).lastLoadedUrl)
                shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(1))
                assertEquals("https://example.com/", shadowOf(fresh).lastLoadedUrl)
            }
        }
        natives.destroyAll()
    }

    /** A navigation in flight ends in `AbortError` when its view goes: a new partition, or a lazy row parked. */
    @Test fun aNavigationInFlightEndsWhenItsViewGoes() {
        val natives = natives().apply { lazy = true }
        natives.ensure(1, 1, 0, 0, props(6), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        listener().onNavigationStarted(navigation("https://example.com/"))
        natives.ensure(1, 1, 0, 0, props(6, partition = "persist:b"), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        listener().onNavigationStarted(navigation("https://example.com/"))
        // Parked while its renderer's recovery waits: nothing loads into the stand-in, and the flight ends.
        val web = NativesPartitionTest.Compat.last()
        shadowOf(web).webViewClient.onRenderProcessGone(web, object : RenderProcessGoneDetail() {
            override fun didCrash() = true
            override fun rendererPriorityAtExit() = WebView.RENDERER_PRIORITY_IMPORTANT
        })
        natives.near(emptySet(), emptySet(), setOf(10))
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
        assertEquals(listOf("navigate", "navigateerror", "navigate", "navigateerror"), sent.map { it.event })
        assertEquals(listOf("AbortError", "crashed"), sent.filter { it.event == "navigateerror" }.map { it.record.getString("error") })
        // A command given while it had no view moved nothing: refused when it is shown again.
        natives.ensure(1, 1, 0, 0, props(6, command(1, "navigate", "https://example.com/b"), partition = "persist:b"), 10)
        natives.near(setOf(10), setOf(10), setOf(10))
        settle()
        assertEquals("InvalidStateError", sent.last().record.getString("error"))
        assertEquals("https://example.com/", shadowOf(NativesPartitionTest.Compat.last()).lastLoadedUrl)
        natives.destroyAll()
    }

    /** `clearSiteData` refuses, by the DOMException's name, what Android cannot clear, and answers when the WebView
     * provider fails rather than throwing. */
    @Test fun clearSiteDataRefusesWhatItCannotClear() {
        val answers = ArrayList<Pair<String, String>?>()
        fun clear(partition: String, vararg types: String) = FrameStorage.clearSiteData(partition, types.toList()) { answers += it }
        clear("a", "cookies")
        clear("persist:a", "cookies")
        NativesPartitionTest.Features.navigation = false
        clear("persist:a", "cookies")
        assertEquals(listOf("NotSupportedError", "NotSupportedError", "NotSupportedError"), answers.map { it?.first })
        assertTrue(answers[1]!!.second, answers[1]!!.second.contains("could not clear"))
    }
}
