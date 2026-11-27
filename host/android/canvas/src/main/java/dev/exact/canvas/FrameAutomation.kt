package dev.exact.canvas

import android.net.Uri
import android.annotation.SuppressLint
import android.view.MotionEvent
import android.webkit.WebView
import androidx.webkit.JavaScriptExecutionException
import androidx.webkit.JavaScriptExecutionWorld
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.Page
import androidx.webkit.ScriptHandler
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import androidx.webkit.WebViewOutcomeReceiver
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener
import java.util.UUID

/**
 * `@exact/browser-automation` on one kept partitioned frame (LLP 1020.002 D9, §11 "Android as built"), owned by its
 * [FrameBrowser] so it outlives a web view a renderer's end replaces.
 *
 * A page-world lexical binding records the document identity without exposing a bridge (D1). Without a `sandbox`, `evaluate` and `callFunction` are
 * `WebView.evaluateJavascript` in the page, which answers synchronously: a promise to await there is
 * `NotSupportedError`, as is a preload script there, since WebView injects one into every frame and only a script of
 * Exact's would keep it to the main one. A `sandbox` is an AndroidX execution world (`JS_INJECTION_IN_FRAME_AND_WORLD`):
 * each world a call names gets a message listener of its own and a document-start script that posts to it, so each
 * document's [JavaScriptReplyProxy] runs the call in that world and its result comes back through the listener, which
 * exists only in that world. A world reaches a document only from its start, so a sandbox is ready in the documents
 * that start after the app first named it: the names are kept across launches ([named]), and each frame's view gets
 * every one's channel when it is made.
 */
internal class FrameAutomation(private val web: () -> WebView?, private val contextId: () -> String, private val changed: () -> Unit) {
    /** `"agent"` after a call that acts, until a person's touch or key reaches the frame (D9). */
    var controller = "person"
        private set

    private class Preload(val id: String, val source: String, val sandbox: String, val session: String)
    private val preloads = ArrayList<Preload>()
    private class Recipient(val sandbox: String, val session: String, val channel: String, val send: (JSONObject) -> Unit) {
        var pending = 0
        var admitted = false
    }
    private val recipients = HashMap<String, Recipient>()

    private fun capability(sandbox: String, session: String, channel: String, send: (JSONObject) -> Unit, reserved: MutableSet<String>): String {
        val key = recipients.entries.firstOrNull { (_, r) -> r.sandbox == sandbox && r.session == session && r.channel == channel }?.key
            ?: UUID.randomUUID().toString().also {
                recipients[it] = Recipient(sandbox, session, channel, send)
            }
        if (reserved.add(key)) recipients.getValue(key).pending++
        return key
    }

    private fun releaseSession(session: String) {
        recipients.entries.removeAll { it.value.session == session }
        for (preload in preloads.filter { it.session == session }) {
            handlers.remove(preload.id)?.remove()
            preloads.remove(preload)
        }
        endAll("AbortError", "the package session ended", frameEnded = false) { it.channelSession != session }
    }

    /** What is installed on [view]: each preload's handler, each world, and each world's channel with the number of
     * navigations begun on the view before it was installed. */
    private var view: WebView? = null
    private val handlers = HashMap<String, ScriptHandler>()
    private val worlds = HashMap<String, JavaScriptExecutionWorld>()
    private val channels = HashMap<String, Int>()

    /** The document the frame shows, and each world's reply proxy in each document, by the page it posted from. */
    private var page: Page? = null
    private class Document {
        val id = UUID.randomUUID().toString()
        var token: String? = null
        var retryReady = false
    }
    private class World(val name: String, val proxy: JavaScriptReplyProxy, val doc: Document, val token: String, val origin: String) {
        var retry = false
    }
    // AndroidX pairs one JavaScriptReplyProxy with one injected JavaScript object in the frame.
    private val markers = java.util.WeakHashMap<JavaScriptReplyProxy, Document>()
    private val confirmed = HashMap<Page?, MutableMap<String, World>>()
    private val proxies = HashMap<Pair<Page?, String>, World>()
    private class Realm(val marker: String, val origin: String, val id: String)
    private val realms = HashMap<String?, Realm>()
    private val routes = HashMap<String?, String>()
    // This lexical constant adds no property, handler or listener on window. Each document gets its own value.
    private val binding = "_exact" + UUID.randomUUID().toString().replace("-", "")
    private fun plant(origin: String = "this.origin", token: String = "null", fields: String = "") =
        "const $binding = window === top ? { __proto__: null, document, token: $token, origin: $origin$fields } : null"
    private var pageIdentity: ScriptHandler? = null
    private var navigating = false
    // A listener identifies its sender, but supplies no Page. Either document can post during navigation.
    private val early = LinkedHashSet<World>()
    // An interrupted check belongs to its Page. A history traversal can resume it.
    private val checks = HashMap<Page?, MutableSet<World>>()
    private var revision = 0L
    /** Navigations begun on the view, and how many had begun when the current document's began. */
    private var starts = 0
    private var documentStart = 0

    private var nextId = 0
    /** Calls in flight: each one's answer, and the world it runs in (null: the page's). */
    private class Pending(val reply: (JSONObject) -> Unit, val world: String?, var takeAt: Long?,
                          var checking: Boolean, val admitChannels: (() -> Unit)?, val channelSession: String?,
                          var realm: Realm? = null, var source: World? = null) {
        var onReady: ((World) -> Unit)? = null
        var postingDocument: Document? = null
        var mayCrossCommit = false
    }
    private val pending = HashMap<Int, Pending>()
    private var personInput = 0L
    private val main = android.os.Handler(android.os.Looper.getMainLooper())

    init { live += this; instances += this; attach() }

    // Lifetime

    /** Installs the preload scripts and every named world's channel on the frame's view, if it is a new one. */
    @SuppressLint("ClickableViewAccessibility")
    fun attach() {
        val current = web() ?: return
        if (current === view) return
        view = current
        forgetView()
        current.setOnTouchListener { _, event -> if (event.actionMasked == MotionEvent.ACTION_DOWN) personTook(); false }
        current.setOnKeyListener { _, _, event -> if (event.action == android.view.KeyEvent.ACTION_DOWN) personTook(); false }
        // Capture the browser's serialized origin before the page can replace window.origin. This also works on
        // providers without isolated worlds. The value is read by evaluateJavascript, never through a page bridge.
        if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            pageIdentity = WebViewCompat.addDocumentStartJavaScript(current, plant(), setOf("*"))
            // The initial empty document of a new top-level WebView has no document-start event or opener.
            if (current.url == null) current.evaluateJavascript(plant("'null'"), null)
        }
        if (!ISOLATED) return
        for (name in named(current)) channel(current, name)
        for (p in preloads) install(current, p)
    }

    private fun forgetView() {
        pageIdentity = null
        realms.clear()
        handlers.clear(); worlds.clear(); channels.clear(); proxies.clear(); confirmed.clear(); early.clear(); checks.clear()
        revision++
        page = null
        navigating = false
        starts = 0; documentStart = 0
    }

    fun starting() {
        if (navigating) endAll("AbortError", "the navigation was superseded") { !it.mayCrossCommit }
        navigating = true
        starts++
    }

    fun cancelled() {
        if (!navigating) return
        navigating = false
        val at = revision
        val waiting = pending.values.filter { it.mayCrossCommit }.mapNotNull { call ->
            call.mayCrossCommit = false
            call.postingDocument?.takeIf { it.token == null }
        }.toSet()
        verifyEarlyWorlds()
        if (waiting.isNotEmpty()) for ((proxy, doc) in markers.entries.filter { it.value in waiting }) {
            // A cancelled provisional document might never send ready. Its proxy can already report its end.
            checkActive(proxy) { active ->
                if (active == false && revision == at && doc.token == null) refuseDocument(doc)
            }
        }
    }

    /** A new document committed (or one came back from the back-forward cache): a script in the old one is aborted. */
    fun committed(page: Page?) {
        realms.clear()
        if (page == null || this.page == null) {
            checks.remove(null)
            confirmed.remove(null)
            proxies.keys.removeAll { it.first == null }
        }
        this.page = page
        navigating = false
        revision++
        documentStart = starts
        // A reply requested during navigation can wait for proof that its own document survived.
        endAll("AbortError", "the document navigated away") { call ->
            val keep = call.mayCrossCommit && call.postingDocument?.let(::awaitingWorld) == true
            call.mayCrossCommit = false
            keep
        }
        early.addAll(checks[page].orEmpty())
        verifyEarlyWorlds()
    }

    fun deleted(page: Page) {
        proxies.keys.removeAll { it.first == page }
        confirmed.remove(page)
        checks.remove(page)?.clear()
    }

    /** The frame's view went (out of view, a renderer's end): its document with it. */
    fun detached() {
        view = null
        forgetView()
        endAll("AbortError", "the frame's page is gone")
    }

    /** The frame unmounted or changed its partition. */
    fun close() {
        live -= this
        view = null
        forgetView()
        endAll("NotFoundError", "the frame was removed or its partition changed")
    }

    /** A partition change makes a new frame: its preload scripts go with it, as on Apple. */
    fun adopt(old: FrameAutomation) {
        recipients.putAll(old.recipients)
        preloads += old.preloads
        nextId = maxOf(nextId, old.nextId)
        view?.let { v -> if (ISOLATED) preloads.forEach { install(v, it) } }
    }

    private fun endAll(error: String, message: String, frameEnded: Boolean = true, keep: (Pending) -> Boolean = { false }) {
        val ended = pending.filterValues { !keep(it) }
        pending.keys.removeAll(ended.keys)
        ended.values.forEach { it.reply(if (frameEnded && it.checking) missingRealm() else failure(error, message)) }
    }

    private fun admitted(call: Pending) {
        call.checking = false
        if (call.takeAt == personInput) {
            call.takeAt = null
            take()
        }
    }

    private fun take() {
        if (controller == "agent") return
        controller = "agent"
        changed()
    }

    private fun personTook() {
        val wasAgent = controller == "agent"
        personInput++
        if (!wasAgent) return
        controller = "person"
        changed()
    }

    // Calls

    fun run(op: String, args: JSONObject, recipient: ((JSONObject) -> Unit)?, reply: (JSONObject) -> Unit) {
        val reserved = mutableSetOf<String>()
        fun finish(result: JSONObject) {
            for (key in reserved) {
                val r = recipients[key] ?: continue
                if (!result.has("error")) r.admitted = true
                if (--r.pending == 0 && !r.admitted) recipients.remove(key)
            }
            reply(result)
        }
        try { run(op, args, recipient, reserved, ::finish) }
        catch (e: RuntimeException) { finish(failure("NotSupportedError", "$op: the WebView refused: $e")) }
    }

    private fun run(op: String, args: JSONObject, recipient: ((JSONObject) -> Unit)?, reserved: MutableSet<String>, reply: (JSONObject) -> Unit) {
        val sandbox = if (args.isNull("sandbox")) null else args.optString("sandbox")
        if (sandbox != null && (sandbox.isEmpty() || sandbox.startsWith("exact."))) {
            return reply(failure("SyntaxError", "${sandbox.ifEmpty { "an empty name" }} is not a sandbox: a name, not one beginning exact."))
        }
        if (sandbox != null && !ISOLATED) {
            return reply(failure("NotSupportedError", "$op: this device's WebView has no isolated worlds (JS_INJECTION_IN_FRAME_AND_WORLD), so a sandbox runs nowhere"))
        }
        val arguments = args.optJSONArray("arguments") ?: JSONArray()
        val values = (0 until arguments.length()).map { arguments.opt(it) }
        fun channelValue(value: Any?) = value is JSONObject && value.opt("type") == "channel" &&
            value.length() == 2 && value.optJSONObject("value")?.let { it.length() == 1 && it.opt("channel") is String } == true
        val hasChannel = op == "callFunction" && values.any(::channelValue)
        val session = args.optString("channelSession")
        var argumentSource = if (op == "addPreloadScript") "" else "...JSON.parse(${JSONObject.quote(arguments.toString())})"
        if (hasChannel || op == "addPreloadScript" && values.isNotEmpty()) {
            if (sandbox == null) return reply(failure("NotSupportedError", "script.message requires a named sandbox"))
            if (recipient == null) return reply(failure("NotSupportedError", "script.message has no receiver"))
            val expressions = values.mapIndexed { i, arg ->
                if (arg is JSONObject && channelValue(arg)) {
                    val channel = arg.getJSONObject("value").getString("channel")
                    val key = capability(sandbox, session, channel, recipient, reserved)
                    """((post, capability) => value => {
                        const json = JSON.stringify(value);
                        if (json === undefined) throw new TypeError('script.message needs JSON');
                        if (new TextEncoder().encode(json).length > $LIMIT) throw new DOMException('script.message exceeds 65536 bytes', 'QuotaExceededError');
                        post(JSON.stringify({capability, json}));
                    })($CHANNEL.postMessage.bind($CHANNEL), ${JSONObject.quote(key)})"""
                } else {
                    if (op == "addPreloadScript") return reply(failure("invalid argument", "preload arguments must be ChannelValues"))
                    "values[$i]"
                }
            }
            argumentSource = "...(values => [${expressions.joinToString(",")}])(JSON.parse(${JSONObject.quote(arguments.toString())}))"
        }
        // The preload scripts are the frame's, kept while it has no view and installed on the next.
        attach()
        when (op) {
            "addPreloadScript" -> {
                if (sandbox == null) return reply(failure("NotSupportedError", "addPreloadScript: Android's WebView runs a preload script in every frame of the page's world, and Exact puts nothing of its own there to keep it to the main one; name a sandbox"))
                val p = Preload("script-${++nextId}", "(\n${args.optString("functionDeclaration")}\n)($argumentSource)", sandbox, session)
                view?.let { name(it, sandbox); install(it, p) }
                preloads += p
                return reply(JSONObject().put("script", p.id))
            }
            "removePreloadScript" -> {
                val id = args.optString("script")
                val index = preloads.indexOfFirst { it.id == id }
                if (index < 0) return reply(failure("NotFoundError", "no preload script $id"))
                preloads.removeAt(index)
                handlers.remove(id)?.remove()
                return reply(JSONObject())
            }
        }
        val view = view ?: return reply(if (args.optJSONObject("target")?.opt("realm") is String) missingRealm()
            else failure("InvalidStateError", "$op: the frame is out of view, with no page"))
        val admitChannels = if (reserved.isEmpty()) null else ({ reserved.forEach { recipients[it]?.admitted = true } })
        when (op) {
            "evaluate", "callFunction" -> script(op, args, sandbox, view, argumentSource, admitChannels, reply)
            else -> reply(failure("NotSupportedError", "$op is not offered on Android (LLP 1020.002 §11)"))
        }
    }

    private fun script(op: String, args: JSONObject, sandbox: String?, view: WebView, argumentSource: String, admitChannels: (() -> Unit)?, reply: (JSONObject) -> Unit) {
        val target = args.optJSONObject("target")?.optString("realm")?.takeIf { !args.getJSONObject("target").isNull("realm") }
        if (target == null && args.optBoolean("userActivation")) take()
        val awaited = if (args.isNull("awaitPromise")) true else args.optBoolean("awaitPromise", true)
        val number = ++nextId
        // A call can end at navigation while its last post is still in transit. Acknowledge the
        // registration before giving the function to guest code, even if its result never settles.
        val arguments = if (admitChannels == null) argumentSource else """
            ...(await new Promise((resolve, reject) => {
              const bound = e => {
                if (e.data !== 'bound:$number' && e.data !== 'closed:$number') return;
                $CHANNEL.removeEventListener('message', bound);
                if (e.data === 'closed:$number') return reject(new DOMException('The call ended', 'AbortError'));
                resolve([$argumentSource]);
              };
              $CHANNEL.addEventListener('message', bound);
              $CHANNEL.postMessage(JSON.stringify({n: $number, bind: true, marker: $binding.token}));
            }))
        """.trimIndent()
        val value = if (op == "evaluate") "(\n${args.optString("expression")}\n)"
            else "(\n${args.optString("functionDeclaration")}\n)($arguments)"
        val async = if (admitChannels == null) "" else "async "
        val ran = "(${async}() => { try { return { v: $value }; } catch (e) { return { e }; } })()"
        val settle = when {
            sandbox == null && awaited -> "if (v && typeof v.then === 'function') return { __exactPromise: true };"
            sandbox == null -> "if (v && typeof v.then === 'function') return '{}';"
            awaited -> "v = await v;"
            else -> "if (v && typeof v.then === 'function') v = {};"
        }
        val text = "if ('e' in r) throw r.e; let v = r.v; $settle const t = JSON.stringify(v); return typeof t === 'string' ? t : 'null';"
        if (sandbox != null && target == null) {
            try { name(view, sandbox) } catch (e: RuntimeException) { return reply(failure("NotSupportedError", "$op: the WebView refused: $e")) }
        }
        val call = Pending(reply, sandbox, personInput.takeIf { target != null && args.optBoolean("userActivation", false) }, target != null,
            admitChannels, if (admitChannels != null) args.optString("channelSession") else null)
        pending[number] = call
        fun answer(result: JSONObject) { pending.remove(number)?.reply?.invoke(result) }
        fun identity(marker: String, origin: String, document: String = marker): Realm? {
            val route = routes.getOrPut(sandbox) { UUID.randomUUID().toString() }
            val id = "$route.$document"
            if (target != null && target != id) return null
            return Realm(marker, origin, id).also { realms[sandbox] = it }
        }
        fun guarded(realm: Realm, body: String, plantToken: Boolean = false): String {
            val active = if (sandbox == null) "" else " && $binding.active()"
            val marker = JSONObject.quote(realm.marker)
            val token = if (plantToken) "($binding.token ??= $marker)" else "$binding.token"
            return "(typeof $binding === 'object' && $binding && $binding.document === document && $token === $marker$active) ? ($body) : false"
        }
        // Admit a realm call before entering guest code. Navigation after admission aborts the call.
        // The second guard still runs in the evaluation that executes the caller's expression.
        fun admit(realm: Realm, execute: (String, (String?) -> Unit) -> Unit, run: () -> Unit) {
            if (target == null) return run()
            execute(guarded(realm, "(() => { void (() => ($ran)); return true })()")) { result ->
                if (number in pending) when (result) {
                    "true" -> { admitted(call); run() }
                    "false" -> answer(missingRealm())
                    else -> answer(failure("SyntaxError", "the script does not parse"))
                }
            }
        }
        if (sandbox == null) {
            fun evaluate(source: String, receive: (String?) -> Unit) {
                try { view.evaluateJavascript(source, receive) }
                catch (e: RuntimeException) { answer(failure("NotSupportedError", "$op: the WebView refused: $e")) }
            }
            val run = "((r) => { try { $text } catch (e) { $THROWN } })($ran)"
            if (pageIdentity == null) {
                if (target != null) return answer(failure("NotSupportedError", "$op: realm targeting needs DOCUMENT_START_SCRIPT"))
                return evaluate(run) { json -> answer(pageResult(json)) }
            }
            fun readRealm(next: (Realm) -> Unit) {
                val token = if (target == null) "($binding.token ??= \"${UUID.randomUUID()}\")" else "$binding.token"
                evaluate("typeof $binding === 'object' && $binding && $binding.document === document ? $token + '\\n' + $binding.origin : null") { json ->
                    if (number !in pending) return@evaluate
                    val marker = json?.let { runCatching { JSONTokener(it).nextValue() as? String }.getOrNull() }
                    if (marker == null || '\n' !in marker) answer(if (target != null) missingRealm() else failure("InvalidStateError", "the document's realm did not start"))
                    else identity(marker.substringBefore('\n'), marker.substringAfter('\n'))?.let(next) ?: answer(missingRealm())
                }
            }
            fun execute(realm: Realm, retry: Boolean) {
                if (number !in pending) return
                val body = guarded(realm, run)
                admit(realm, ::evaluate) {
                    evaluate(body) { json ->
                        if (number !in pending) return@evaluate
                        if (json == "false" && target == null && retry) {
                            return@evaluate readRealm { execute(it, false) }
                        }
                        val result = if (json == "false") failure("AbortError", "the document navigated away") else pageResult(json)
                        if (result.has("result")) result.put("realm", realm.id).put("origin", realm.origin)
                        answer(result)
                    }
                }
            }
            realms[null]?.takeIf { target == null || it.id == target }?.let { return execute(it, true) }
            readRealm { execute(it, false) }
            return
        }
        val waitForArguments = if (admitChannels == null) "" else "r = await r;"
        val body = "((post, r) => { (async () => { try { $waitForArguments $text } catch (e) { $THROWN } })()" +
            ".then(x => post(JSON.stringify({ n: $number, marker: $binding.token, r: x })), () => post(JSON.stringify({ n: $number, marker: $binding.token, r: null })));" +
            " return true; })($CHANNEL.postMessage.bind($CHANNEL), $ran)"
        fun send(world: World) {
            if (number !in pending) return
            val realm = identity(world.token, world.origin, world.doc.id) ?: return answer(missingRealm())
            call.realm = realm
            call.onReady = null
            call.mayCrossCommit = false
            call.source = world
            fun execute(source: String, receive: (String?) -> Unit) {
                try { world.proxy.executeJavaScript(source, object : WebViewOutcomeReceiver<String, JavaScriptExecutionException> {
                    override fun onResult(result: String?) { if (number in pending) receive(result) }
                    override fun onError(error: JavaScriptExecutionException) {
                        answer(if (error.errorType == JavaScriptExecutionException.ERROR_FRAME_DESTROYED) {
                            if (call.checking) missingRealm() else failure("AbortError", "the document navigated away")
                        } else failure("Error", error.message ?: "the script did not run"))
                    }
                }) } catch (e: RuntimeException) { answer(failure("NotSupportedError", "$op: the WebView refused: $e")) }
            }
            admit(realm, ::execute) {
                execute(guarded(realm, body, plantToken = target == null)) { result ->
                    when (result) {
                        "true" -> {
                            confirmed.getOrPut(page) { HashMap() }[realm.id] = world
                            admitted(call)
                        }
                        "false" -> answer(failure("AbortError", "the document navigated away"))
                        else -> answer(failure("SyntaxError", "the script does not parse"))
                    }
                }
            }
        }
        fun waitForReady(timeout: JSONObject, document: Document? = null) {
            call.onReady = ::send
            call.postingDocument = document
            call.mayCrossCommit = document != null && navigating
            main.postDelayed({ if (call.onReady != null) answer(timeout) }, READY_WAIT)
            for (world in checks[page].orEmpty().filter { it.retry && it.name == sandbox && (document == null || it.doc === document) }) {
                verifyWorld(world)
            }
        }
        val key = page to sandbox
        if (target != null) {
            confirmed[page]?.get(target)?.let { return send(it) }
            val document = markers.values.firstOrNull { target == "${routes[sandbox]}.${it.id}" }
            if (document != null && awaitingWorld(document)) {
                // Both the channel post and ready can precede the native commit, in either order.
                waitForReady(missingRealm(), document)
                return
            }
        }
        proxies[key]?.let { return send(it) }
        if (target != null) return answer(missingRealm())
        // The world's channel was on the view before this document began, so its post is on its way.
        val installed = channels[sandbox]
        if (checks[page].orEmpty().any { it.name == sandbox } || installed != null && (navigating || installed < documentStart)) {
            waitForReady(failure("InvalidStateError", "$op: the document's sandbox $sandbox did not start"))
            return
        }
        answer(failure("InvalidStateError",
            "$op: sandbox $sandbox reaches this frame from its next document: Android's WebView runs a world's script only from a document's start"))
    }

    /** `evaluateJavascript`'s answer: the JSON of what the wrapper returned, or `null` where the script did not parse. */
    private fun pageResult(json: String?): JSONObject {
        val value = json?.let { runCatching { JSONTokener(it).nextValue() }.getOrNull() }
        return when {
            value is String -> settled(value)
            value is JSONObject && value.optBoolean("__exactPromise") -> failure("NotSupportedError",
                "awaiting a promise in the page's world needs a script of Exact's there; name a sandbox, or pass awaitPromise: false")
            value is JSONObject && value.optBoolean("__exactThrown") -> thrown(value)
            else -> failure("SyntaxError", "the script does not parse")
        }
    }

    // Worlds

    /** Notes a sandbox name for every frame: its channel is installed on each frame's view, now and when made. */
    private fun name(view: WebView, sandbox: String) {
        val names = named(view)
        if (sandbox !in names) {
            names += sandbox
            while (names.size > KEPT) names.remove(names.first())
            view.context.getSharedPreferences(PREFERENCES, android.content.Context.MODE_PRIVATE).edit()
                .putString("sandboxes", JSONArray(names.toList()).toString()).apply()
        }
        for (a in live) a.view?.let { a.channel(it, sandbox) }
    }

    private fun world(view: WebView, name: String) = worlds.getOrPut(name) { WebViewCompat.getExecutionWorld(view, name) }

    /** [name]'s listener, which only scripts in that world see, and the main frame's post to it at each document's start. */
    private fun channel(view: WebView, name: String) {
        if (name in channels) return
        val world = world(view, name)
        WebViewCompat.addWebMessageListener(view, CHANNEL, setOf("*"), world) { _, message, origin, isMainFrame, proxy ->
            if (!isMainFrame || message.type != WebMessageCompat.TYPE_STRING) return@addWebMessageListener
            val data = message.data ?: return@addWebMessageListener
            if (data.startsWith("ready:")) {
                val token = data.removePrefix("ready:")
                if (token.length != 32 || token.any { it !in "0123456789abcdef" }) return@addWebMessageListener
                val doc = markers.getOrPut(proxy) { Document() }
                if (this.view !== view) return@addWebMessageListener
                if (doc.token != null) {
                    if (doc.token != token) return@addWebMessageListener
                    if (!doc.retryReady) {
                        checks[page]?.firstOrNull { it.doc === doc && it.retry }?.let(::verifyWorld)
                        return@addWebMessageListener
                    }
                }
                doc.token = token
                doc.retryReady = false
                val world = World(name, proxy, doc, token, serializedOrigin(origin))
                if (navigating) early.add(world) else verifyWorld(world)
                return@addWebMessageListener
            }
            val posted = runCatching { JSONObject(data) }.getOrNull() ?: return@addWebMessageListener
            if (posted.has("capability")) {
                val recipient = recipients[posted.optString("capability")] ?: return@addWebMessageListener
                if (recipient.sandbox != name) return@addWebMessageListener
                val json = posted.opt("json") as? String ?: return@addWebMessageListener
                if (json.toByteArray(Charsets.UTF_8).size > LIMIT) return@addWebMessageListener
                if (this.view !== view || web() !== view) {
                    if (reportedRetired.add(recipient.session)) Exact.journal("script.message: dropped a post from a retired web view")
                    return@addWebMessageListener
                }
                val doc = markers.getOrPut(proxy) { Document() }
                val route = routes.getOrPut(name) { UUID.randomUUID().toString() }
                val source = JSONObject().put("realm", "$route.${doc.id}").put("origin", serializedOrigin(origin)).put("context", contextId())
                recipient.send(JSONObject().put("channel", recipient.channel).put("json", json).put("source", source))
                return@addWebMessageListener
            }
            if (this.view !== view) return@addWebMessageListener
            val n = posted.optInt("n")
            val call = pending[n]
            if (call == null) {
                if (posted.optBoolean("bind")) proxy.postMessage("closed:$n")
                return@addWebMessageListener
            }
            val realm = call.realm ?: return@addWebMessageListener
            val source = call.source ?: return@addWebMessageListener
            if (call.world != name || source.proxy !== proxy || realm.origin != serializedOrigin(origin) ||
                posted.optString("marker") != realm.marker) return@addWebMessageListener
            if (posted.optBoolean("bind")) {
                val accept = call.admitChannels ?: return@addWebMessageListener
                accept()
                proxy.postMessage("bound:$n")
                return@addWebMessageListener
            }
            confirmed.getOrPut(page) { HashMap() }[realm.id] = source
            admitted(call)
            val reply = pending.remove(n)!!.reply
            reply(when (val r = posted.opt("r")) {
                is String -> settled(r).also { if (it.has("result")) it.put("realm", realm.id).put("origin", realm.origin) }
                is JSONObject -> thrown(r)
                else -> failure("Error", "the script's exception has no text")
            })
        }
        val active = """, active: (length => () => { try { length(); return true; } catch { return false; } })(
            Object.getOwnPropertyDescriptor(History.prototype, 'length').get.bind(history))""".trimIndent()
        val token = "Array.from(crypto.getRandomValues(new Uint8Array(16)), n => n.toString(16).padStart(2, '0')).join('')"
        try {
            WebViewCompat.addJavaScriptOnEvent(view, "${plant(token = token, fields = active)}; if (window === top) $CHANNEL.postMessage('ready:' + $binding.token);",
                WebViewCompat.INJECTION_EVENT_DOCUMENT_START, setOf("*"), world)
        } catch (e: RuntimeException) {
            WebViewCompat.removeWebMessageListener(view, world, CHANNEL)
            throw e
        }
        channels[name] = starts
    }

    private fun awaitingWorld(doc: Document) =
        doc.token == null || early.any { it.doc === doc } || checks[page].orEmpty().any { it.doc === doc }

    private fun verifyEarlyWorlds() {
        val batch = early.toList()
        early.clear()
        val at = revision
        for (world in batch) if (revision == at) verifyWorld(world)
    }

    // HTML's History.length getter throws unless its document is fully active. Check after
    // commit or cancellation if ready arrived during navigation, before sending the caller's script.
    private fun verifyWorld(world: World) {
        val at = revision
        val candidates = checks.getOrPut(page) { LinkedHashSet() }
        candidates.add(world)
        world.retry = false
        checkActive(world.proxy) { active ->
            if (revision != at || world !in candidates) return@checkActive
            world.retry = active == null
            if (!world.retry) candidates.remove(world)
            when (active) {
                true -> ready(world.name, world)
                false -> {
                    world.doc.retryReady = true
                    refuseDocument(world.doc)
                }
                null -> endAll("Error", "the WebView could not check the document", frameEnded = false) { it.postingDocument !== world.doc }
            }
        }
    }

    private fun refuseDocument(doc: Document) =
        endAll("AbortError", "the document navigated away") { it.postingDocument !== doc }

    private fun checkActive(proxy: JavaScriptReplyProxy, answer: (Boolean?) -> Unit) {
        try {
            proxy.executeJavaScript("$binding.active()", object : WebViewOutcomeReceiver<String, JavaScriptExecutionException> {
                override fun onResult(result: String?) = answer(result?.toBooleanStrictOrNull())
                override fun onError(error: JavaScriptExecutionException) =
                    answer(if (error.errorType == JavaScriptExecutionException.ERROR_FRAME_DESTROYED) false else null)
            })
        } catch (_: RuntimeException) { answer(null) }
    }

    /** The current document's post in [name]'s world: its proxy, and the calls that waited for it. */
    private fun ready(name: String, proxy: World) {
        proxies[page to name] = proxy
        for (call in pending.values.toList()) {
            if (call.world == name && (call.postingDocument == null || call.postingDocument === proxy.doc)) {
                call.onReady?.invoke(proxy)
            }
        }
    }

    private fun install(view: WebView, p: Preload) {
        channel(view, p.sandbox)
        handlers[p.id] = WebViewCompat.addJavaScriptOnEvent(view, "if (window === top) ${p.source};",
            WebViewCompat.INJECTION_EVENT_DOCUMENT_START, setOf("*"), world(view, p.sandbox))
    }

    companion object {
        /** The listener's name, in each sandbox's world only. */
        private const val CHANNEL = "__exactAutomation"
        /** The exception `e` as a value, or `Error` where its name or text throws. */
        private const val THROWN = "try { return { __exactThrown: true, name: String(e?.name ?? 'Error'), message: String(e?.message ?? e) }; } " +
            "catch { return { __exactThrown: true, name: 'Error', message: '' }; }"
        /** How long a call waits for a document's world to start. */
        private const val READY_WAIT = 10_000L
        /** A result's JSON, at most this many bytes (Apple's limit). */
        private const val LIMIT = 64 * 1024
        private val ISOLATED get() = WebViewFeature.isFeatureSupported(WebViewFeature.JS_INJECTION_IN_FRAME_AND_WORLD)

        /** Every frame's automation. */
        private val live = LinkedHashSet<FrameAutomation>()

        // A retired WebView can still deliver a post. Release the receiver there as well.
        private val instances = java.util.Collections.newSetFromMap(java.util.WeakHashMap<FrameAutomation, Boolean>())
        private val reportedRetired = HashSet<String>()
        fun release(session: String) {
            reportedRetired.remove(session)
            instances.toList().forEach { it.releaseSession(session) }
        }

        /** The sandboxes the app has named, newest last, at most [KEPT], in its preferences across launches. */
        private var names: LinkedHashSet<String>? = null
        private const val PREFERENCES = "exact.browser-automation"
        private const val KEPT = 32
        private fun named(view: WebView): LinkedHashSet<String> = names ?: LinkedHashSet<String>().also { set ->
            val kept = view.context.getSharedPreferences(PREFERENCES, android.content.Context.MODE_PRIVATE).getString("sandboxes", null)
            kept?.let { runCatching { JSONArray(it) }.getOrNull() }?.let { a -> (0 until a.length()).mapTo(set) { a.optString(it) } }
            names = set
        }

        private fun serializedOrigin(origin: Uri): String {
            val scheme = origin.scheme ?: return "null"
            val authority = origin.encodedAuthority ?: return "null"
            return when (scheme) {
                "https" -> "https://" + authority.removeSuffix(":443")
                "http" -> "http://" + authority.removeSuffix(":80")
                else -> "$scheme://$authority"
            }
        }


        fun missingRealm() = failure("no such frame", "the script realm is no longer live")

        fun resolveRealm(id: String, frames: List<FrameAutomation?>): Pair<FrameAutomation, String?>? {
            for (frame in frames.filterNotNull()) for ((sandbox, route) in frame.routes) {
                if (id.startsWith("$route.")) return frame to sandbox
            }
            return null
        }

        fun failure(error: String, message: String): JSONObject = JSONObject().put("error", error).put("message", message)

        private fun settled(text: String): JSONObject {
            val bytes = text.toByteArray(Charsets.UTF_8).size
            if (bytes > LIMIT) return failure("QuotaExceededError", "the result is $bytes bytes of JSON; at most $LIMIT")
            return JSONObject().put("result", runCatching { JSONTokener(text).nextValue() }.getOrNull() ?: JSONObject.NULL)
        }

        /** The guest's exception by its own name (`TypeError`), else `Error`, as on Apple. */
        private fun thrown(value: JSONObject): JSONObject {
            val name = value.optString("name")
            return failure(if (name.endsWith("Error") && ' ' !in name) name else "Error", value.optString("message"))
        }
    }
}

/**
 * `@exact/browser-automation`'s calls on Android, for its implementation, on the main thread: the one live iframe of
 * this app with the call's `id` (LLP 1020.002 D9). A frame without a kept partition answers `NotSupportedError`, as on
 * Apple; none, or more than one (two sessions mounting the same id), `NotFoundError`.
 */
object FrameAutomations {
    /** Every reader, each with its frames. */
    internal val readers: MutableSet<Natives> = java.util.Collections.newSetFromMap(java.util.WeakHashMap())

    @JvmStatic fun run(op: String, args: JSONObject, reply: (JSONObject) -> Unit) = run(op, args, null, reply)
    @JvmStatic fun release(session: String) = FrameAutomation.release(session)

    @JvmStatic fun run(op: String, args: JSONObject, recipient: ((JSONObject) -> Unit)?, reply: (JSONObject) -> Unit) {
        val target = args.optJSONObject("target")
        val invalid = { FrameAutomation.failure("invalid argument", "use a string context and optional sandbox, or a string realm") }
        if (!args.isNull("target") && target == null) return reply(invalid())
        if (target != null) {
            if (listOf("context", "sandbox", "realm").any { !target.isNull(it) && target.opt(it) !is String }) return reply(invalid())
            if (op !in setOf("evaluate", "callFunction") || !args.isNull("id") || !args.isNull("sandbox") ||
                target.isNull("realm") == target.isNull("context") || !target.isNull("realm") && !target.isNull("sandbox")) {
                return reply(FrameAutomation.failure("invalid argument", "use one script target: context and optional sandbox, or realm"))
            }
            if (!target.isNull("realm")) {
                val (frame, sandbox) = FrameAutomation.resolveRealm(target.getString("realm"), readers.flatMap { it.automations(null) }) ?: return reply(FrameAutomation.missingRealm())
                args.put("sandbox", sandbox)
                return frame.run(op, args, recipient, reply)
            }
            args.put("sandbox", target.opt("sandbox"))
        }
        val id = target?.optString("context") ?: args.optString("id").takeUnless { args.isNull("id") }
            ?: return reply(FrameAutomation.failure("invalid argument", "a script needs an iframe id or a target"))
        val frames = readers.flatMap { it.automations(id) }
        if (frames.size != 1) return reply(FrameAutomation.failure("NotFoundError", "no unique live iframe with id $id"))
        val automation = frames[0] ?: return reply(FrameAutomation.failure("NotSupportedError",
            "$op: iframe $id has no partition; only a partitioned frame is driven"))
        automation.run(op, args, recipient, reply)
    }
}
