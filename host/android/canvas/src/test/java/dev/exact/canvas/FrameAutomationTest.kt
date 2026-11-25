package dev.exact.canvas

import android.net.Uri
import android.os.Looper
import android.webkit.WebView
import androidx.webkit.JavaScriptExecutionException
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.Navigation
import androidx.webkit.Page
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewOutcomeReceiver
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
import java.util.concurrent.Callable

/**
 * `@exact/browser-automation` on a kept partitioned frame (LLP 1020.002 D9, §11 "Android as built"): scripts in the
 * page's world without awaiting, in a sandbox's world through its own channel, and refusals where Exact would need
 * something of its own in the page's world or the WebView has no worlds. The emulator drive measures the platform.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [
    NativesPartitionTest.Host::class, NativesPartitionTest.Compat::class, NativesPartitionTest.Features::class,
])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class FrameAutomationTest {
    private val compat = NativesPartitionTest.Compat

    @Before fun clear() {
        NativesPartitionTest.Host.sent.clear(); compat.profiles.clear(); compat.worldListeners.clear(); compat.worldScripts.clear()
        NativesPartitionTest.Features.profiles = true; NativesPartitionTest.Features.navigation = true
        NativesPartitionTest.Features.worlds = true
        FrameAutomations.readers.clear()
    }

    private fun natives() = Natives(RuntimeEnvironment.getApplication(), { File(".") }, emptyMap())

    private fun props(id: String, partition: String? = "persist:a") = JSONObject()
        .put("src", "https://example.com/").put("id", id).put("partition", partition ?: JSONObject.NULL)
        .put("sandbox", JSONObject.NULL).put("slot", 1).put("generation", 1).put("session", 7).toString()

    private fun settle() = shadowOf(Looper.getMainLooper()).idle()

    /** A call as the package makes it; its answer, once given. */
    private fun call(op: String, vararg pairs: Pair<String, Any?>): Array<JSONObject?> {
        val answer = arrayOfNulls<JSONObject>(1)
        val args = JSONObject().put("id", "tab")
        for ((k, v) in pairs) args.put(k, v ?: JSONObject.NULL)
        FrameAutomations.run(op, args) { answer[0] = it }
        return answer
    }

    private fun Array<JSONObject?>.error() = this[0]?.optString("error")?.ifEmpty { null }

    /** A reply proxy that keeps what it was asked to run. */
    private class Proxy : JavaScriptReplyProxy() {
        val token = java.util.UUID.randomUUID().toString().replace("-", "")
        val scripts = ArrayList<Pair<String, WebViewOutcomeReceiver<String, JavaScriptExecutionException>>>()
        var fullyActive = true
        var holdCheck = false
        val checks = mutableListOf<WebViewOutcomeReceiver<String, JavaScriptExecutionException>>()
        val acknowledgements = mutableListOf<String>()
        override fun postMessage(message: String) { acknowledgements += message }
        override fun postMessage(message: ByteArray) {}
        override fun executeJavaScript(script: String, receiver: WebViewOutcomeReceiver<String, JavaScriptExecutionException>?) {
            if (script.endsWith(".active()")) {
                checks += receiver!!
                if (!holdCheck) receiver.onResult(fullyActive.toString())
            } else scripts += script to receiver!!
        }
    }

    /** The world's script posting to its listener, as the main frame does. */
    private fun post(web: WebView, world: String, data: String, proxy: Proxy, source: Proxy = proxy, origin: String = "https://example.com:443") {
        val text = if (data == "ready") "ready:${proxy.token}" else {
            val message = JSONObject(data)
            val marker = proxy.scripts.lastOrNull()?.first?.let { Regex("""token (?:\?\?=|===) "([^"]+)""").find(it)?.groupValues?.get(1) }
            message.put("marker", marker).toString()
        }
        compat.worldListeners.getValue(web).getValue(world)
            .onPostMessage(web, WebMessageCompat(text), Uri.parse(origin), true, source)
    }

    /** The last script [proxy] ran answers its value, as the world's script posts it. */
    private fun answer(web: WebView, proxy: Proxy, world: String, value: Any) {
        val (script, receiver) = proxy.scripts.last()
        receiver.onResult("true")
        val n = Regex("n: (\\d+)").find(script)!!.groupValues[1].toInt()
        post(web, world, JSONObject().put("n", n).put("r", value).toString(), proxy)
    }

    private class Peer(private val properties: Map<String, Any?> = emptyMap()) : InvocationHandler {
        private var peer: Any? = null
        override fun invoke(proxy: Any, method: Method, args: Array<out Any?>?): Any? {
            if (method.name != "getOrCreatePeer") return properties[method.name]
            return peer ?: (args!!.single() as Callable<*>).call().also { peer = it }
        }
    }

    private fun commit(web: WebView, url: String, beforeCommit: () -> Unit = {}) {
        val navigation = Navigation.forInvocationHandler(Peer(mapOf(
            "getUrl" to url, "getPage" to Peer(), "isSameDocument" to false, "wasInitiatedByPage" to true,
            "isHistory" to false, "isReload" to false, "didCommit" to true, "didCommitErrorPage" to false,
        )))
        val listener = compat.listeners.getValue(web)
        listener.onNavigationStarted(navigation)
        beforeCommit()
        listener.onNavigationCompleted(navigation)
    }

    @Test fun thePagesWorldAnswersWhatItCanWithoutAnythingOfExactsThere() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("tab"), 0)
        val web = compat.last()
        val title = call("evaluate", "expression" to "document.title")
        val shadow = shadowOf(web)
        shadow.lastEvaluatedJavascriptCallback.onReceiveValue(JSONObject.quote("token\nhttps://example.com"))
        assertTrue(shadow.lastEvaluatedJavascript.contains("document.title"))
        shadow.lastEvaluatedJavascriptCallback.onReceiveValue(JSONObject.quote("\"Auto\""))
        assertEquals("Auto", title[0]!!.getString("result"))
        assertEquals("https://example.com", title[0]!!.getString("origin"))
        val realm = title[0]!!.getString("realm")
        val targeted = call("evaluate", "id" to null, "target" to JSONObject().put("realm", realm), "expression" to "3")
        shadow.lastEvaluatedJavascriptCallback.onReceiveValue("false")
        assertEquals("no such frame", targeted.error())
        val promise = call("evaluate", "expression" to "Promise.resolve(7)")
        shadow.lastEvaluatedJavascriptCallback.onReceiveValue("""{"__exactPromise":true}""")
        assertEquals("NotSupportedError", promise.error())
        assertTrue(promise[0]!!.getString("message").contains("sandbox"))
        val thrown = call("callFunction", "functionDeclaration" to "() => nope()", "arguments" to JSONArray())
        shadow.lastEvaluatedJavascriptCallback.onReceiveValue("""{"__exactThrown":true,"name":"ReferenceError","message":"nope is not defined"}""")
        assertEquals("ReferenceError", thrown.error())
        val unparsed = call("evaluate", "expression" to "1 +")
        shadow.lastEvaluatedJavascriptCallback.onReceiveValue("null")
        assertEquals("SyntaxError", unparsed.error())
        assertEquals("NotSupportedError", call("addPreloadScript", "functionDeclaration" to "() => {}").error())
        assertEquals("NotSupportedError", call("performActions", "actions" to JSONArray()).error())
        assertEquals("SyntaxError", call("evaluate", "expression" to "1", "sandbox" to "exact.browser").error())
        assertTrue("nothing of Exact's in the page's world", compat.pageWorld.isEmpty())
    }

    @Test fun aSandboxAnswersThroughItsWorldFromItsNextDocumentAndEndsWithIt() {
        // A name no earlier test gave: the app's names are kept, and a known one's world is on each new view.
        val sb = "t${System.nanoTime()}"
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("tab"), 0)
        val web = compat.last()
        assertEquals("InvalidStateError", call("evaluate", "expression" to "1", "sandbox" to sb).error())
        val added = call("addPreloadScript", "functionDeclaration" to "() => { window.isolated = 1 }", "sandbox" to sb)
        assertTrue(added[0]!!.getString("script").startsWith("script-"))
        assertEquals("the channel's post, then the preload", 2, compat.worldScripts.getValue(web).count { it.first == sb })
        val proxy = Proxy()
        post(web, sb, "ready", proxy)
        val seven = call("evaluate", "expression" to "new Promise(r => r(7))", "sandbox" to sb)
        answer(web, proxy, sb, "7")
        assertEquals(7, seven[0]!!.getInt("result"))
        assertEquals("https://example.com", seven[0]!!.getString("origin"))
        val firstRealm = seven[0]!!.getString("realm")
        val late = call("evaluate", "expression" to "Promise.reject(new TypeError('late'))", "sandbox" to sb)
        answer(web, proxy, sb, JSONObject().put("__exactThrown", true).put("name", "TypeError").put("message", "late"))
        assertEquals("TypeError", late.error())
        val unparsed = call("evaluate", "expression" to "1 +", "sandbox" to sb)
        proxy.scripts.last().second.onResult(null)
        assertEquals("SyntaxError", unparsed.error())

        val pending = call("evaluate", "expression" to "new Promise(() => {})", "sandbox" to sb)
        commit(web, "https://example.com/next")
        assertEquals("AbortError", pending.error())
        // The world was on the view before this document began: a call waits for its post, not the old proxy.
        val waits = call("evaluate", "expression" to "1", "sandbox" to sb)
        assertNull(waits[0])
        assertEquals("no such frame", call("evaluate", "id" to null, "target" to JSONObject().put("realm", firstRealm), "expression" to "1").error())
        val second = Proxy()
        post(web, sb, "ready", second)
        answer(web, second, sb, "1")
        assertEquals(1, waits[0]!!.getInt("result"))
        assertEquals("the old document's proxy ran nothing more", 4, proxy.scripts.size)
        // A reply posted in another world is not this call's.
        call("addPreloadScript", "functionDeclaration" to "() => {}", "sandbox" to sb + "x")
        val other = call("evaluate", "expression" to "2", "sandbox" to sb)
        val n = Regex("n: (\\d+)").find(second.scripts.last().first)!!.groupValues[1].toInt()
        post(web, sb + "x", JSONObject().put("n", n).put("r", "3").toString(), second)
        assertNull(other[0])
        // The new document's post can reach the listener before WebView reports its commit.
        val early = Proxy()
        commit(web, "https://example.com/third") { post(web, sb, "ready", early) }
        assertEquals("AbortError", other.error())
        call("evaluate", "expression" to "1", "sandbox" to sb)
        assertEquals(1, early.scripts.size)

        // A call that acts takes the frame; its record says so.
        val acts = call("evaluate", "expression" to "1", "userActivation" to true)
        shadowOf(web).lastEvaluatedJavascriptCallback.onReceiveValue(JSONObject.quote("page\nhttps://example.com"))
        shadowOf(web).lastEvaluatedJavascriptCallback.onReceiveValue(JSONObject.quote("1"))
        assertNull(acts.error())
        settle()
        assertEquals("agent", NativesPartitionTest.Host.sent.last { it.event == "currententrychange" }.record.getString("controller"))

        // A changed partition is a new frame: a call in flight ends, and the preload script goes with it.
        val original = call("evaluate", "expression" to "1", "sandbox" to sb)
        val originalNumber = Regex("n: (\\d+)").find(early.scripts.last().first)!!.groupValues[1].toInt()
        val replacement = Proxy()
        post(web, sb, "ready", replacement)
        val current = call("evaluate", "expression" to "2", "sandbox" to sb)
        post(web, sb, JSONObject().put("n", originalNumber).put("r", "2").toString(), early, source = replacement)
        assertNull("only the original AndroidX source can settle the call", original[0])
        answer(web, replacement, sb, "2")
        post(web, sb, JSONObject().put("n", originalNumber).put("r", "1").toString(), early)
        assertEquals(1, original[0]!!.getInt("result"))
        assertNotEquals(current[0]!!.getString("realm"), original[0]!!.getString("realm"))
        val held = call("evaluate", "expression" to "new Promise(() => {})", "sandbox" to sb)
        assertNull(held[0])
        natives.ensure(1, 1, 0, 0, props("tab", "persist:b"), 0)
        assertEquals("NotFoundError", held.error())
        val fresh = compat.last()
        assertNotSame(web, fresh)
        assertTrue(compat.worldScripts.getValue(fresh).any { it.first == sb && it.second.contains("window.isolated") })
        val kept = added[0]!!.getString("script")
        assertNull("the same id after the partition change", call("removePreloadScript", "script" to kept).error())
        assertFalse(compat.worldScripts.getValue(fresh).any { it.second.contains("window.isolated") })
        assertEquals("NotFoundError", call("removePreloadScript", "script" to kept).error())
    }

    @Test fun repeatedReadyKeepsTheSandboxAndOnlyAnAdmittedActTakesControl() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("tab"), 0)
        val web = compat.last()
        val world = "approval"
        call("addPreloadScript", "sandbox" to world, "functionDeclaration" to "() => {}")
        commit(web, "https://example.com/")
        val proxy = Proxy()
        fun read(): String {
            post(web, world, "ready", proxy)
            val read = call("evaluate", "expression" to "42", "sandbox" to world)
            answer(web, proxy, world, "42")
            return read[0]!!.getString("realm")
        }
        val realm = read()
        val afterReady = read()
        val arm = natives.automations("tab").single()!!
        val target = JSONObject().put("realm", afterReady)
        val route = call("evaluate", "id" to null, "target" to target, "expression" to "42")
        proxy.scripts.last().second.onResult("true")
        answer(web, proxy, world, "42")
        assertEquals(afterReady, route[0]!!.getString("realm"))
        // A delayed ready from another document must not displace this confirmed realm.
        val other = Proxy()
        post(web, world, "ready", other)
        val act = call("evaluate", "id" to null, "target" to target, "sandbox" to null,
            "userActivation" to true, "expression" to "42")
        val beforeAdmission = arm.controller
        proxy.scripts.last().second.onResult("false")
        assertEquals("no such frame", act.error())
        assertEquals("person", arm.controller)
        assertEquals(listOf("person", realm), listOf(beforeAdmission, afterReady))
        val entered = call("evaluate", "id" to null, "target" to target, "userActivation" to true, "expression" to "42")
        proxy.scripts.last().second.onResult("true")
        assertEquals("agent", arm.controller)
        commit(web, "https://example.com/next")
        assertEquals("AbortError", entered.error())
    }

    @Test fun aPostKeepsItsProxyAndOriginAfterANewDocumentCommits() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("tab"), 0)
        val web = compat.last()
        val args = JSONObject().put("id", "tab").put("channelSession", "first").put("functionDeclaration", "post => post('pause')")
            .put("arguments", JSONArray("""[{"type":"channel","value":{"channel":"playback"}}]"""))
        val messages = mutableListOf<JSONObject>()
        val other = mutableListOf<JSONObject>()
        val first: (JSONObject) -> Unit = { messages.add(it) }
        val second: (JSONObject) -> Unit = { other.add(it) }
        fun register(session: String, receiver: (JSONObject) -> Unit): JSONObject {
            var answer: JSONObject? = null
            FrameAutomations.run("addPreloadScript", args.put("channelSession", session), receiver) { answer = it }
            return answer!!
        }
        fun capability() = Regex("[0-9a-f-]{36}").find(compat.worldScripts.getValue(web).last().second)!!.value
        fun emit(key: String, json: String, source: Proxy) =
            post(web, "video", JSONObject().put("capability", key).put("json", json).toString(), source)
        assertEquals("NotSupportedError", register("first", first).getString("error"))
        args.put("sandbox", "video")
        val script = register("first", first).getString("script")
        val key = capability()
        assertTrue(register("second", second).has("script"))
        val otherKey = capability()
        assertNotEquals(key, otherKey)
        val proxy = Proxy()
        commit(web, "https://example.com/first")
        emit(key, "1", proxy)
        val early = messages.removeAt(0).getJSONObject("source")
        val live = call("evaluate", "id" to null, "target" to JSONObject().put("realm", early.getString("realm")), "expression" to "2")
        assertTrue(proxy.scripts.isEmpty())
        post(web, "video", "ready", proxy)
        proxy.scripts.last().second.onResult("true")
        answer(web, proxy, "video", "2")
        assertEquals(2, live[0]!!.getInt("result"))
        val realm = live[0]!!.getString("realm")
        assertEquals(realm, early.getString("realm"))
        // The call is still awaiting its guest promise when it navigates. AndroidX delivers its
        // last post after the commit and before the execution callback for the call itself.
        val once = mutableListOf<JSONObject>()
        var aborted: JSONObject? = null
        val pendingArgs = JSONObject(args.toString()).put("channelSession", "calling")
            .put("functionDeclaration", "post => { post('pause'); return new Promise(() => {}) }")
        FrameAutomations.run("callFunction", pendingArgs, { once.add(it) }) { aborted = it }
        val body = proxy.scripts.last().first
        val callKey = Regex("[0-9a-f-]{36}").find(body)!!.value
        val number = Regex("n: (\\d+)").find(body)!!.groupValues[1].toInt()
        post(web, "video", JSONObject().put("n", number).put("bind", true).toString(), proxy)
        commit(web, "https://example.com/next")
        assertEquals("AbortError", aborted!!.getString("error"))
        emit(callKey, "\"pause\"", proxy)
        assertEquals(realm, once.single().getJSONObject("source").getString("realm"))

        val outgoingRequest = Proxy()
        emit(key, "\"outgoing\"", outgoingRequest)
        val outgoingRealm = messages.removeAt(0).getJSONObject("source").getString("realm")
        val waitingForOutgoing = call("evaluate", "id" to null, "target" to JSONObject().put("realm", outgoingRealm), "expression" to "41")
        commit(web, "https://example.com/replacement")
        assertEquals("no such frame", waitingForOutgoing.error())
        val automation = natives.automations("tab").single()!!
        automation.starting()
        emit(key, "\"request\"", Proxy())
        val superseded = messages.removeAt(0).getJSONObject("source").getString("realm")
        val late = call("evaluate", "id" to null, "target" to JSONObject().put("realm", superseded), "expression" to "42")
        automation.starting()
        assertEquals("no such frame", late.error())
        automation.committed(null)
        // A wallet posts at document start. Its reply can reach the host before the navigation commit.
        for (readyBeforeCommit in listOf(true, false)) {
            val incoming = Proxy()
            lateinit var reply: Array<JSONObject?>
            commit(web, "https://example.com/wallet") {
                if (readyBeforeCommit) post(web, "video", "ready", incoming)
                emit(key, "\"request\"", incoming)
                val source = messages.removeAt(0).getJSONObject("source")
                reply = call("evaluate", "id" to null, "target" to JSONObject().put("realm", source.getString("realm")), "expression" to "42")
                assertNull("the incoming document can receive a reply", reply.error())
            }
            if (!readyBeforeCommit) post(web, "video", "ready", incoming)
            assertNull(reply.error())
            incoming.scripts.last().second.onResult("true")
            answer(web, incoming, "video", "42")
            assertEquals(42, reply[0]!!.getInt("result"))
        }
        commit(web, "https://elsewhere.example/next")
        val outgoing = Proxy()
        emit(key, "0", outgoing)
        messages.removeAt(0)
        val read = call("evaluate", "sandbox" to "video", "expression" to "3")
        assertTrue(outgoing.scripts.isEmpty())
        val current = Proxy()
        post(web, "video", "ready", current)
        answer(web, current, "video", "3")
        assertEquals(3, read[0]!!.getInt("result"))
        emit(key, "\"pause\"", proxy)
        assertTrue(other.isEmpty())
        val message = messages.single()
        assertEquals("playback", message.getString("channel"))
        assertEquals("\"pause\"", message.getString("json"))
        assertEquals(realm, message.getJSONObject("source").getString("realm"))
        assertEquals("https://example.com", message.getJSONObject("source").getString("origin"))
        assertEquals("tab", message.getJSONObject("source").getString("context"))
        val installed = compat.worldScripts.getValue(web).size
        FrameAutomations.release("first")
        assertEquals(installed - 1, compat.worldScripts.getValue(web).size)
        assertEquals("NotFoundError", call("removePreloadScript", "script" to script).error())
        emit(key, "4", current)
        emit(otherKey, "5", current)
        assertEquals(1, messages.size)
        assertEquals("5", other.single().getString("json"))
    }

    private inner class Channel(row: Int = 0, session: String = "") {
        val natives = natives()
        val world = "ordering"
        val web: WebView
        val frame: FrameAutomation
        private val key: String
        val messages = mutableListOf<JSONObject>()

        init {
            natives.ensure(1, 1, 0, 0, props("tab"), row)
            web = compat.last()
            frame = natives.automations("tab").single()!!
            val args = JSONObject().put("id", "tab").put("sandbox", world).put("channelSession", session)
                .put("arguments", JSONArray("""[{"type":"channel","value":{"channel":"status"}}]"""))
                .put("functionDeclaration", "post => post(1)")
            FrameAutomations.run("addPreloadScript", args, { messages.add(it) }) { assertTrue(it.has("script")) }
            key = Regex("[0-9a-f-]{36}").find(compat.worldScripts.getValue(web).last().second)!!.value
            commit(web, "https://example.com/first")
        }

        fun send(doc: Proxy, json: String) = post(web, world, JSONObject().put("capability", key).put("json", json).toString(), doc)

        fun postSource(doc: Proxy): JSONObject? {
            send(doc, "1")
            return if (messages.isEmpty()) null else messages.single().getJSONObject("source").also { messages.clear() }
        }

        fun source(doc: Proxy): JSONObject = checkNotNull(postSource(doc))

        fun read(source: JSONObject) = call("evaluate", "id" to null,
            "target" to JSONObject().put("realm", source.getString("realm")), "expression" to "42")

        fun ready(doc: Proxy) = post(web, world, "ready", doc)

        fun checkCurrent(doc: Proxy) {
            val read = call("evaluate", "sandbox" to world, "expression" to "42")
            answer(web, doc, world, "42")
            assertEquals(source(doc).getString("realm"), read[0]?.getString("realm"))
        }
    }

    private fun outgoingReply(readyDuringNavigation: Boolean) {
        val channel = Channel()
        val old = Proxy()
        val current = Proxy()
        val source = channel.source(old)
        lateinit var reply: Array<JSONObject?>
        commit(channel.web, "https://example.com/next") {
            if (readyDuringNavigation) channel.ready(old)
            reply = channel.read(source)
            assertTrue(old.scripts.isEmpty())
            old.fullyActive = false
        }
        if (!readyDuringNavigation) channel.ready(old)
        assertEquals("no such frame", reply.error())
        assertTrue(old.scripts.isEmpty())
        assertTrue(current.scripts.isEmpty())
        assertEquals("no such frame", channel.read(source).error())
        val frameRead = call("evaluate", "sandbox" to channel.world, "expression" to "42")
        assertTrue(old.scripts.isEmpty())
        channel.ready(current)
        answer(channel.web, current, channel.world, "42")
        assertEquals(channel.source(current).getString("realm"), frameRead[0]?.getString("realm"))
        channel.natives.destroyAll()
    }

    @Test fun aPostBeforeReadyDoesNotCarryItsReplyIntoTheNextPage() = outgoingReply(false)

    @Test fun anOutgoingReadyDuringNavigationCannotClaimTheCommit() = outgoingReply(true)

    private fun cancelledReply(stillCurrent: Boolean, waiting: Boolean = false) {
        val channel = Channel()
        val doc = Proxy()
        channel.frame.starting()
        val source = channel.source(doc)
        // In both cases the first callback from this proxy is the post during navigation.
        val pending = if (waiting) channel.read(source) else null
        doc.fullyActive = stillCurrent
        channel.frame.cancelled()
        val reply = pending ?: channel.read(source)
        channel.ready(doc)
        if (stillCurrent) {
            doc.scripts.last().second.onResult("true")
            answer(channel.web, doc, channel.world, "42")
            assertEquals(source.getString("realm"), reply[0]?.getString("realm"))
        } else {
            assertEquals("no such frame", reply.error())
            assertTrue(doc.scripts.isEmpty())
            assertEquals("no such frame", channel.read(source).error())
        }
        channel.natives.destroyAll()
    }

    @Test fun cancellingLeavesTheCurrentPostingDocumentLive() = cancelledReply(true)

    @Test fun cancellingRetiresTheProvisionalPostingDocument() = cancelledReply(false)

    @Test fun cancellationSettlesAWaitingReplyAccordingToWhichDocumentPosted() {
        cancelledReply(stillCurrent = false, waiting = true)
        cancelledReply(stillCurrent = true, waiting = true)
    }

    @Test fun aRepeatedReadyCannotReplaceTheCommittedWorld() {
        val channel = Channel()
        val old = Proxy()
        channel.ready(old)
        channel.checkCurrent(old)
        commit(channel.web, "https://example.com/next") { old.fullyActive = false }
        val current = Proxy()
        channel.ready(current)
        val checks = old.checks.size
        channel.ready(old)
        assertEquals(checks, old.checks.size)
        channel.checkCurrent(current)
        assertEquals(1, old.scripts.size)
        channel.natives.destroyAll()
    }

    @Test fun aWorldCheckAnsweredAfterCommitCannotPublishItsOldResult() {
        val channel = Channel()
        val old = Proxy().apply { holdCheck = true }
        channel.ready(old)
        commit(channel.web, "https://example.com/next")
        val read = call("evaluate", "sandbox" to channel.world, "expression" to "42")
        old.checks.single().onResult("true")
        assertTrue(old.scripts.isEmpty())
        val current = Proxy()
        channel.ready(current)
        answer(channel.web, current, channel.world, "42")
        assertEquals(channel.source(current).getString("realm"), read[0]?.getString("realm"))
        channel.natives.destroyAll()
    }

    @Test fun aCheckStartedBeforeNavigationCanStillAdmitTheCurrentDocument() {
        val channel = Channel()
        val old = Proxy().apply { holdCheck = true }
        channel.ready(old)
        channel.frame.starting()
        val source = channel.source(old)
        val reply = channel.read(source)
        old.checks.single().onResult("true")
        shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofSeconds(11))
        old.scripts.last().second.onResult("true")
        answer(channel.web, old, channel.world, "42")
        assertEquals(source.getString("realm"), reply[0]?.getString("realm"))
        channel.natives.destroyAll()
    }

    @Test fun historyRestoresAWorldWhoseFirstCheckCrossedACommit() {
        val channel = Channel()
        val a = Page.forInvocationHandler(Peer())
        channel.frame.starting()
        channel.frame.committed(a)
        val old = Proxy().apply { holdCheck = true }
        val source = channel.source(old)
        channel.ready(old)
        channel.frame.starting()
        old.holdCheck = false
        old.fullyActive = false
        channel.frame.committed(Page.forInvocationHandler(Peer()))
        old.checks.first().onResult("true")
        assertEquals("no such frame", channel.read(source).error())
        channel.frame.starting()
        old.fullyActive = true
        channel.frame.committed(a)
        val reply = channel.read(source)
        old.scripts.last().second.onResult("true")
        answer(channel.web, old, channel.world, "42")
        assertEquals(source.getString("realm"), reply[0]?.getString("realm"))
        channel.natives.destroyAll()
    }

    @Test fun aProviderErrorCheckingALiveDocumentAllowsAnotherAttempt() {
        val channel = Channel()
        val doc = Proxy().apply { holdCheck = true }
        val source = channel.source(doc)
        channel.ready(doc)
        val first = channel.read(source)
        doc.checks.single().onError(JavaScriptExecutionException(JavaScriptExecutionException.ERROR_GENERIC, "unavailable"))
        assertEquals("Error", first.error())
        doc.holdCheck = false
        val next = channel.read(source)
        doc.scripts.last().second.onResult("true")
        answer(channel.web, doc, channel.world, "42")
        assertEquals(source.getString("realm"), next[0]?.getString("realm"))
        channel.natives.destroyAll()
    }

    @Test fun anUnpublishedWorldCanAnnounceTheSameTokenWhenActive() {
        val channel = Channel()
        val doc = Proxy().apply { fullyActive = false }
        val source = channel.source(doc)
        channel.ready(doc)
        assertEquals("no such frame", channel.read(source).error())
        doc.fullyActive = true
        channel.ready(doc)
        channel.checkCurrent(doc)
        channel.natives.destroyAll()
    }

    @Test fun cancellationOfADestroyedProxyNeedsNoReadyToEndItsWaitingReply() {
        val channel = Channel()
        val doc = Proxy().apply { holdCheck = true }
        channel.frame.starting()
        val reply = channel.read(channel.source(doc))
        channel.frame.cancelled()
        doc.checks.single().onError(JavaScriptExecutionException(JavaScriptExecutionException.ERROR_FRAME_DESTROYED, "gone"))
        assertEquals("no such frame", reply.error())
        assertTrue(doc.scripts.isEmpty())
        channel.natives.destroyAll()
    }

    @Test fun aFirstReadyFromAnOlderProxyCannotDivertAFrameRead() {
        val channel = Channel()
        val old = Proxy().apply { fullyActive = false }
        val current = Proxy()
        commit(channel.web, "https://example.com/current")
        channel.ready(current)
        channel.checkCurrent(current)
        channel.ready(old)
        channel.checkCurrent(current)
        assertTrue(old.scripts.isEmpty())
        channel.natives.destroyAll()
    }

    @Test fun aSupersededWorldCannotTakeTheNextWorldsWaitingReply() {
        val channel = Channel()
        val lost = Proxy()
        channel.frame.starting()
        channel.ready(lost)
        channel.frame.starting()
        lost.fullyActive = false
        val incoming = Proxy()
        val reply = channel.read(channel.source(incoming))
        channel.ready(incoming)
        channel.frame.committed(null)
        incoming.scripts.last().second.onResult("true")
        answer(channel.web, incoming, channel.world, "42")
        assertEquals(42, reply[0]?.getInt("result"))
        assertTrue(lost.scripts.isEmpty())
        channel.checkCurrent(incoming)
        channel.natives.destroyAll()
    }

    @Test fun aSameDocumentNavigationWithNoPageKeepsTheWorld() {
        val channel = Channel()
        val doc = Proxy()
        channel.frame.committed(null)
        channel.ready(doc)
        channel.checkCurrent(doc)
        val nav = Navigation.forInvocationHandler(Peer(mapOf(
            "getUrl" to "https://example.com/first#here", "getPage" to null, "isSameDocument" to true,
            "isHistory" to false, "isReload" to false, "didCommit" to true,
        )))
        compat.listeners.getValue(channel.web).onNavigationStarted(nav)
        compat.listeners.getValue(channel.web).onNavigationCompleted(nav)
        channel.checkCurrent(doc)
        channel.natives.destroyAll()
    }

    @Test fun aPostDeliveredFromADroppedViewNamesItsPostingDocument() {
        val channel = Channel()
        val old = Proxy()
        val source = channel.source(old)
        channel.ready(old)
        channel.natives.ensure(1, 1, 0, 0, props("tab", "persist:b"), 0)
        assertNotSame(channel.web, compat.last())
        channel.postSource(old)?.let { after ->
            for (key in listOf("realm", "context", "origin")) assertEquals(source.getString(key), after.getString(key))
        }
        assertEquals("no such frame", channel.read(source).error())
        channel.natives.destroyAll()
    }

    @Test fun aKeptViewCanPostUntilTheFrameReleasesIt() {
        val journal = NativesPartitionTest.Host.journal
        journal.clear()
        try {
            for ((release, session) in listOf("partition" to "player", "park" to "player", "unmount" to "wallet")) {
                val channel = Channel(row = 10, session = session)
                channel.natives.near(setOf(10), setOf(10), setOf(10))
                val doc = Proxy()
                val source = channel.source(doc)
                channel.ready(doc)
                for (shown in listOf(emptySet(), setOf(10))) {
                    channel.natives.near(shown, setOf(10), setOf(10))
                    assertSame(channel.web, compat.last())
                    assertEquals(source.toString(), channel.source(doc).toString())
                }
                channel.send(doc, "2")
                when (release) {
                    "partition" -> channel.natives.ensure(1, 1, 0, 0, props("tab", "persist:b"), 10)
                    "park" -> {
                        var replied = false
                        val args = JSONObject().put("id", "tab").put("sandbox", channel.world).put("expression", "1")
                        FrameAutomations.run("evaluate", args) {
                            // Ending the call can run another frame command before park replaces the view.
                            call("removePreloadScript", "script" to "missing")
                            replied = true
                        }
                        channel.natives.near(emptySet(), emptySet(), setOf(10))
                        assertTrue(replied)
                    }
                    else -> channel.natives.destroyAll()
                }
                repeat(2) { channel.send(doc, "3") }
                val queued = channel.messages.single()
                assertEquals("2", queued.getString("json"))
                assertEquals(source.toString(), queued.getJSONObject("source").toString())
                channel.messages.clear()
                assertEquals("no such frame", channel.read(source).error())
                if (release == "park") {
                    channel.natives.near(setOf(10), setOf(10), setOf(10))
                    assertNotSame(channel.web, compat.last())
                    assertNull(channel.postSource(doc))
                }
                channel.natives.destroyAll()
            }
            assertEquals(List(2) { "script.message: dropped a post from a retired web view" }, journal)
        } finally {
            FrameAutomations.release("player")
            FrameAutomations.release("wallet")
            journal.clear()
        }
    }

    @Test fun closingAPackageSessionRemovesItsPreloadWithoutArguments() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("tab"), 0)
        val web = compat.last()
        fun preload(session: String) = call("addPreloadScript", "sandbox" to "plain",
            "channelSession" to session, "functionDeclaration" to "() => {}")[0]!!.getString("script")
        val other = preload("kept")
        val before = compat.worldScripts.getValue(web).size
        val closing = preload("closing")
        FrameAutomations.release("closing")
        assertEquals(before, compat.worldScripts.getValue(web).size)
        assertEquals("NotFoundError", call("removePreloadScript", "script" to closing).error())
        assertNull(call("removePreloadScript", "script" to other).error())
        val unnamed = call("addPreloadScript", "sandbox" to "plain", "functionDeclaration" to "() => {}")[0]!!.getString("script")
        FrameAutomations.release("")
        assertEquals("NotFoundError", call("removePreloadScript", "script" to unnamed).error())
        natives.destroyAll()
    }

    @Test fun aRefusedWorldScriptLeavesNoListenerAndThePreloadCanBeRetried() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("tab"), 0)
        val web = compat.last()
        fun preload() = call("addPreloadScript", "sandbox" to "retry", "functionDeclaration" to "() => {}")
        val before = compat.worldListeners[web].orEmpty().keys.toSet()
        compat.refuseNextScript = true
        assertEquals("NotSupportedError", preload().error())
        assertEquals(before, compat.worldListeners[web].orEmpty().keys)
        assertNotNull(preload()[0]?.getString("script"))
        val installed = compat.worldListeners.getValue(web).getValue("retry")
        compat.refuseNextScript = true
        assertEquals("NotSupportedError", preload().error())
        assertTrue(installed === compat.worldListeners.getValue(web)["retry"])
        natives.destroyAll()
    }

    @Test fun closingASessionSettlesACallWaitingForItsChannelArguments() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("tab"), 0)
        val web = compat.last()
        val world = "waiting"
        call("addPreloadScript", "sandbox" to world, "functionDeclaration" to "() => {}")
        commit(web, "https://example.com/doc")
        val proxy = Proxy()
        post(web, world, "ready", proxy)
        val current = call("evaluate", "sandbox" to world, "expression" to "1")
        answer(web, proxy, world, "1")
        val realm = current[0]!!.getString("realm")
        val before = proxy.scripts.size
        val args = JSONObject().put("id", "tab").put("sandbox", world).put("channelSession", "closing")
            .put("functionDeclaration", "post => {}")
            .put("arguments", JSONArray("""[{"type":"channel","value":{"channel":"status"}}]"""))
        val replies = mutableListOf<JSONObject>()
        FrameAutomations.run("callFunction", args, {}) { replies.add(it) }
        val target = JSONObject(args.toString()).apply {
            remove("id")
            remove("sandbox")
            put("target", JSONObject().put("realm", realm))
            put("userActivation", true)
        }
        FrameAutomations.run("callFunction", JSONObject(target.toString()), {}) { replies.add(it) }
        assertTrue(replies.isEmpty())
        assertEquals(before + 2, proxy.scripts.size)
        FrameAutomations.run("callFunction", target, {}) { replies.add(it) }
        proxy.scripts.last().second.onResult("true")
        assertEquals(before + 4, proxy.scripts.size)
        val number = Regex("""n: (\d+)""").find(proxy.scripts.last().first)!!.groupValues[1].toInt()
        FrameAutomations.release("closing")
        assertEquals(listOf("AbortError", "AbortError", "AbortError"), replies.map { it.optString("error") })
        post(web, world, JSONObject().put("n", number).put("bind", true).toString(), proxy)
        assertEquals(listOf("closed:$number"), proxy.acknowledgements)
        natives.destroyAll()
    }

    @Test fun rejectedPreloadsLeaveNoChannelRegistrations() {
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("tab"), 0)
        val frame = natives.automations("tab").single()!!
        val recipients = FrameAutomation::class.java.getDeclaredField("recipients").also { it.isAccessible = true }.get(frame) as Map<*, *>
        val before = recipients.size
        for (i in 1..3) {
            val arguments = JSONArray().put(JSONObject().put("type", "channel").put("value", JSONObject().put("channel", "req-$i"))).put(1)
            val args = JSONObject().put("id", "tab").put("sandbox", "video").put("channelSession", "observe")
                .put("arguments", arguments).put("functionDeclaration", "post => {}")
            FrameAutomations.run("addPreloadScript", args, {}) { assertEquals("invalid argument", it.getString("error")) }
        }
        assertEquals("native registrations after three rejected preloads", before, recipients.size)
        val call = JSONObject().put("id", "tab").put("sandbox", "not-yet-loaded").put("channelSession", "refused")
            .put("arguments", JSONArray("""[{"type":"channel","value":{"channel":"wallet"}}]"""))
            .put("functionDeclaration", "post => {}")
        FrameAutomations.run("callFunction", call, {}) { assertEquals("InvalidStateError", it.getString("error")) }
        assertEquals(before, recipients.size)
    }

    @Test fun aProviderWithoutDocumentStartStillEvaluatesInThePage() {
        NativesPartitionTest.Features.documentStart = false
        try {
            val natives = natives()
            natives.ensure(1, 1, 0, 0, props("tab"), 0)
            val read = call("evaluate", "expression" to "42")
            shadowOf(compat.last()).lastEvaluatedJavascriptCallback.onReceiveValue(JSONObject.quote("42"))
            assertEquals(42, read[0]!!.getInt("result"))
            assertFalse(read[0]!!.has("realm"))
            assertFalse(read[0]!!.has("origin"))
        } finally { NativesPartitionTest.Features.documentStart = true }
    }

    @Test fun withoutWorldsEverySandboxIsRefusedAndNoFrameButAPartitionedOneIsDriven() {
        NativesPartitionTest.Features.worlds = false
        val natives = natives()
        natives.ensure(1, 1, 0, 0, props("tab"), 0)
        assertEquals("NotSupportedError", call("evaluate", "expression" to "1", "sandbox" to "t3").error())
        assertEquals("NotSupportedError", call("addPreloadScript", "functionDeclaration" to "() => {}", "sandbox" to "t3").error())
        assertTrue("no world was touched", compat.worldScripts.isEmpty() && compat.worldListeners.isEmpty())
        natives.ensure(2, 1, 0, 0, props("plain", partition = null).replace("\"slot\":1", "\"slot\":2"), 0)
        assertEquals("NotSupportedError", call("evaluate", "expression" to "1").let { a -> FrameAutomations.run("evaluate",
            JSONObject().put("id", "plain").put("expression", "1")) { a[0] = it }; a }.error())
        natives.ensure(3, 1, 0, 0, props("tab").replace("\"slot\":1", "\"slot\":3"), 0)
        assertEquals("two frames with one id", "NotFoundError", call("evaluate", "expression" to "1").error())
        assertEquals("NotFoundError", call("evaluate", "expression" to "1").let { a -> FrameAutomations.run("evaluate",
            JSONObject().put("id", "none").put("expression", "1")) { a[0] = it }; a }.error())
    }
}
