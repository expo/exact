package exact.backgroundtransfer

import exact.backgroundtransfer.Lifecycle.Conditions
import exact.backgroundtransfer.Lifecycle.Effect
import exact.backgroundtransfer.Lifecycle.Event
import exact.backgroundtransfer.Lifecycle.Intent
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Outcome
import exact.backgroundtransfer.Lifecycle.Start
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executor

/** What [NativeTransfer] answers, for the adapter's tests: a transport, a secret store and the app's files under [files]. */
internal class FakeTransport(private val files: File) {
    val seen = ArrayList<JSONObject>()
    val secrets = HashMap<String, JSONObject>()
    /** The answer to an exchange; the default keeps a small response. */
    var run: (JSONObject) -> JSONObject = { request ->
        File(request.getString("response")).writeText("response")
        JSONObject().put("status", 200).put("contentType", "text/plain").put("downloaded", 8)
    }
    var snapshot: (File) -> Unit = { it.writeText("upload") }
    /** The operations that fail: a locked secret store fails `forget`. */
    var failing = emptySet<String>()
    private var handles = 0L

    fun ops() = synchronized(seen) { seen.map { it.getString("op") } }

    fun exchange(request: JSONObject): JSONObject {
        synchronized(seen) { seen += JSONObject(request.toString()) }
        if (request.getString("op") in failing) return JSONObject().put("error", "InvalidStateError")
        return when (request.getString("op")) {
            "validate" -> JSONObject().put("url", request.getString("url").substringBefore('?').substringBefore('#'))
            "keep" -> if (request.getString("url").contains("denied")) JSONObject().put("error", "NotAllowedError") else {
                synchronized(secrets) { secrets[request.getString("key")] = request }
                JSONObject().put("url", request.getString("url").substringBefore('?')).put("method", request.getString("method"))
            }
            "forget" -> JSONObject().also { synchronized(secrets) { secrets.remove(request.getString("key")) } }
            "snapshot" -> try { snapshot(File(request.getString("file"))); JSONObject() } catch (_: java.io.IOException) { JSONObject().put("error", "NotFoundError") }
            "open" -> JSONObject().put("handle", synchronized(this) { ++handles })
            "run" -> run(request)
            "place" -> JSONObject().also {
                val path = File(files, request.getString("path").removePrefix("app:/"))
                path.parentFile!!.mkdirs()
                File(request.getString("file")).copyTo(path, overwrite = true)
            }
            "removePlaced" -> JSONObject().also { File(files, request.getString("path").removePrefix("app:/")).deleteRecursively() }
            else -> JSONObject()
        }
    }
}

/** A platform that holds every job it is asked for and remembers what it was told. */
internal class FakePlatform : Platform {
    override var conditions = Conditions(api = 35, visible = false)
    var stopped = false
    /** Where the answers to schedules wait for the test to give them; given at once when null. */
    var answers: MutableList<(Boolean) -> Unit>? = null
    val jobs: MutableSet<Pair<Key, Job>> = ConcurrentHashMap.newKeySet()
    val finished = CopyOnWriteArrayList<Pair<Int, Boolean>>()
    val acknowledged = CopyOnWriteArrayList<Namespace>()
    /** The runs asked for foreground execution. */
    val promoted = CopyOnWriteArrayList<Int>()
    override fun stopped() = stopped
    override fun restarted(namespace: Namespace) = false
    override fun held(namespace: Namespace) = jobs.filter { it.second == Job.UserJob }.map { it.first }.toSet()
    override fun schedule(key: Key, job: Job, record: JSONObject, answer: (Boolean) -> Unit) {
        jobs += key to job
        answers?.add(answer) ?: answer(true)
    }
    override fun cancel(key: Key, job: Job?) { jobs.removeAll { it.first == key && (job == null || it.second == job) } }
    /** Told of each run given back. */
    var finish: () -> Unit = {}
    /** A run that is not to run again ends its job. */
    override fun finish(key: Key, run: Int, again: Boolean) {
        finished += run to again
        finish()
        if (!again) jobs.removeAll { it.first == key }
    }
    override fun promote(key: Key, run: Int, record: JSONObject) { promoted += run }
    override fun progress(key: Key, record: JSONObject) {}
    override fun acknowledge(namespace: Namespace) { stopped = false; acknowledged += namespace }
}

/** A host that remembers each wake and each answer, and reports nothing by itself. */
internal class FakeHost : Host {
    val raised = CopyOnWriteArrayList<Triple<Key, Int, (Outcome) -> Unit>>()
    val answers = CopyOnWriteArrayList<Triple<Key, Boolean, String?>>()
    override fun raise(key: Key, delivery: Int, ended: (Outcome) -> Unit) { raised += Triple(key, delivery, ended) }
    override fun expire(key: Key, delivery: Int) {}
    override fun answer(key: Key, registered: Boolean, error: String?) { answers += Triple(key, registered, error) }
    override fun changed(namespace: Namespace) {}
}

internal object Fixture {
    val app = Namespace("app")
    /** Each effect is performed, and each operation run, where it is asked for. */
    val inline = Executor { it.run() }
    fun inline(transfers: () -> Transfers): (Effect, Boolean, () -> Event?) -> Unit = { _, _, body -> body()?.let { transfers().feed(it) } }

    fun key(id: String, namespace: Namespace = app) = Key(namespace, id, UUID.randomUUID().toString())

    /** Wait for what another thread does. */
    fun await(what: String, condition: () -> Boolean) {
        val deadline = System.nanoTime() + 5_000_000_000
        while (!condition()) {
            if (System.nanoTime() > deadline) throw AssertionError("timed out waiting for $what")
            Thread.sleep(2)
        }
    }

    /** Start [key] as a call would: its admission, its requests with their credentials, then the event. */
    fun start(transfers: Transfers, key: Key, root: File, intent: Intent = Intent.Ordinary, upload: Boolean = false,
              saveTo: String? = null, url: String = "https://example.test/a?token=private", total: Long? = null): Boolean {
        val roots = JSONObject().put("data", File(root, "data").path).put("cache", File(root, "cache").path).put("temporary", File(root, "tmp").path)
        val request = Registrations.Request(url.substringBefore('?'), if (upload) "POST" else "GET", saveTo, if (upload) "app:/data/video" else null)
        transfers.admit(key, Registrations.Admission("Upload video", "net.fetch https://example.test", "app", listOf(request), total, roots),
            JSONArray().put(JSONObject().put("url", url).put("headers", JSONArray().put(JSONObject().put("name", "Authorization").put("value", "secret")))))
        return transfers.feed(Start(key, intent, 1, transfers.conditions)).accepted
    }
}
