// On-device translation and bundled language identification (LLP 1133.027 D7–D14).
package exact.translation

import android.content.ComponentCallbacks2
import android.content.Context
import android.content.res.Configuration
import android.icu.text.BreakIterator
import android.net.ConnectivityManager
import android.os.Handler
import android.os.Looper
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.android.gms.tasks.Task
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.common.model.DownloadConditions
import com.google.mlkit.common.model.RemoteModelManager
import com.google.mlkit.nl.languageid.LanguageIdentification
import com.google.mlkit.nl.languageid.LanguageIdentificationOptions
import com.google.mlkit.nl.languageid.LanguageIdentifier
import com.google.mlkit.nl.translate.TranslateLanguage
import com.google.mlkit.nl.translate.TranslateRemoteModel
import com.google.mlkit.nl.translate.Translator
import com.google.mlkit.nl.translate.TranslatorOptions
import dev.exact.canvas.Cause
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject

class TranslationModule internal constructor(context: ExtensionContext, private val models: TranslationModels) : ExactExtension(context) {
    constructor(context: ExtensionContext) : this(context, TranslationModels())

    companion object {
        const val PAIR_TOPIC = "translation.translatorAvailability"
        const val DETECTOR_TOPIC = "translation.languageDetectorAvailability"

        /** D13 lets the app set the executor. The package owns the language pair. */
        var optionsHatch: ((TranslatorOptions.Builder) -> Unit)? = null

        /** ML Kit's languages as D7's table lists them. */
        internal val listed: List<String> by lazy { Languages.mlkitListed(TranslateLanguage.getAllLanguages()) }

        internal fun error(name: String, message: String? = null): JSONObject =
            JSONObject().put("error", name).also { if (message != null) it.put("message", message) }
    }

    private val main = Handler(Looper.getMainLooper())
    private var serial = 0
    private val translators = HashMap<String, Handle>()
    private val detectors = HashMap<String, Handle>()
    private var ended = false
    private var watchingMemory = false
    private val memory = object : ComponentCallbacks2 {
        override fun onConfigurationChanged(config: Configuration) {}
        override fun onLowMemory() = discard()
        override fun onTrimMemory(level: Int) {
            if (level != ComponentCallbacks2.TRIM_MEMORY_UI_HIDDEN) discard()
        }
        private fun discard() {
            for (h in translators.values) serially(h) { done -> h.closeClient(); done() }
        }
    }

    // The agent's words (§9).
    private var pairOverlay = JSONObject()
    private var detectorOverlay = JSONObject()
    private val translations = ArrayList<JSONObject>()
    private val detections = ArrayList<JSONObject>()
    private var seenTranslations = ""
    private var seenDetections = ""
    private val removed = HashSet<String>()

    /** Downloads in flight by pair key, and the creates waiting on each: every one hears the end (null when done). */
    private class Download {
        val waiters = ArrayList<(JSONObject?) -> Unit>()
        var loading = false
        var client: Translator? = null
    }
    private val downloads = HashMap<String, Download>()
    private val cancellations = HashMap<Int, () -> Unit>()

    private val answered = ArrayDeque<String>()
    private fun say(line: String) {
        if (context.agent) context.journal("translation: $line") else context.log("translation: $line")
    }

    private fun disclose() {
        if (!context.agent) return
        fun handles(entries: Collection<Handle>) = JSONArray(entries.map { h ->
            JSONObject().put("id", h.id).put("platform", "agent").put("state", h.state).put("calls", h.calls).apply {
                h.pair?.let { put("sourceLanguage", it.source); put("targetLanguage", it.target) }
                h.expected?.let { put("expectedInputLanguages", it) }
            }
        })
        context.state(JSONObject().put("translators", handles(translators.values)).put("detectors", handles(detectors.values))
            .put("translations", translations.size).put("detections", detections.size).put("answered", JSONArray(answered.toList())))
    }

    /** A translator or detector: its languages, its work one at a time, and ML Kit's client behind it. */
    private class Handle(val id: String, val pair: Pair? = null, val expected: JSONArray? = null) {
        var calls = 0
        var client: Translator? = null
        var identifier: LanguageIdentifier? = null
        fun closeClient() { client?.close(); client = null }
        val queue = ArrayDeque<(() -> Unit) -> Unit>()
        var running = false
        var abort: (() -> Unit)? = null
        var state = "live"
        var dead = false
    }

    /** `work` after every call on `handle` admitted before it. */
    private fun serially(handle: Handle, work: (() -> Unit) -> Unit) {
        handle.calls++
        disclose()
        handle.queue.addLast(work)
        if (handle.running) return
        fun next() {
            val w = handle.queue.removeFirstOrNull() ?: run { handle.running = false; return }
            handle.running = true
            w { handle.calls--; disclose(); main.post { next() } }
        }
        next()
    }

    private fun onHandle(handle: Handle, request: Int?, reply: (JSONObject) -> Unit, work: ((JSONObject) -> Unit, () -> Boolean) -> Unit) {
        var waiting = true
        fun finish(value: JSONObject) {
            if (!waiting) return
            waiting = false
            request?.let(cancellations::remove)
            reply(value)
        }
        if (request != null) cancellations[request] = { finish(error("AbortError")) }
        serially(handle) { done ->
            if (handle.dead || !waiting) {
                finish(error("AbortError"))
                done()
                return@serially
            }
            var running = true
            fun answer(value: JSONObject) {
                if (!running) return
                running = false
                handle.abort = null
                finish(value)
                done()
            }
            handle.abort = { answer(error("AbortError")) }
            work(::answer) { waiting }
        }
    }

    override fun later(name: String, args: JSONObject, preferred: Any?, cause: Cause, reply: (JSONObject) -> Unit) {
        if (!context.agent) return dispatch(name, args, cause.request, reply)
        dispatch(name, args, cause.request) { answer ->
            val entry = "$name (${cause.origin})"
            context.journal("translation: $entry")
            answered.addLast(entry)
            if (answered.size > 16) answered.removeFirst()
            disclose()
            reply(answer)
        }
    }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) = dispatch(name, args, null, reply)

    private fun dispatch(name: String, args: JSONObject, request: Int?, reply: (JSONObject) -> Unit) {
        when (name) {
            "translatorAvailability" -> pairAvailability(args, reply)
            "createTranslator" -> createTranslator(args, request, reply)
            "translate" -> translate(args, request, reply)
            "destroyTranslator" -> destroy(translators, args.opt("translator"), reply)
            "languageDetectorAvailability" -> reply(detectorAvailability(args))
            "createLanguageDetector" -> createDetector(args, reply)
            "detectLanguage" -> detect(args, request, reply)
            "destroyLanguageDetector" -> destroy(detectors, args.opt("detector"), reply)
            else -> reply(error("NotSupportedError"))
        }
    }

    // ---------------------------------------------------------------- languages (D7)

    private data class Pair(val source: String, val target: String, val s: String, val t: String) {
        val key = "$s>$t"
        fun installed(models: Set<String>) = s in models && t in models
    }

    private fun pair(args: JSONObject): Pair? {
        val source = (args.opt("sourceLanguage") as? String)?.let(Languages::canonical) ?: return null
        val target = (args.opt("targetLanguage") as? String)?.let(Languages::canonical) ?: return null
        val supported = if (context.agent) Languages.agent else listed
        val s = Languages.listed(source, supported) ?: return null
        val t = Languages.listed(target, supported) ?: return null
        return if (s == t) null else Pair(source, target, Languages.mlkitCode(s), Languages.mlkitCode(t))
    }

    // ---------------------------------------------------------------- availability (D8)

    /** The translation models on the device, by ML Kit code. */
    private fun downloaded(done: (Set<String>?) -> Unit) {
        sdkTask { models.downloaded() }.addOnCompleteListener {
            if (it.isSuccessful) done(it.result) else {
                say("the downloaded models were not read: ${it.exception?.message ?: "cancelled"}")
                done(null)
            }
        }
    }

    private fun pairAvailability(args: JSONObject, reply: (JSONObject) -> Unit) {
        val p = pair(args) ?: return reply(JSONObject().put("availability", "unavailable").put("reason", "unsupported-options"))
        if (context.agent) return reply(during(agentPair(p), p.key))
        downloaded { models ->
            if (models == null) return@downloaded reply(JSONObject().put("availability", "unavailable").put("reason", "model-not-ready"))
            val word = when {
                p.installed(models) -> "available"
                this.models.canDownload(context.application) -> "downloadable"
                else -> "unavailable"
            }
            val a = JSONObject().put("availability", word)
                .put("android", JSONObject().put("downloadedModels", JSONArray(Languages.mlkitListed(models.sorted()))))
            if (word == "unavailable") a.put("reason", "device-not-eligible")
            reply(during(a, p.key))
        }
    }

    private fun agentPair(p: Pair): JSONObject {
        val a = JSONObject().put("availability", if (p.key in removed) "downloadable" else "available")
        for (k in pairOverlay.keys()) a.put(k, pairOverlay.get(k))
        if (a.optString("availability") != "unavailable") a.remove("reason")
        return a
    }

    /** An availability with a download in flight laid over it. */
    private fun during(a: JSONObject, key: String): JSONObject {
        val word = a.optString("availability")
        if (downloads[key]?.loading != true || (word != "downloadable" && word != "downloading")) return a
        a.put("availability", "downloading")
        // ML Kit reports no progress; the agent's download stands at 0 until the drive completes it (§9).
        if (context.agent) a.put("progress", 0) else a.remove("progress")
        return a
    }

    private fun detectorAvailability(args: JSONObject): JSONObject {
        val expected = args.optJSONArray("expectedInputLanguages")
        if (expected != null && (0 until expected.length()).any { (expected.opt(it) as? String)?.let(Languages::identifies) != true }) {
            return JSONObject().put("availability", "unavailable").put("reason", "unsupported-options")
        }
        // Language Identification's model is bundled with the app: never a download.
        val a = JSONObject().put("availability", "available")
        if (context.agent) for (k in detectorOverlay.keys()) a.put(k, detectorOverlay.get(k))
        return a
    }

    // ---------------------------------------------------------------- translators (D9)

    private fun createTranslator(args: JSONObject, request: Int?, reply: (JSONObject) -> Unit) {
        val p = pair(args) ?: return reply(error("NotSupportedError", "unsupported-options"))
        val made = {
            serial += 1
            val h = Handle("translator-$serial", p)
            translators[h.id] = h
            say("${h.id}: ${p.source} to ${p.target}")
            reply(JSONObject().put("id", h.id).put("sourceLanguage", p.source).put("targetLanguage", p.target)
                .put("attribution", "google-translate"))
        }
        if (context.agent) {
            val a = during(agentPair(p), p.key)
            when (a.optString("availability")) {
                "available" -> made()
                "unavailable" -> reply(error("NotSupportedError", a.optString("reason", "unsupported-options")))
                else -> {
                    val d = join(p.key, request) { e -> if (e == null) made() else reply(e) } ?: return
                    d.loading = true
                    context.changed(PAIR_TOPIC)
                    say("${p.key}: download started (agent)")
                }
            }
            return
        }
        val d = join(p.key, request) { e -> if (e == null) made() else reply(e) } ?: return
        downloaded { installed ->
            if (downloads[p.key] !== d) return@downloaded
            if (installed == null) return@downloaded settle(p.key, error("OperationError", "model-not-ready"))
            if (p.installed(installed)) return@downloaded settle(p.key, null)
            if (!models.canDownload(context.application)) return@downloaded settle(p.key, error("NotSupportedError", "device-not-eligible"))
            // With no network DownloadManager can keep the request pending indefinitely.
            if (!models.hasNetwork(context.application)) return@downloaded settle(p.key, error("NetworkError", "offline"))
            val android = args.optJSONObject("android")
            val conditions = DownloadConditions.Builder().apply {
                if (android?.optBoolean("requireWifi") == true) requireWifi()
                if (android?.optBoolean("requireCharging") == true) requireCharging()
            }.build()
            d.loading = true
            context.changed(PAIR_TOPIC)
            sdkTask {
                models.client(p.s, p.t).also { d.client = it }.downloadModelIfNeeded(conditions)
            }.addOnCompleteListener { task ->
                // Translator.create rejects NetworkError if its model download fails.
                if (downloads[p.key] === d) settle(p.key, if (task.isSuccessful) null else error("NetworkError", task.exception?.message))
            }
        }
    }

    /** The first create owns the inventory read and any download. Every waiter hears its result. */
    private fun join(key: String, request: Int?, done: (JSONObject?) -> Unit): Download? {
        val current = downloads[key]
        val download = current ?: Download().also { downloads[key] = it }
        val waiter = { result: JSONObject? -> request?.let(cancellations::remove); done(result) }
        download.waiters.add(waiter)
        if (request != null) cancellations[request] = {
            download.waiters.remove(waiter)
            if (download.waiters.isEmpty()) settle(key, error("AbortError"))
            waiter(error("AbortError"))
        }
        return if (current == null) download else null
    }

    override fun cancel(request: Int) { cancellations.remove(request)?.invoke() }

    private fun settle(key: String, error: JSONObject?) {
        val d = downloads.remove(key) ?: return
        d.client?.close()
        if (d.loading && !ended) context.changed(PAIR_TOPIC)
        d.waiters.forEach { it(error) }
    }

    private fun translate(args: JSONObject, request: Int?, reply: (JSONObject) -> Unit) {
        val h = (args.opt("translator") as? String)?.let { translators[it] } ?: return reply(error("InvalidStateError"))
        val input = args.opt("input") as? String
        val entry = if (context.agent && !input.isNullOrEmpty()) translations.removeFirstOrNull() else null
        onHandle(h, request, reply) { answer, active ->
            if (input == null) return@onHandle answer(error("TypeError"))
            if (input.isEmpty()) return@onHandle answer(JSONObject().put("text", ""))
            val p = h.pair!!
            if (context.agent) {
                val failed = entry?.opt("error") as? String
                if (failed != null) {
                    val reason = entry.opt("reason") as? String
                    if (reason == "not-installed") {
                        h.state = "dead"
                        removed += p.key
                        context.changed(PAIR_TOPIC)
                    }
                    return@onHandle answer(error(failed, reason))
                }
                if (h.state == "dead") h.state = "rebuilt"
                return@onHandle answer(JSONObject().put("text", (entry?.opt("text") as? String) ?: "[${p.target}] $input"))
            }
            fun notInstalled(): JSONObject {
                h.closeClient()
                context.changed(PAIR_TOPIC)
                return error("InvalidStateError", "not-installed")
            }
            // A long input goes in paragraphs (D9), as on Apple.
            val parts = if (input.length > 1000 && "\n\n" in input && BreakIterator.getCharacterInstance().run {
                setText(input)
                next(1001) != BreakIterator.DONE
            }) input.split("\n\n") else listOf(input)
            val out = ArrayList<String>()
            fun step(index: Int) {
                if (!active()) return answer(error("AbortError"))
                var i = index
                while (i < parts.size && parts[i].isEmpty()) { out += ""; i++ }
                if (i == parts.size) return answer(JSONObject().put("text", out.joinToString("\n\n")))
                sdkTask {
                    val client = h.client ?: models.client(p.s, p.t).also { h.client = it }
                    client.translate(parts[i])
                }.addOnSuccessListener { text -> out += text; step(i + 1) }
                    .addOnCanceledListener { answer(error("AbortError")) }
                    .addOnFailureListener { e ->
                        downloaded { installed ->
                            answer(if (active() && installed != null && !p.installed(installed)) notInstalled()
                                else error("OperationError", e.message))
                        }
                    }
            }
            downloaded { installed ->
                if (!active()) return@downloaded answer(error("AbortError"))
                if (installed == null) return@downloaded answer(error("OperationError", "cannot read downloaded models"))
                if (!p.installed(installed)) return@downloaded answer(notInstalled())
                if (!watchingMemory) {
                    context.application.registerComponentCallbacks(memory)
                    watchingMemory = true
                }
                step(0)
            }
        }
    }

    private fun destroy(map: HashMap<String, Handle>, id: Any?, reply: (JSONObject) -> Unit) {
        val h = (id as? String)?.let { map.remove(it) } ?: return reply(error("InvalidStateError"))
        drop(h, "destroyed by the app")
        reply(JSONObject().put("id", h.id))
    }

    private fun drop(h: Handle, why: String) {
        h.dead = true
        h.abort?.invoke()
        h.closeClient()
        h.identifier?.close()
        h.identifier = null
        say("${h.id}: $why")
    }

    // ---------------------------------------------------------------- detection

    private fun createDetector(args: JSONObject, reply: (JSONObject) -> Unit) {
        val a = detectorAvailability(args)
        if (a.optString("availability") != "available") return reply(error("NotSupportedError", a.optString("reason")))
        serial += 1
        val list = args.optJSONArray("expectedInputLanguages")?.let { tags ->
            JSONArray((0 until tags.length()).map { Languages.canonical(tags.getString(it)) })
        }
        val h = Handle("language-detector-$serial", expected = list)
        detectors[h.id] = h
        say("${h.id}: made")
        val info = JSONObject().put("id", h.id)
        if (list != null) info.put("expectedInputLanguages", list)
        reply(info)
    }

    private fun detect(args: JSONObject, request: Int?, reply: (JSONObject) -> Unit) {
        val h = (args.opt("detector") as? String)?.let { detectors[it] } ?: return reply(error("InvalidStateError"))
        val input = args.opt("input") as? String
        val entry = if (context.agent && !input.isNullOrBlank()) detections.removeFirstOrNull() else null
        onHandle(h, request, reply) { answer, _ ->
            if (input == null) return@onHandle answer(error("TypeError"))
            val unknown = { JSONObject().put("results", JSONArray().put(JSONObject().put("detectedLanguage", "und").put("confidence", 1.0))) }
            if (input.isBlank()) return@onHandle answer(unknown())
            if (context.agent) {
                entry?.let { return@onHandle answer(it) }
                val language = Languages.ruled(input) ?: return@onHandle answer(unknown())
                return@onHandle answer(JSONObject().put("results", JSONArray()
                    .put(JSONObject().put("detectedLanguage", language).put("confidence", 0.9))
                    .put(JSONObject().put("detectedLanguage", "und").put("confidence", 0.1))))
            }
            // ML Kit accepts no language hints.
            sdkTask {
                val client = h.identifier ?: LanguageIdentification.getClient(
                    LanguageIdentificationOptions.Builder().setConfidenceThreshold(0.01f).build()).also { h.identifier = it }
                client.identifyPossibleLanguages(input)
            }.addOnCanceledListener { answer(error("AbortError")) }
                .addOnSuccessListener { found ->
                    val hypotheses = found.associate { it.languageTag to it.confidence.toDouble() }
                    answer(JSONObject().put("results", JSONArray(Languages.results(hypotheses).map { (l, c) ->
                        JSONObject().put("detectedLanguage", l).put("confidence", c)
                    })))
                }
                .addOnFailureListener { e -> answer(error("OperationError", e.message)) }
        }
    }

    // ---------------------------------------------------------------- the session

    override fun preferred(values: JSONObject) {
        if (!context.agent) return
        pairOverlay = values.optJSONObject("translatorAvailability") ?: JSONObject()
        detectorOverlay = values.optJSONObject("languageDetectorAvailability") ?: JSONObject()
        val t = values.opt("translations")?.toString() ?: ""
        val d = values.opt("detections")?.toString() ?: ""
        if (t != seenTranslations) {
            seenTranslations = t
            translations.clear()
            values.optJSONArray("translations")?.let { a -> for (i in 0 until a.length()) a.optJSONObject(i)?.let(translations::add) }
            say("script: ${translations.size} translations")
        }
        if (d != seenDetections) {
            seenDetections = d
            detections.clear()
            values.optJSONArray("detections")?.let { a -> for (i in 0 until a.length()) a.optJSONObject(i)?.let(detections::add) }
            say("script: ${detections.size} detections")
        }
        disclose()
        // The drive completes or fails an agent's download by the availability it lays over (§9).
        when (pairOverlay.optString("availability")) {
            "available" -> for (key in downloads.keys.toList()) { removed -= key; say("$key: download finished (agent)"); settle(key, null) }
            "unavailable" -> for (key in downloads.keys.toList()) settle(key, error("NotSupportedError"))
        }
    }

    override fun dispose(ids: List<String>) {
        for (id in ids) (translators.remove(id) ?: detectors.remove(id))?.let { drop(it, "its reply was dropped") }
        disclose()
    }

    /** Back in the foreground, a model may have arrived or gone (D8). */
    override fun foreground(shown: Boolean) {
        if (shown && !context.agent) context.changed(PAIR_TOPIC)
    }

    override fun destroy(done: () -> Unit) {
        ended = true
        for (key in downloads.keys.toList()) settle(key, error("AbortError"))
        if (watchingMemory) context.application.unregisterComponentCallbacks(memory)
        for (h in translators.values + detectors.values) drop(h, "the session ended")
        translators.clear()
        detectors.clear()
        done()
    }
}

/** Client construction and the SDK call can fail before ML Kit returns its task. */
private inline fun <T> sdkTask(start: () -> Task<T>): Task<T> =
    try { start() } catch (e: Exception) { Tasks.forException(e) }

internal open class TranslationModels {
    open fun hasNetwork(context: Context) = context.getSystemService(ConnectivityManager::class.java).activeNetwork != null

    open fun canDownload(context: Context) =
        GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(context) == ConnectionResult.SUCCESS

    open fun downloaded(): Task<Set<String>> =
        RemoteModelManager.getInstance().getDownloadedModels(TranslateRemoteModel::class.java)
            .continueWith { task -> task.result.map { it.language }.toSet() }

    open fun client(source: String, target: String): Translator {
        val builder = TranslatorOptions.Builder()
        TranslationModule.optionsHatch?.invoke(builder)
        return com.google.mlkit.nl.translate.Translation.getClient(builder.setSourceLanguage(source).setTargetLanguage(target).build())
    }
}
