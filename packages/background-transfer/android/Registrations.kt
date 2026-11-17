package exact.backgroundtransfer

import exact.backgroundtransfer.Lifecycle.Delete
import exact.backgroundtransfer.Lifecycle.Durable
import exact.backgroundtransfer.Lifecycle.Intent
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Phase
import exact.backgroundtransfer.Lifecycle.Put
import exact.backgroundtransfer.Lifecycle.Record
import exact.backgroundtransfer.Lifecycle.Result
import exact.backgroundtransfer.Lifecycle.Stopping
import exact.backgroundtransfer.Lifecycle.Write
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.nio.channels.FileChannel
import java.nio.file.Files
import java.nio.file.LinkOption.NOFOLLOW_LINKS
import java.nio.file.StandardCopyOption.ATOMIC_MOVE
import java.nio.file.StandardCopyOption.REPLACE_EXISTING
import java.nio.file.StandardOpenOption.READ
import java.nio.file.StandardOpenOption.WRITE
import java.util.UUID

/**
 * What the lifecycle keeps on disk (LLP 1133.017 D10): one directory a registration under its
 * namespace's root, and the app's force-stop mark beside the roots. [write] performs one of
 * [Lifecycle]'s writes. The other methods are the file work of an operation and decide nothing:
 * HTTP, secret storage and app-file placement belong to their callers.
 *
 * A method that reads or writes a record holds this object's monitor, which is the lock a step
 * and its writes are taken under. A copy, an exchange and a placement run outside it.
 */
internal open class Registrations(root: File) {
    // Android owns the parent; /data/user/0 may itself be a platform alias.
    private val boundary = requireNotNull(root.absoluteFile.parentFile).canonicalFile
    private val root = File(boundary, root.name)

    companion object {
        /**
         * The directory a registration's responses with no `saveTo` are published in (LLP 1133.017
         * D3): `app:/data/transfers/<id>`. The id is one path segment there, with every byte but an
         * ASCII letter, a digit, `-` and `_` percent-encoded, as Apple's module writes it.
         */
        fun kept(id: String): String = "app:/data/transfers/" + id.toByteArray().joinToString("") { byte ->
            val c = byte.toInt().toChar()
            if (c in 'a'..'z' || c in 'A'..'Z' || c in '0'..'9' || c == '-' || c == '_') c.toString() else "%%%02X".format(byte)
        }

        /** Only this projection crosses into app answers, wakes and notification intents. */
        fun publicRecord(record: JSONObject): JSONObject {
            val out = JSONObject()
            for (key in listOf("id", "uploadTotal")) out.put(key, record.get(key))
            val result = record.getJSONObject("state").optString("result")
            out.put("uploaded", record.optLong("uploaded")).put("downloadTotal", record.optLong("downloadTotal"))
                .put("downloaded", record.optLong("downloaded") + record.optLong("receiving"))
                .put("result", when (result) { "" -> ""; Result.Success.name -> "success"; else -> "failure" })
                .put("failureReason", when (result) {
                    Result.Aborted.name -> "aborted"
                    Result.BadStatus.name -> "bad-status"
                    Result.FetchError.name -> "fetch-error"
                    Result.DownloadTotalExceeded.name -> "download-total-exceeded"
                    else -> ""
                }).put("apple", JSONObject.NULL)
            val records = JSONArray()
            val kept = record.getJSONArray("records")
            for (i in 0 until kept.length()) {
                val r = kept.getJSONObject(i)
                val item = JSONObject()
                for (key in listOf("index", "url", "status", "contentType")) item.put(key, r.get(key))
                item.put("path", r.opt("path") ?: JSONObject.NULL).put("bodyLost", r.optBoolean("bodyLost"))
                records.put(item)
            }
            return out.put("records", records)
        }
    }

    /** An admitted request: its URL carries no credentials. */
    data class Request(val url: String, val method: String, val saveTo: String? = null, val bodyFrom: String? = null)

    /** What a start gives its registration beside the lifecycle's record. [roots] are the session's `data`, `cache` and `temporary`. */
    class Admission(val title: String, val source: String, val origin: String, val requests: List<Request>,
                    val downloadTotal: Long?, val roots: JSONObject)

    /** One attempt's files. [number] is the lifecycle's attempt and [index] its request. */
    data class Attempt(val key: Key, val number: Int, val index: Int, val credentialKey: String, val upload: File?,
                       val response: File, val remaining: Long?)

    /** A finished exchange's response, kept by the write that counts it. */
    class Response(val attempt: Attempt, val status: Int, val contentType: String)

    class BodyMissing : Exception()

    init {
        Files.createDirectories(this.root.toPath())
        regularPath(this.root, directory = true)
    }

    @Synchronized fun durable(): Durable {
        val records = HashMap<Key, Record>()
        for ((namespace, space) in spaces()) for (dir in space.listFiles().orEmpty()) {
            if (!validGeneration(dir.name)) continue
            val record = record(namespace, read(dir))
            records[record.key] = record
        }
        val mark = File(root, "stopping")
        val stopping = if (!Files.exists(mark.toPath(), NOFOLLOW_LINKS)) emptySet() else {
            regularPath(mark)
            JSONArray(mark.readText()).let { names -> (0 until names.length()).mapNotNull { namespace(names.getString(it)) }.toSet() }
        }
        return Durable(records, stopping)
    }

    /**
     * One write of a step. A new record takes its [admission]. A record that counts one more
     * request takes its [response] in the same write, so a kept response and its count are never apart.
     */
    @Synchronized open fun write(write: Write, admission: Admission? = null, response: Response? = null) {
        when (write) {
            is Stopping -> save(root, "stopping", JSONArray(write.namespaces.map { directory(it).name }).toString())
            is Delete -> {
                // The record leaves in one rename. A death after it leaves a hidden directory, which [sweep] removes.
                val space = directory(write.key.namespace)
                val leaving = File(space, ".delete-${write.key.generation}")
                Files.move(directory(write.key).toPath(), leaving.toPath(), ATOMIC_MOVE)
                sync(space)
                remove(leaving)
            }
            is Put -> {
                val dir = directory(write.record.key)
                if (!Files.exists(dir.toPath(), NOFOLLOW_LINKS)) return create(write.record, requireNotNull(admission))
                val record = read(dir)
                if (write.record.done == record.getJSONArray("records").length() + 1) keep(dir, record, requireNotNull(response))
                check(write.record.done == record.getJSONArray("records").length())
                save(dir, "record", record.put("state", state(write.record)).toString())
            }
        }
    }

    @Synchronized fun get(key: Key): JSONObject? = directory(key).takeIf { Files.exists(it.toPath(), NOFOLLOW_LINKS) }?.let(::read)

    /** What a dead process left: a start it never recorded, a snapshot it never committed, a deleted registration it had not removed. */
    @Synchronized fun sweep() {
        for ((_, space) in spaces()) space.listFiles().orEmpty().filter { it.name.startsWith(".") }.forEach(::remove)
    }

    /**
     * Copy the upload bodies beside the record: staged outside the monitor and committed inside it.
     * A body a dead process committed is kept. False when [cancelled] or the record is gone.
     */
    fun snapshot(key: Key, cancelled: () -> Boolean, copy: (JSONObject, String, File) -> Unit): Boolean {
        val record = get(key) ?: return false
        val dir = directory(key)
        val staging = File(dir.parentFile, ".snapshot-${key.generation}")
        try {
            remove(staging)
            Files.createDirectory(staging.toPath())
            val requests = record.getJSONArray("requests")
            var total = 0L
            for (i in 0 until requests.length()) {
                val request = requests.getJSONObject(i)
                if (!request.getBoolean("body")) continue
                if (cancelled()) return false
                val kept = File(dir, "$i.upload")
                val file = if (Files.exists(kept.toPath(), NOFOLLOW_LINKS)) kept else File(staging, "$i.upload").also {
                    Files.createFile(it.toPath())
                    copy(record, request.getString("bodyFrom"), it)
                }
                regularPath(file)
                request.put("uploadTotal", file.length())
                total = Math.addExact(total, file.length())
            }
            synchronized(this) {
                val current = get(key)
                if (current == null || cancelled()) return false
                staging.listFiles().orEmpty().forEach { Files.move(it.toPath(), File(dir, it.name).toPath(), ATOMIC_MOVE) }
                save(dir, "record", current.put("requests", requests).put("uploadTotal", total).toString())
            }
            return true
        } finally { remove(staging) }
    }

    /** The files of attempt [number] at request [index]. A new attempt never resumes an incomplete response. */
    @Synchronized fun begin(key: Key, number: Int, index: Int): Attempt {
        val dir = directory(key)
        val record = read(dir)
        dir.listFiles().orEmpty().filter { it.name.endsWith(".part") }.forEach(::remove)
        val response = File(dir, "$index.$number.part")
        Files.createFile(response.toPath())
        val request = record.getJSONArray("requests").getJSONObject(index)
        val remaining = if (record.isNull("downloadTotal")) null else record.getLong("downloadTotal") - record.getLong("downloaded")
        return Attempt(key, number, index, request.getString("credentialKey"),
            if (request.getBoolean("body")) File(dir, "$index.upload").also { regularPath(it) } else null, response, remaining)
    }

    /** The bytes of the attempt that holds the lease; a stale attempt's are dropped. The record, when it was written. */
    @Synchronized fun progress(attempt: Attempt, uploaded: Long, downloaded: Long): JSONObject? {
        val record = get(attempt.key) ?: return null
        val state = record.getJSONObject("state")
        if (state.getString("phase") != Phase.Sending.name || state.getInt("attempt") != attempt.number) return null
        record.put("uploaded", Math.addExact(sent(record, attempt.index), uploaded)).put("receiving", downloaded)
        save(directory(attempt.key), "record", record.toString())
        return record
    }

    /** Whether the attempt's response takes the registration past its `downloadTotal`. */
    @Synchronized fun exceeds(attempt: Attempt): Boolean {
        val record = get(attempt.key) ?: return false
        return !record.isNull("downloadTotal") && record.getLong("downloaded") + attempt.response.length() > record.getLong("downloadTotal")
    }

    @Synchronized fun discard(attempt: Attempt) { Files.deleteIfExists(attempt.response.toPath()) }

    /**
     * Put each kept response where the app reads it, through [publish], outside the monitor. A
     * rename may have committed before the record did: `placing` says so, and [publish] recognises
     * either place. A failure leaves the response unplaced for the next placement.
     */
    fun place(key: Key, publish: (JSONObject, Int, File, String, Boolean) -> Unit) {
        val dir = directory(key)
        val count = get(key)?.getJSONArray("records")?.length() ?: return
        for (i in 0 until count) {
            val record = synchronized(this) {
                val record = get(key) ?: return
                val entry = record.getJSONArray("records").getJSONObject(i)
                if (entry.optBoolean("placed")) null else {
                    entry.put("placing", true)
                    save(dir, "record", record.toString())
                    record
                }
            } ?: continue
            val request = record.getJSONArray("requests").getJSONObject(i)
            val kept = request.isNull("saveTo")
            val path = if (kept) "${kept(key.id)}/$i" else request.getString("saveTo")
            val lost = try {
                publish(record, i, File(dir, record.getJSONArray("records").getJSONObject(i).getString("file")), path, kept)
                false
            } catch (_: BodyMissing) { true }
            synchronized(this) {
                val current = get(key) ?: return
                val entry = current.getJSONArray("records").getJSONObject(i)
                entry.remove("placing")
                if (lost) entry.put("bodyLost", true) else entry.put("path", path)
                entry.put("placed", true)
                save(dir, "record", current.toString())
            }
        }
    }

    /**
     * Remove what a retired registration kept: its placed bodies through [cleanup], its credentials
     * through [forget], then its files. The record stays for the [Delete] that follows, so a
     * failure here is tried again at the next recovery.
     */
    fun clean(key: Key, cleanup: (JSONObject) -> Unit, forget: (JSONObject, String) -> Unit) {
        val record = get(key) ?: return
        cleanup(record)
        val requests = record.getJSONArray("requests")
        for (i in 0 until requests.length()) forget(record, requests.getJSONObject(i).getString("credentialKey"))
        synchronized(this) {
            val dir = directory(key)
            remove(File(dir.parentFile, ".snapshot-${key.generation}"))
            dir.listFiles().orEmpty().filter { it.name != "record" }.forEach(::remove)
        }
    }

    private fun create(r: Record, admission: Admission) {
        val space = directory(r.key.namespace)
        Files.createDirectories(space.toPath())
        regularPath(space, directory = true)
        val entries = JSONArray()
        for ((index, request) in admission.requests.withIndex()) entries.put(JSONObject().put("url", request.url)
            .put("method", request.method).put("credentialKey", "exact.transfer.${r.key.generation}.$index").put("uploadTotal", 0L)
            .put("saveTo", request.saveTo ?: JSONObject.NULL).put("bodyFrom", request.bodyFrom ?: JSONObject.NULL)
            .put("body", request.bodyFrom != null))
        check(r.requests == entries.length() && r.done == 0)
        val record = JSONObject().put("id", r.key.id).put("generation", r.key.generation).put("title", admission.title)
            .put("source", admission.source).put("origin", admission.origin).put("requests", entries)
            .put("uploadTotal", 0L).put("downloadTotal", admission.downloadTotal ?: JSONObject.NULL)
            .put("uploaded", 0L).put("downloaded", 0L).put("records", JSONArray()).put("state", state(r))
        for (name in listOf("data", "cache", "temporary")) record.put(name, admission.roots.getString(name))
        val staging = File(space, ".start-${r.key.generation}")
        remove(staging)
        Files.createDirectory(staging.toPath())
        save(staging, "record", record.toString())
        Files.move(staging.toPath(), directory(r.key).toPath(), ATOMIC_MOVE)
        sync(space)
    }

    /** Only private response custody is handled here; `saveTo` needs the host's admitted file operation. */
    private fun keep(dir: File, record: JSONObject, response: Response) {
        val attempt = response.attempt
        val records = record.getJSONArray("records")
        require(directory(attempt.key) == dir && attempt.index == records.length())
        require(attempt.response == File(dir, "${attempt.index}.${attempt.number}.part"))
        regularPath(attempt.response)
        val count = attempt.response.length()
        val file = File(dir, "${attempt.index}.response")
        FileChannel.open(attempt.response.toPath(), WRITE, NOFOLLOW_LINKS).use { it.force(true) }
        Files.move(attempt.response.toPath(), file.toPath(), ATOMIC_MOVE, REPLACE_EXISTING)
        val requests = record.getJSONArray("requests")
        records.put(JSONObject().put("index", attempt.index).put("url", requests.getJSONObject(attempt.index).getString("url"))
            .put("status", response.status).put("contentType", response.contentType).put("file", file.name).put("downloaded", count))
        record.put("downloaded", Math.addExact(record.getLong("downloaded"), count)).put("receiving", 0L)
            .put("uploaded", sent(record, attempt.index + 1))
    }

    /** The upload bytes of the requests before [index]. */
    private fun sent(record: JSONObject, index: Int): Long {
        val requests = record.getJSONArray("requests")
        return (0 until index).sumOf { requests.getJSONObject(it).getLong("uploadTotal") }
    }

    private fun record(namespace: Namespace, json: JSONObject): Record {
        val s = json.getJSONObject("state")
        return Record(Key(namespace, json.getString("id"), json.getString("generation")), Intent.valueOf(s.getString("intent")),
            json.getJSONArray("requests").length(), Phase.valueOf(s.getString("phase")),
            s.getString("job").takeIf { it.isNotEmpty() }?.let(Job::valueOf), s.getBoolean("parked"), s.getInt("attempt"),
            json.getJSONArray("records").length(), s.getInt("stops"), s.getInt("retries"),
            s.getString("result").takeIf { it.isNotEmpty() }?.let(Result::valueOf), s.getBoolean("placed"), s.getInt("wakes"),
            s.getInt("delivery"))
    }

    /** The lifecycle's fields. `requests` and `done` are the lengths of the record's own lists. */
    private fun state(r: Record) = JSONObject().put("intent", r.intent.name).put("phase", r.phase.name).put("job", r.job?.name ?: "")
        .put("parked", r.parked).put("attempt", r.attempt).put("stops", r.stops).put("retries", r.retries)
        .put("result", r.result?.name ?: "").put("placed", r.placed).put("wakes", r.wakes).put("delivery", r.delivery)

    private fun spaces(): List<Pair<Namespace, File>> = root.listFiles().orEmpty().mapNotNull { dir ->
        namespace(dir.name)?.let { regularPath(dir, directory = true); it to dir }
    }
    private fun namespace(name: String): Namespace? = when {
        name.startsWith("app.") -> Namespace(name.removePrefix("app."))
        name.startsWith("agent.") -> Namespace(name.removePrefix("agent."), agent = true)
        else -> null
    }
    private fun directory(namespace: Namespace): File {
        require(Regex("[A-Za-z0-9_-]{1,64}").matches(namespace.name))
        return File(root, (if (namespace.agent) "agent." else "app.") + namespace.name)
    }
    private fun directory(key: Key): File {
        require(validGeneration(key.generation))
        return File(directory(key.namespace), key.generation)
    }
    private fun read(dir: File): JSONObject {
        regularPath(dir, directory = true)
        regularPath(File(dir, "record"))
        return JSONObject(File(dir, "record").readText())
    }
    private fun save(dir: File, name: String, text: String) {
        regularPath(dir, directory = true)
        val next = File(dir, "$name.next")
        Files.deleteIfExists(next.toPath())
        Files.createFile(next.toPath())
        FileOutputStream(next).use { it.write(text.toByteArray(Charsets.UTF_8)); it.fd.sync() }
        Files.move(next.toPath(), File(dir, name).toPath(), ATOMIC_MOVE, REPLACE_EXISTING)
        sync(dir)
    }
    private fun sync(dir: File) { FileChannel.open(dir.toPath(), READ).use { it.force(true) } }
    protected open fun remove(file: File) {
        if (Files.isDirectory(file.toPath(), NOFOLLOW_LINKS)) file.listFiles().orEmpty().forEach { remove(it) }
        Files.deleteIfExists(file.toPath())
    }
    private fun regularPath(file: File, directory: Boolean = false) {
        var path: java.nio.file.Path? = file.toPath().toAbsolutePath()
        require(if (directory) Files.isDirectory(path, NOFOLLOW_LINKS) else Files.isRegularFile(path, NOFOLLOW_LINKS))
        require(path!!.startsWith(root.toPath()))
        while (path != boundary.toPath()) {
            require(path != null && !Files.isSymbolicLink(path))
            path = path.parent
        }
    }
    private fun validGeneration(value: String) = runCatching { UUID.fromString(value).toString() == value }.getOrDefault(false)
}
