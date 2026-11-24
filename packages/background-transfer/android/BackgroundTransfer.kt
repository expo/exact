package exact.backgroundtransfer

import android.app.ActivityManager
import android.content.Context
import android.os.Handler
import android.os.Looper
import dev.exact.canvas.Cause
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import exact.backgroundtransfer.Lifecycle.Abort
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Outcome
import exact.backgroundtransfer.Lifecycle.SessionEnded
import exact.backgroundtransfer.Lifecycle.SessionStarted
import exact.backgroundtransfer.Lifecycle.Start
import exact.backgroundtransfer.Lifecycle.ViewAttached
import exact.backgroundtransfer.Lifecycle.ViewDetached
import org.json.JSONArray
import org.json.JSONObject
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executor
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/**
 * The host's half of a transfer (LLP 1133.017 D10). An ending is raised in the live session of its namespace. With
 * none the host boots a session with no view ([ExtensionContext.session]), whose module then raises it. The wake's
 * outcome is the host's: `completed` after the handler and the store's writes, so the lifecycle holds the run until
 * then. Where no session can be had the wake is reported expired and the lifecycle delivers the ending again, at
 * last at a launch.
 */
internal object Sessions : Host {
    private val incarnations = AtomicInteger()
    private val live = ConcurrentHashMap<Namespace, BackgroundTransferModule>()
    /** The application, from the process's first entrant: what a session with no view is booted for. */
    @Volatile var application: Context? = null
    /** The wakes raised with no session, each with how its outcome is told, until it is. */
    private val owed = ConcurrentHashMap<Pair<Key, Int>, (Outcome) -> Unit>()

    /** Where every session's events are fed, one at a time: a session's end is fed before the next one's start. */
    val events: Executor = ThreadPoolExecutor(1, 1, 30, TimeUnit.SECONDS, LinkedBlockingQueue()) { work -> Thread(work, "exact-transfer-calls") }
        .apply { allowCoreThreadTimeOut(true) }

    /** A session's number, never given twice in a process. */
    fun next() = incarnations.incrementAndGet()
    fun started(namespace: Namespace, module: BackgroundTransferModule) { live[namespace] = module }
    fun ended(namespace: Namespace, module: BackgroundTransferModule) { live.remove(namespace, module) }

    override fun raise(key: Key, delivery: Int, ended: (Outcome) -> Unit) {
        live[key.namespace]?.let { return it.raise(key, delivery, ended) }
        val app = application
        // An agent drive's store is named by its launch: no session can be booted for it.
        if (app == null || key.namespace.agent) return ended(Outcome.Expired)
        val wake = key to delivery
        // Told once: by the wake's outcome, or sooner by the stop of the run that holds it.
        val told = { outcome: Outcome -> if (owed.remove(wake, ended)) ended(outcome) }
        owed[wake] = ended
        ExtensionContext.session(app) {
            // The module reads the record, so off the main thread.
            events.execute { live[key.namespace]?.takeIf { owed.containsKey(wake) }?.raise(key, delivery, told) ?: told(Outcome.Expired) }
        }
    }
    override fun expire(key: Key, delivery: Int) {
        val waiting = owed.remove(key to delivery)
        live[key.namespace]?.expire(key, delivery)
        waiting?.invoke(Outcome.Expired)
    }
    override fun answer(key: Key, registered: Boolean, error: String?) { live[key.namespace]?.answer(key, registered, error) }
    override fun changed(namespace: Namespace) { live[namespace]?.changed() }
}

/**
 * `@exact/background-transfer` on Android (LLP 1133.017 D3, D4, D10): the package's calls and its
 * session's life as events of [Transfers]. Calls are heard on the main thread and fed from
 * [events], one at a time in order, so no disk write is made on the main thread.
 */
class BackgroundTransferModule internal constructor(context: ExtensionContext, made: () -> Transfers,
    private val exchange: (JSONObject) -> JSONObject, private val events: Executor) : ExactExtension(context) {
    constructor(context: ExtensionContext) : this(context, { Jobs.transfers(context.application) }, NativeTransfer::exchange, Sessions.events)

    private class Refused(val code: String) : Exception()
    private class Wake(val id: String, val ended: (Outcome) -> Unit)

    private val transfers by lazy(made)
    private val main = Handler(Looper.getMainLooper())
    private val incarnation = Sessions.next()
    private val roots = JSONObject().put("data", context.data.path).put("cache", context.cache.path)
        .put("temporary", context.temporary.path)
    /**
     * The session's storage namespace: the app's, or a drive's, named by the drive's store so that a later launch
     * of the drive finds its registrations. A drive that binds this package (`--bind`) is still a drive's: the
     * registrations, their jobs and their credentials are outside the session's directories, in the app's own files.
     */
    private val namespace = if (!context.scratch) Namespace("app") else Namespace(
        MessageDigest.getInstance("SHA-256").digest(context.data.path.toByteArray()).take(8).joinToString("") { "%02x".format(it) }, agent = true)
    private val fresh = namespace.agent && android.system.Os.getenv("EXACT_AGENT_STORAGE_FRESH") == "1"
    private val replies = ConcurrentHashMap<Key, (JSONObject) -> Unit>()
    private val wakes = HashMap<Pair<Key, Int>, Wake>()
    private var attached = false
    private var destroyed = false
    /** Whether the lifecycle was last told the session has a view. Read and written on [events]. */
    private var shown = false

    init {
        Sessions.started(namespace, this)
        Jobs.visible = { context.visible }
        events.execute { runCatching { entered() } }
        if (context.visible && context.foreground) attach()
    }

    /**
     * On [events], before each event of this session: the namespace's recovery, then the session,
     * which the lifecycle refuses while it knows one. Both are owed at the session's start, and
     * again after a failed write, when the lifecycle starts over from the disk.
     */
    private fun entered() {
        if (fresh) transfers.emptyScratch(namespace)
        transfers.enter(namespace)
        if (transfers.feed(SessionStarted(namespace, incarnation)).accepted && shown)
            transfers.feed(ViewAttached(namespace, incarnation, transfers.conditions))
    }
    private fun tell(event: Lifecycle.Event): Lifecycle.Step { entered(); return transfers.feed(event) }

    /** An activity shows the session: a launch with a view. Told once for each time it comes to show. */
    private fun attach() {
        if (attached || destroyed) return
        attached = true
        events.execute { runCatching { tell(ViewAttached(namespace, incarnation, transfers.conditions)); shown = true } }
    }
    private fun detach() {
        if (!attached) return
        attached = false
        events.execute { runCatching { shown = false; tell(ViewDetached(namespace, incarnation)) } }
    }

    private fun port(request: JSONObject): JSONObject {
        val answer = exchange(request)
        if (answer.has("error")) throw Refused(answer.getString("error"))
        return answer
    }
    private fun base(op: String, authority: String, origin: String): JSONObject = JSONObject(roots.toString())
        .put("op", op).put("authority", authority).put("origin", origin)
    private val denied get() = context.application.getSystemService(ActivityManager::class.java).isBackgroundRestricted
    private fun failure(e: Exception) = JSONObject().put("error", when (e) {
        is Refused -> e.code
        is IllegalArgumentException, is org.json.JSONException -> "TypeError"
        else -> "UnknownError"
    })

    internal fun changed() {
        main.post {
            if (destroyed) return@post
            context.changed("background-transfer.backgroundFetchRegistration")
            context.changed("background-transfer.backgroundFetchIds")
        }
    }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) = later(name, args, null, Cause.none, reply)

    override fun later(name: String, args: JSONObject, preferred: Any?, cause: Cause, reply: (JSONObject) -> Unit) {
        if (destroyed) return
        val authority = cause.authority
        val origin = cause.origin
        val person = cause.userActivation
        if (name == "backgroundFetchStatus") {
            reply(JSONObject().put("available", true).put("state", if (denied) "denied" else "granted"))
            return
        }
        if (name == "backgroundFetch" && denied) { reply(JSONObject().put("error", "NotAllowedError")); return }
        events.execute {
            val answer = try {
                entered()
                val key = transfers.find(namespace, args.optString("id"))
                when (name) {
                    // A start is answered by the lifecycle, once its upload is copied.
                    "backgroundFetch" -> { start(args, authority, origin, person, reply); return@execute }
                    "backgroundFetchRegistration" -> JSONObject().put("exact:answer", key?.let(transfers::read) ?: JSONObject.NULL)
                    "backgroundFetchIds" -> JSONObject().put("exact:answer", JSONArray(transfers.ids(namespace)))
                    "abortBackgroundFetch" -> JSONObject().put("exact:answer", key != null && tell(Abort(key)).accepted)
                    // Answered when the registration's files and its record are gone: its id can be started again from
                    // then. A removal that failed is an error, and the call made again removes what it left.
                    "retireBackgroundFetch" -> {
                        val retiring = key ?: transfers.retired(namespace, args.optString("id"))
                        if (retiring != null && transfers.retire(retiring) { removed ->
                                main.post { if (!destroyed) reply(if (removed) JSONObject() else JSONObject().put("error", "UnknownError")) }
                            }) return@execute
                        JSONObject()
                    }
                    else -> JSONObject().put("error", "NotSupportedError")
                }
            } catch (e: Exception) { failure(e) }
            main.post { if (!destroyed) reply(answer) }
        }
    }

    /** [person]: the call came inside a person's activation window ([Cause.userActivation]). */
    private fun start(args: JSONObject, authority: String, origin: String, person: Boolean, reply: (JSONObject) -> Unit) {
        val id = args.getString("id")
        val list = args.getJSONArray("requests")
        require(id.isNotEmpty() && id.toByteArray().size <= 64 && origin in setOf("app", "tool", "wake"))
        require(list.length() in 1..256 && args.optString("title").toByteArray().size <= 1024)
        val total = if (args.isNull("downloadTotal")) null else args.getDouble("downloadTotal").let {
            require(it.isFinite() && it >= 0 && it <= 9_007_199_254_740_991.0 && it % 1.0 == 0.0); it.toLong()
        }
        val requests = (0 until list.length()).map { i ->
            val r = list.getJSONObject(i)
            val public = port(base("validate", authority, origin).put("url", r.getString("url"))).getString("url")
            val save = r.optString("saveTo").takeIf { it.isNotEmpty() && it != "null" }
            if (save != null) port(base("checkSave", authority, origin).put("path", save))
            val from = if (r.isNull("bodyFrom")) null else r.getString("bodyFrom")
            val form = !r.isNull("form")
            require(!form || (from == null && r.getJSONArray("form").length() <= 256))
            val method = if (r.isNull("method")) {
                if (from != null || form) "POST" else "GET"
            } else r.getString("method").uppercase(java.util.Locale.ROOT)
            Registrations.Request(public, method, saveTo = save, bodyFrom = from, form = form)
        }
        val key = Key(namespace, id, UUID.randomUUID().toString())
        val conditions = transfers.conditions
        transfers.admit(key, Registrations.Admission(if (args.isNull("title")) id else args.getString("title"), authority, origin,
            requests, total, roots), list)
        replies[key] = reply
        val intent = Lifecycle.intent(person, conditions)
        if (!transfers.feed(Start(key, intent, requests.size, conditions)).accepted) {
            replies.remove(key)
            // The id has a registration the app has not retired.
            throw if (transfers.find(namespace, id) != null) IllegalArgumentException() else Refused("InvalidStateError")
        }
    }

    internal fun answer(key: Key, registered: Boolean, error: String?) {
        val reply = replies.remove(key) ?: return
        val answer = (if (registered) transfers.read(key) else null) ?: JSONObject().put("error", error ?: "UnknownError")
        main.post { if (!destroyed) reply(answer) }
    }

    internal fun raise(key: Key, delivery: Int, ended: (Outcome) -> Unit) {
        val record = transfers.read(key)
        main.post {
            // The session ended first.
            if (destroyed || record == null) { events.execute { ended(Outcome.Superseded) }; return@post }
            val event = when {
                record.getString("result") == "success" -> "backgroundfetchsuccess"
                record.getString("failureReason") == "aborted" -> "backgroundfetchabort"
                else -> "backgroundfetchfail"
            }
            val wake = Wake("transfer.${key.generation}.$delivery", ended)
            wakes[key to delivery] = wake
            context.wake(event, JSONArray().put(record), wake.id) { outcome ->
                if (wakes.remove(key to delivery, wake)) events.execute {
                    ended(when (outcome) { "expired" -> Outcome.Expired; "superseded" -> Outcome.Superseded; else -> Outcome.Completed })
                }
            }
        }
    }

    internal fun expire(key: Key, delivery: Int) {
        main.post { wakes[key to delivery]?.let { context.expireWake(it.id) } }
    }

    override fun foreground(shown: Boolean) { if (shown) attach() else detach() }
    override fun resumed() {
        context.changed("background-transfer.backgroundFetchStatus")
        changed()
        attach()
    }

    /** The session's wakes end superseded, then the session. */
    override fun destroy(done: () -> Unit) {
        destroyed = true
        Sessions.ended(namespace, this)
        val waking = wakes.values.toList()
        wakes.clear()
        waking.forEach { context.expireWake(it.id) }
        events.execute {
            waking.forEach { it.ended(Outcome.Superseded) }
            runCatching { transfers.feed(SessionEnded(namespace, incarnation)) }
        }
        done()
    }
}
