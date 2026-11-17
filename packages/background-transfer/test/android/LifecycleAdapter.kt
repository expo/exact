package exact.backgroundtransfer

import exact.backgroundtransfer.Lifecycle.Acknowledge
import exact.backgroundtransfer.Lifecycle.Answer
import exact.backgroundtransfer.Lifecycle.Cancel
import exact.backgroundtransfer.Lifecycle.Clean
import exact.backgroundtransfer.Lifecycle.CleanFailed
import exact.backgroundtransfer.Lifecycle.Cleaned
import exact.backgroundtransfer.Lifecycle.Conditions
import exact.backgroundtransfer.Lifecycle.Copy
import exact.backgroundtransfer.Lifecycle.CredentialsKept
import exact.backgroundtransfer.Lifecycle.Durable
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
import exact.backgroundtransfer.Lifecycle.Kind
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Op
import exact.backgroundtransfer.Lifecycle.Outcome
import exact.backgroundtransfer.Lifecycle.Place
import exact.backgroundtransfer.Lifecycle.Placed
import exact.backgroundtransfer.Lifecycle.Process
import exact.backgroundtransfer.Lifecycle.Promote
import exact.backgroundtransfer.Lifecycle.Raise
import exact.backgroundtransfer.Lifecycle.Response
import exact.backgroundtransfer.Lifecycle.ResponseComplete
import exact.backgroundtransfer.Lifecycle.Schedule
import exact.backgroundtransfer.Lifecycle.SnapshotCommitted
import exact.backgroundtransfer.Lifecycle.Start
import exact.backgroundtransfer.Lifecycle.Step
import exact.backgroundtransfer.Lifecycle.Transmit
import exact.backgroundtransfer.Lifecycle.Write
import exact.backgroundtransfer.LifecycleModel.Sim
import exact.backgroundtransfer.LifecycleModel.Task
import exact.backgroundtransfer.LifecycleModel.Violation
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executor

/**
 * The Android adapter under [LifecycleModel.Sim]: a [Transfers] over a [Registrations] in [root],
 * with its queue and its operations performed when the model says. The platform and the host are
 * the model's, so each effect the adapter performs is checked as the model's own would be, and
 * the transport is scripted to give the answer a case names. A death drops the [Transfers] and
 * keeps the disk.
 */
internal class LifecycleAdapter(private val sim: Sim, private val root: File) : LifecycleModel.Backend, Platform, Host {
    class Death : RuntimeException()

    private var transfers: Transfers? = null
    private val control = ArrayDeque<Runnable>()
    /** Each operation the adapter started and has not run, by its [Op], or by its key for a cleanup. */
    private val bodies = HashMap<Any, () -> Event?>()
    private var current: Effect? = null
    private var script: Event? = null
    /** The secret store removes nothing. */
    private var locked = false
    /** The writes left before the process dies inside a step; none is counted below zero. */
    private var writes = -1
    private var dead = false
    private var handles = 0L

    /** A process starts over what the last one left on disk. */
    fun boot() {
        die()
        val store = object : Registrations(root) {
            @Synchronized override fun write(write: Write, admission: Admission?, response: Response?) {
                if (writes == 0) { dead = true; throw Death() }
                if (writes > 0) writes--
                super.write(write, admission, response)
            }
        }
        transfers = Transfers(store, ::exchange, this, this, Executor { control.add(it) }, ::launch, sim.c.limits, pause = {})
    }

    fun die() {
        transfers = null
        control.clear()
        bodies.clear()
        writes = -1
        dead = false
    }

    /** The process dies after [count] more writes. */
    fun dying(count: Int) { writes = count }

    override fun feed(event: Event): Step {
        val live = transfers ?: throw Violation("$event with no process")
        if (event is Enter) return live.enter(event.namespace) ?: Lifecycle.step(durable(), live.state().first, event, sim.c.limits)
        if (event is Start) live.admit(event.key, admission(event.requests), written(event.requests))
        return live.feed(event)
    }

    override fun dispatch(effect: Effect) {
        current = effect
        control.removeFirst().run()
    }

    override fun report(task: Task, event: Event) {
        val body = bodies.remove(task.op) ?: throw Violation("the adapter never started ${task.op}")
        script = event
        val reported = try { body() } finally { script = null }
        if (reported != event) throw Violation("the adapter's ${task.op} reported $reported, not $event")
    }

    override fun clean(key: Key, removed: Boolean) {
        val body = bodies.remove(key) ?: throw Violation("the adapter never started the cleanup of $key")
        val event = if (removed) Cleaned(key) else CleanFailed(key)
        locked = !removed
        val reported = try { body() } finally { locked = false }
        if (reported != event) throw Violation("the adapter's cleanup of $key reported $reported, not $event")
    }

    override fun durable(): Durable = Registrations(root).durable()
    override fun state(): Pair<Process, List<Effect>> = transfers!!.state()
    /** The record as the app reads it. */
    fun read(key: Key): JSONObject? = transfers!!.read(key)

    private fun admission(requests: Int) = Registrations.Admission("Upload", "net.fetch https://example.test", "app",
        List(requests) { Registrations.Request("https://example.test/$it", "POST", bodyFrom = if (it == 0) "app:/data/video" else null) },
        null, JSONObject().put("data", root.path).put("cache", root.path).put("temporary", root.path))
    private fun written(requests: Int) = JSONArray(List(requests) { JSONObject().put("url", "https://example.test/$it?token=private") })

    /** The effect the queue is performing is the one the adapter asks the platform, the host or an operation for. */
    private fun asked(effect: Effect, admitted: Boolean = true) {
        // A process that died inside a step lets its runs go; the platform no longer hears it.
        if (dead) return
        if (current != effect) throw Violation("the adapter performed $effect where the queue held $current")
        sim.perform(effect, admitted)
    }

    private fun launch(effect: Effect, admitted: Boolean, body: () -> Event?) {
        bodies[when (effect) {
            is Keep -> Op(Kind.Keep, effect.key)
            is Copy -> Op(Kind.Copy, effect.key)
            is Place -> Op(Kind.Place, effect.key)
            is Transmit -> Op(Kind.Exchange, effect.key, effect.attempt)
            is Clean -> effect.key
            else -> throw Violation("$effect is no operation")
        }] = body
        asked(effect, admitted)
    }

    override val conditions: Conditions get() = sim.conditions
    override fun stopped() = sim.w.seen
    override fun restarted(namespace: Namespace) = namespace in sim.w.restarted
    override fun held(namespace: Namespace) = sim.w.jobs.filter { it.second == Job.UserJob }.map { it.first }.toSet()
    override fun schedule(key: Key, job: Job, record: JSONObject, answer: (Boolean) -> Unit) =
        asked((current as? Schedule)?.takeIf { it.key == key && it.job == job } ?: Schedule(key, job, -1))
    override fun cancel(key: Key, job: Job?) = asked(Cancel(key, job))
    override fun finish(key: Key, run: Int, again: Boolean) = asked(Finish(key, run, again))
    override fun promote(key: Key, run: Int, record: JSONObject) = asked(Promote(key, run))
    override fun progress(key: Key, record: JSONObject) {}
    override fun acknowledge(namespace: Namespace) = asked(Acknowledge(namespace))
    override fun raise(key: Key, delivery: Int, ended: (Outcome) -> Unit) = asked(Raise(key, delivery))
    override fun expire(key: Key, delivery: Int) = asked(Expire(key, delivery))
    override fun answer(key: Key, registered: Boolean, error: String?) = asked(Answer(key, registered))
    override fun changed(namespace: Namespace) {}

    /** The transport, the secret store and the app's files: each answers as the report a case asked for needs. */
    private fun exchange(request: JSONObject): JSONObject {
        val failed = JSONObject().put("error", "NotAllowedError")
        return when (request.getString("op")) {
            "keep" -> if (script is CredentialsKept) JSONObject().put("url", request.getString("url").substringBefore('?')) else failed
            "snapshot" -> if (script is SnapshotCommitted) JSONObject().also { File(request.getString("file")).writeText("upload") } else failed
            "open" -> JSONObject().put("handle", ++handles)
            "run" -> when (val report = script) {
                is ResponseComplete -> {
                    File(request.getString("response")).writeText("response")
                    val answer = JSONObject().put("contentType", "text/plain").put("downloaded", 8)
                    when (report.response) {
                        Response.Ok -> answer.put("status", 201)
                        Response.BadStatus -> answer.put("status", 404)
                        Response.Invalid -> answer.put("status", 999)
                        Response.TotalExceeded -> JSONObject().put("failure", "download-total-exceeded")
                    }
                }
                is ExchangeFailed -> JSONObject().put("failure", when (report.failure) {
                    Failure.Network -> "network"; Failure.Fatal -> "fetch-error"; Failure.Cancelled -> "cancelled"
                })
                else -> throw Violation("an exchange ran with no report asked of it")
            }
            "place" -> if (script is Placed) JSONObject() else failed
            "forget" -> if (locked) failed else JSONObject()
            "cancel", "close", "progress", "removePlaced" -> JSONObject()
            else -> throw Violation("the adapter asked the transport for $request")
        }
    }
}
