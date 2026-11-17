package exact.backgroundtransfer

import exact.backgroundtransfer.Lifecycle.Acknowledge
import exact.backgroundtransfer.Lifecycle.Answer
import exact.backgroundtransfer.Lifecycle.Cancel
import exact.backgroundtransfer.Lifecycle.CancelCopy
import exact.backgroundtransfer.Lifecycle.CancelIo
import exact.backgroundtransfer.Lifecycle.Clean
import exact.backgroundtransfer.Lifecycle.CleanFailed
import exact.backgroundtransfer.Lifecycle.Cleaned
import exact.backgroundtransfer.Lifecycle.Conditions
import exact.backgroundtransfer.Lifecycle.Copy
import exact.backgroundtransfer.Lifecycle.CredentialsKept
import exact.backgroundtransfer.Lifecycle.CredentialsRefused
import exact.backgroundtransfer.Lifecycle.Effect
import exact.backgroundtransfer.Lifecycle.Enter
import exact.backgroundtransfer.Lifecycle.Event
import exact.backgroundtransfer.Lifecycle.ExchangeFailed
import exact.backgroundtransfer.Lifecycle.Expire
import exact.backgroundtransfer.Lifecycle.Failure
import exact.backgroundtransfer.Lifecycle.Finish
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.Keep
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Limits
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Outcome
import exact.backgroundtransfer.Lifecycle.Phase
import exact.backgroundtransfer.Lifecycle.Place
import exact.backgroundtransfer.Lifecycle.PlaceFailed
import exact.backgroundtransfer.Lifecycle.Placed
import exact.backgroundtransfer.Lifecycle.Process
import exact.backgroundtransfer.Lifecycle.Promote
import exact.backgroundtransfer.Lifecycle.Raise
import exact.backgroundtransfer.Lifecycle.Response
import exact.backgroundtransfer.Lifecycle.ResponseComplete
import exact.backgroundtransfer.Lifecycle.Retire
import exact.backgroundtransfer.Lifecycle.Schedule
import exact.backgroundtransfer.Lifecycle.ScheduleRefused
import exact.backgroundtransfer.Lifecycle.Scheduled
import exact.backgroundtransfer.Lifecycle.SnapshotCommitted
import exact.backgroundtransfer.Lifecycle.SnapshotFailed
import exact.backgroundtransfer.Lifecycle.Start
import exact.backgroundtransfer.Lifecycle.Step
import exact.backgroundtransfer.Lifecycle.Transmit
import exact.backgroundtransfer.Lifecycle.WakeEnded
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** What the platform holds for the lifecycle, and what an entrant reads of it ([Lifecycle.Enter]). */
internal interface Platform {
    val conditions: Conditions
    /** Evidence of a force-stop since the last [acknowledge]. */
    fun stopped(): Boolean
    /** Whether the device rebooted or the package was updated since [namespace]'s last [acknowledge]. */
    fun restarted(namespace: Namespace): Boolean
    /** The registrations of [namespace] whose user-initiated job the platform holds, pending or running. */
    fun held(namespace: Namespace): Set<Key>
    /** Ask the platform to hold [job]. [answer] is told once whether it does. */
    fun schedule(key: Key, job: Job, record: JSONObject, answer: (Boolean) -> Unit)
    fun cancel(key: Key, job: Job?)
    fun finish(key: Key, run: Int, again: Boolean)
    /** Ask a running worker for foreground execution. */
    fun promote(key: Key, run: Int, record: JSONObject)
    /** The bytes moved so far, for a notification the platform shows. */
    fun progress(key: Key, record: JSONObject)
    fun acknowledge(namespace: Namespace)
}

/**
 * The host's half of a transfer (LLP 1133.017 D10): the session an ending is raised in and the
 * call a start answers. The host reports a wake's outcome as [Lifecycle.WakeEnded] and its
 * sessions as [Lifecycle.SessionStarted] and the events beside it.
 */
internal interface Host {
    /** Raise the ending in its namespace's session. [ended] is told the wake's outcome once. */
    fun raise(key: Key, delivery: Int, ended: (Outcome) -> Unit)
    /** The platform stopped the run that held this wake. */
    fun expire(key: Key, delivery: Int)
    /** [error] is the code of the operation that failed a start, when one did. */
    fun answer(key: Key, registered: Boolean, error: String?)
    fun changed(namespace: Namespace)
}

/**
 * The Android adapter of [Lifecycle] (LLP 1133.017 D10). A platform callback, a call and an
 * operation's report each become one event through [feed]: the step is taken and its writes
 * land, in order, under the store's monitor, and its effects join one queue that [control]
 * performs one at a time off the monitor. Nothing here decides what a transfer does next.
 *
 * An effect that starts a long operation (keeping credentials, the upload copy, an exchange, a
 * placement, a retirement's cleanup) only starts it: [Lifecycle.admits] is read when the queue
 * reaches the effect, and the operation runs on a thread of its own and reports one event. So an
 * abort or a stop never waits behind a copy.
 *
 * A write that fails makes this process, to the lifecycle, the successor of one that died inside
 * the step ([collapse]). The lifecycle's successor finds no operation of its predecessor running,
 * so no namespace is entered until those the failure cut short have returned, and nothing they,
 * a schedule or a wake report afterwards is heard.
 *
 * @param launch runs an operation's body and feeds the report it answers, if any; a thread of this object's own when null.
 * @param pause what a placement waits before it tries again.
 */
internal class Transfers(
    val store: Registrations, private val exchange: (JSONObject) -> JSONObject, private val platform: Platform,
    private val host: Host, private val control: Executor,
    private val launch: ((Effect, Boolean, () -> Event?) -> Unit)? = null, private val limits: Limits = Limits(),
    private val pause: () -> Unit = { Thread.sleep(1000) }) {

    private companion object {
        /** The clock every exchange's progress is read on. */
        val ticks by lazy { Executors.newSingleThreadScheduledExecutor { work -> Thread(work, "exact-transfer-progress").apply { isDaemon = true } } }
    }
    private class Refused(val code: String) : Exception()
    /** A start's requests as the app wrote them. Their credentials go to the secret store and nowhere else. */
    private class Admitted(val admission: Registrations.Admission, val requests: JSONArray)

    /** What a failed write owes outside the monitor: the runs and the calls its lost queue and process state held. */
    private class Owed(val runs: List<Pair<Key, Int>>, val answers: List<Triple<Key, Boolean, String?>>, val retired: List<(Boolean) -> Unit>)

    private var durable = store.durable()
    private var process = Process()
    private val queue = ArrayDeque<Effect>()
    /**
     * The writes that have failed. An operation, a schedule and a wake carry the count they started
     * under, and a report under a later count is not heard: to the lifecycle its process is dead.
     */
    private var failures = 0
    /** The operations started under this count that have not returned, and those started under an earlier one. */
    private var running = 0
    private var orphaned = 0
    /** A write failed, and [durable] has not been read from the disk since. */
    private var unread = false
    private val admitted = HashMap<Key, Admitted>()
    private val refusals = HashMap<Key, String>()
    private val responses = HashMap<Pair<Key, Int>, Registrations.Response>()
    private val exchanges = HashMap<Pair<Key, Int>, TransferIO>()
    private val copies = HashMap<Key, AtomicBoolean>()
    /** What the retirements calls made of each key are told when its cleanup has ended. */
    private val retiring = HashMap<Key, ArrayList<(Boolean) -> Unit>>()
    /** The recoveries not yet told how they ended ([recover]), each with its namespaces. */
    private val recovering = ArrayList<Pair<Set<Namespace>, (Boolean) -> Unit>>()
    private val operations by lazy { Executors.newCachedThreadPool { work -> Thread(work, "exact-transfer-operation") } }

    init { store.sweep() }

    /** The process's volatile state and the effects not yet performed. */
    fun state(): Pair<Process, List<Effect>> = synchronized(store) { process to queue.toList() }

    val conditions: Conditions get() = platform.conditions
    fun namespaces(): Set<Namespace> = synchronized(store) { durable.records.keys.map { it.namespace }.toSet() }
    /** The unretired registration of [id]. */
    fun find(namespace: Namespace, id: String): Key? = synchronized(store) {
        durable.records.values.firstOrNull { it.key.namespace == namespace && it.key.id == id && it.phase != Phase.Retired }?.key
    }
    /** The retired registration of [id] whose record a failed or unfinished cleanup has left. */
    fun retired(namespace: Namespace, id: String): Key? = synchronized(store) {
        durable.records.values.firstOrNull { it.key.namespace == namespace && it.key.id == id && it.phase == Phase.Retired }?.key
    }
    fun ids(namespace: Namespace): List<String> = synchronized(store) {
        durable.records.values.filter { it.key.namespace == namespace && it.phase != Phase.Retired }.map { it.key.id }.sorted()
    }
    fun read(key: Key): JSONObject? = store.get(key)?.let(Registrations::publicRecord)

    /** What the [Start] of [key] will record. [requests] carry the credentials [Keep] stores. */
    fun admit(key: Key, admission: Registrations.Admission, requests: JSONArray) {
        synchronized(store) { admitted[key] = Admitted(admission, requests) }
    }

    /**
     * The recovery every entrant makes before any other event of its namespace. Null when the
     * namespace is entered already, and while an operation that a failed write cut short has
     * not returned: until then nothing is taken, so nothing reuses the files it holds.
     */
    fun enter(namespace: Namespace): Step? {
        if (synchronized(store) { namespace in process.crossed || orphaned > 0 }) return null
        return take(Enter(namespace, platform.stopped(), platform.restarted(namespace), platform.held(namespace), platform.conditions))
    }

    /**
     * Enter every namespace that has a registration. [recorded] is told once how that recovery ended: true when the
     * platform has answered every schedule of those namespaces and each answer is on the disk, so the jobs that
     * replace the lost ones are held and named by their records; false when a namespace cannot be entered yet or a
     * failed write has undone the recovery, which the next entrant makes again.
     */
    fun recover(recorded: (Boolean) -> Unit) {
        val namespaces = namespaces()
        namespaces.forEach { enter(it) }
        val ended = synchronized(store) { recovered(namespaces) ?: run { recovering += namespaces to recorded; return } }
        recorded(ended)
    }

    /** Under the monitor: how the recovery of [namespaces] ended, or null while a schedule of theirs is unanswered. */
    private fun recovered(namespaces: Set<Namespace>): Boolean? = when {
        !process.crossed.containsAll(namespaces) -> false
        process.asking.keys.any { it.key.namespace in namespaces } -> null
        else -> true
    }

    /** One event that is not an [Enter], which [enter] makes. */
    fun feed(event: Event): Step = checkNotNull(take(event))

    /**
     * Retire [key] for a call, or clean again a retired record an earlier cleanup left. [removed] is
     * told once: true when the registration's files are gone and the deletion of its record is on
     * the disk, which frees its id; false when the removal failed, or a failed write cut it short,
     * and the record stands for this call made again or the next recovery. False, with nothing
     * told, when the lifecycle refuses.
     */
    fun retire(key: Key, removed: (Boolean) -> Unit): Boolean = checkNotNull(take(Retire(key), removed = removed)).accepted

    /**
     * One step. [from] is the count of failed writes a report's operation, schedule or wake
     * started under; null is answered for one from before the last failure, and for an [Enter]
     * that [enter] would not make.
     */
    private fun take(event: Event, from: Int? = null, operation: Boolean = false, removed: ((Boolean) -> Unit)? = null): Step? {
        var failure: Exception? = null
        var owed: Owed? = null
        val recoveries = ArrayList<() -> Unit>()
        val step = synchronized(store) {
            if (operation && !returned(checkNotNull(from), event)) return null
            if (from != null && from != failures) return null
            val response = (event as? ResponseComplete)?.let { responses.remove(it.key to it.attempt) }
            if (event is Enter) {
                if (orphaned > 0) return null
                if (unread) { durable = store.durable(); unread = false }
            }
            val step = Lifecycle.step(durable, process, event, limits)
            val admission = (event as? Start)?.let { admitted[it.key]?.admission }
            if (event is Start && !step.accepted) admitted.remove(event.key)
            try {
                for (write in step.writes) store.write(write, admission, response)
                // A response the step did not count is not kept.
                response?.let { store.discard(it.attempt) }
                durable = step.durable
                process = step.process
                queue.addAll(step.effects)
                if (removed != null && event is Retire && step.accepted) retiring.getOrPut(event.key, ::ArrayList) += removed
            } catch (e: Exception) {
                failure = e
                owed = collapse()
            }
            recovering.removeAll { (namespaces, recorded) ->
                val ended = recovered(namespaces) ?: return@removeAll false
                recoveries += { recorded(ended) }
                true
            }
            step
        }
        recoveries.forEach { it() }
        failure?.let { e ->
            owed?.let { lost ->
                lost.runs.forEach { (key, run) -> platform.finish(key, run, again = true) }
                lost.answers.forEach { (key, registered, error) -> host.answer(key, registered, error) }
                lost.retired.forEach { it(false) }
            }
            throw e
        }
        repeat(step.effects.size) { control.execute(::perform) }
        if (step.writes.isNotEmpty()) host.changed(event.namespace)
        if (step.accepted && (event is Cleaned || event is CleanFailed)) cleaned(event.key, event is Cleaned)
        return step
    }

    /**
     * The report of an operation, a schedule or a wake. A failed write has left this process as its
     * successor would be ([collapse]), and the next entrant recovers: there is no caller to tell.
     */
    private fun report(from: Int, event: Event, operation: Boolean = false) { try { take(event, from, operation) } catch (_: Exception) {} }

    /** Under the monitor: an operation that started under [from] has returned with [event]. Whether its report is heard. */
    private fun returned(from: Int, event: Event?): Boolean {
        if (from == failures) { running--; return true }
        orphaned--
        (event as? ResponseComplete)?.let { responses.remove(it.key to it.attempt) }?.let { store.discard(it.attempt) }
        return false
    }

    /** The cleanup of [key]'s retirement has ended: with its record deleted on the disk when [removed]. */
    private fun cleaned(key: Key, removed: Boolean) { synchronized(store) { retiring.remove(key) }?.forEach { it(removed) } }

    /**
     * A write failed, so the disk holds what a death inside the step leaves, and this process goes
     * on as that death's successor would: nothing is crossed and no effect of the step is performed.
     * What a death would have ended, it ends: each run held or waiting to be finished is finished to
     * run again, each start not yet answered is answered, as failed with its outcome unknown unless
     * its answer was queued, and each exchange and copy is cancelled. An operation still running is
     * counted until it returns, and [enter] waits for it. The runs and the calls are the caller's
     * to settle, off the monitor.
     */
    private fun collapse(): Owed {
        val runs = process.runs.map { it.key to it.value.id } + queue.filterIsInstance<Finish>().map { it.key to it.run }
        val answers = queue.filterIsInstance<Answer>().map { Triple(it.key, it.registered, refusals[it.key]) } +
            process.starting.keys.map { Triple(it, false, "UnknownError") }
        val retired = retiring.values.flatten()
        process = Process()
        queue.clear()
        admitted.clear()
        refusals.clear()
        retiring.clear()
        failures++
        orphaned += running
        running = 0
        unread = true
        exchanges.values.forEach { it.cancel() }
        copies.values.forEach { it.set(true) }
        return Owed(runs, answers, retired)
    }

    private fun perform() {
        val effect: Effect
        val admits: Boolean
        val from: Int
        var written: JSONArray? = null
        synchronized(store) {
            effect = queue.removeFirstOrNull() ?: return
            admits = Lifecycle.admits(durable, process, effect)
            from = failures
            if (effect is Keep || effect is Copy || effect is Transmit || effect is Place || effect is Clean) running++
            if (effect is Copy && admits) copies[effect.key] = AtomicBoolean()
            // The start's answer forgets the requests; an admitted Keep has them from here.
            if (effect is Keep && admits) written = admitted[effect.key]?.requests
        }
        // The handle exists before the exchange runs, so the cancellation that follows a stop finds it, as a failed write's does.
        if (effect is Transmit && admits) TransferIO(exchange).let { io ->
            synchronized(store) { exchanges[effect.key to effect.attempt] = io; if (from != failures) io.cancel() }
        }
        when (effect) {
            is Keep -> start(effect, admits, from) { if (keep(effect.key, written)) CredentialsKept(effect.key) else CredentialsRefused(effect.key) }
            is Copy -> start(effect, admits, from) { if (admits && copy(effect.key)) SnapshotCommitted(effect.key) else SnapshotFailed(effect.key) }
            is CancelCopy -> synchronized(store) { copies[effect.key] }?.set(true)
            is Transmit -> start(effect, admits, from) { if (admits) transmit(effect) else ExchangeFailed(effect.key, effect.attempt, Failure.Cancelled) }
            is CancelIo -> synchronized(store) { exchanges[effect.key to effect.attempt] }?.cancel()
            is Place -> start(effect, admits, from) { if (admits && place(effect)) Placed(effect.key) else PlaceFailed(effect.key) }
            is Clean -> start(effect, true, from) { if (clean(effect.key)) Cleaned(effect.key) else CleanFailed(effect.key) }
            is Schedule -> if (admits) {
                val answer = { held: Boolean ->
                    report(from, if (held) Scheduled(effect.key, effect.job, effect.token) else ScheduleRefused(effect.key, effect.job, effect.token))
                }
                store.get(effect.key)?.let { platform.schedule(effect.key, effect.job, it, answer) } ?: answer(false)
            }
            is Cancel -> platform.cancel(effect.key, effect.job)
            is Finish -> platform.finish(effect.key, effect.run, effect.again)
            is Promote -> store.get(effect.key)?.let { platform.promote(effect.key, effect.run, it) }
            is Raise -> host.raise(effect.key, effect.delivery) { report(from, WakeEnded(effect.key, effect.delivery, it)) }
            is Expire -> host.expire(effect.key, effect.delivery)
            is Answer -> host.answer(effect.key, effect.registered, synchronized(store) { admitted.remove(effect.key); refusals.remove(effect.key) })
            is Acknowledge -> platform.acknowledge(effect.namespace)
        }
    }

    /** Run an operation's [body], counted since its effect left the queue under [from]. Its report is heard unless a write has failed since. */
    private fun start(effect: Effect, admits: Boolean, from: Int, body: () -> Event?) {
        if (launch == null) operations.execute {
            val event = body()
            if (event != null) report(from, event, operation = true) else synchronized(store) { returned(from, null) }
        } else launch.invoke(effect, admits) {
            val event = body()
            synchronized(store) { event.takeIf { returned(from, it) } }
        }
    }

    private fun call(request: JSONObject): JSONObject {
        val answer = exchange(request)
        if (answer.has("error")) throw Refused(answer.getString("error"))
        return answer
    }
    private fun request(record: JSONObject, op: String): JSONObject = JSONObject().put("op", op)
        .put("authority", record.getString("source")).put("origin", record.getString("origin"))
        .put("data", record.getString("data")).put("cache", record.getString("cache")).put("temporary", record.getString("temporary"))

    private fun keep(key: Key, written: JSONArray?): Boolean = try {
        requireNotNull(written)
        val record = requireNotNull(store.get(key))
        val requests = record.getJSONArray("requests")
        for (i in 0 until requests.length()) {
            val kept = requests.getJSONObject(i)
            call(request(record, "keep").put("key", kept.getString("credentialKey")).put("url", written.getJSONObject(i).getString("url"))
                .put("method", kept.getString("method")).put("body", kept.getBoolean("body"))
                .put("headers", written.getJSONObject(i).optJSONArray("headers") ?: JSONArray()))
        }
        true
    } catch (e: Exception) {
        synchronized(store) { refusals[key] = (e as? Refused)?.code ?: "UnknownError" }
        false
    }

    private fun copy(key: Key): Boolean {
        val cancelled = synchronized(store) { copies[key] } ?: return false
        return try {
            store.snapshot(key, cancelled::get) { record, path, file -> call(request(record, "snapshot").put("path", path).put("file", file.path)) }
        } catch (_: Exception) { false } finally { synchronized(store) { copies.remove(key, cancelled) } }
    }

    private fun transmit(effect: Transmit): Event {
        val key = effect.key
        val io = synchronized(store) { exchanges[key to effect.attempt] } ?: return ExchangeFailed(key, effect.attempt, Failure.Cancelled)
        var attempt: Registrations.Attempt? = null
        var kept = false
        try {
            val record = requireNotNull(store.get(key))
            val files = store.begin(key, effect.attempt, effect.request).also { attempt = it }
            val tick = ticks.scheduleAtFixedRate({
                runCatching { io.progress() }.getOrNull()?.let { moved ->
                    store.progress(files, moved.optLong("uploaded"), moved.optLong("downloaded"))?.let {
                        platform.progress(key, it)
                        host.changed(key.namespace)
                    }
                }
            }, 1, 1, TimeUnit.SECONDS)
            val answer = try {
                io.run(request(record, "run").put("key", files.credentialKey)
                    .put("method", record.getJSONArray("requests").getJSONObject(effect.request).getString("method"))
                    .put("upload", files.upload?.path ?: JSONObject.NULL).put("response", files.response.path)
                    .put("remaining", files.remaining ?: JSONObject.NULL))
            } finally { tick.cancel(false) }
            val failure = answer.optString("failure")
            return when {
                failure == "cancelled" -> ExchangeFailed(key, effect.attempt, Failure.Cancelled)
                failure == "network" -> ExchangeFailed(key, effect.attempt, Failure.Network)
                failure == "download-total-exceeded" -> ResponseComplete(key, effect.attempt, Response.TotalExceeded)
                failure.isNotEmpty() || answer.has("error") -> ExchangeFailed(key, effect.attempt, Failure.Fatal)
                else -> {
                    val status = answer.getInt("status")
                    synchronized(store) { responses[key to effect.attempt] = Registrations.Response(files, status, answer.optString("contentType")) }
                    kept = true
                    ResponseComplete(key, effect.attempt, when {
                        status !in 100..599 -> Response.Invalid
                        store.exceeds(files) -> Response.TotalExceeded
                        status in 200..299 -> Response.Ok
                        else -> Response.BadStatus
                    })
                }
            }
        } catch (_: Exception) {
            return ExchangeFailed(key, effect.attempt, Failure.Fatal)
        } finally {
            io.close()
            synchronized(store) { exchanges.remove(key to effect.attempt, io) }
            if (!kept) attempt?.let(store::discard)
        }
    }

    private fun place(effect: Place): Boolean = try {
        if (effect.again) pause()
        store.place(effect.key) { record, _, file, path, kept ->
            val answer = call(request(record, "place").put("file", file.path).put("path", path).put("kept", kept))
            if (answer.optBoolean("bodyLost")) throw Registrations.BodyMissing()
        }
        true
    } catch (_: Exception) { false }

    /** A failure leaves the retired record for a retirement made again or the next recovery to clean: a locked secret store is one. */
    private fun clean(key: Key): Boolean = try {
        store.clean(key, { record ->
            val requests = record.getJSONArray("requests")
            val kept = record.getJSONArray("records")
            // The directory of its kept responses goes whole, once one was placed there or may have been.
            if ((0 until kept.length()).any { i ->
                    requests.getJSONObject(i).isNull("saveTo") && kept.getJSONObject(i).let { it.has("path") || it.optBoolean("placing") }
                }) call(request(record, "removePlaced").put("path", Registrations.kept(key.id)).put("kept", true))
        }) { record, name -> call(request(record, "forget").put("key", name).put("authority", "").put("origin", "app")) }
        true
    } catch (_: Exception) { false }
}
