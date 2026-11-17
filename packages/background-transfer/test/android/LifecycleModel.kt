package exact.backgroundtransfer

import exact.backgroundtransfer.Lifecycle.Abort
import exact.backgroundtransfer.Lifecycle.Acknowledge
import exact.backgroundtransfer.Lifecycle.Answer
import exact.backgroundtransfer.Lifecycle.Ask
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
import exact.backgroundtransfer.Lifecycle.Delete
import exact.backgroundtransfer.Lifecycle.Durable
import exact.backgroundtransfer.Lifecycle.Effect
import exact.backgroundtransfer.Lifecycle.Enter
import exact.backgroundtransfer.Lifecycle.Event
import exact.backgroundtransfer.Lifecycle.ExchangeFailed
import exact.backgroundtransfer.Lifecycle.Expire
import exact.backgroundtransfer.Lifecycle.Failure
import exact.backgroundtransfer.Lifecycle.Finish
import exact.backgroundtransfer.Lifecycle.Intent
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.JobStarted
import exact.backgroundtransfer.Lifecycle.JobStopped
import exact.backgroundtransfer.Lifecycle.Keep
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Kind
import exact.backgroundtransfer.Lifecycle.Limits
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Op
import exact.backgroundtransfer.Lifecycle.Outcome
import exact.backgroundtransfer.Lifecycle.Pending
import exact.backgroundtransfer.Lifecycle.Phase
import exact.backgroundtransfer.Lifecycle.Place
import exact.backgroundtransfer.Lifecycle.PlaceFailed
import exact.backgroundtransfer.Lifecycle.Placed
import exact.backgroundtransfer.Lifecycle.Process
import exact.backgroundtransfer.Lifecycle.Promote
import exact.backgroundtransfer.Lifecycle.Put
import exact.backgroundtransfer.Lifecycle.Raise
import exact.backgroundtransfer.Lifecycle.Record
import exact.backgroundtransfer.Lifecycle.Response
import exact.backgroundtransfer.Lifecycle.ResponseComplete
import exact.backgroundtransfer.Lifecycle.Result
import exact.backgroundtransfer.Lifecycle.Retire
import exact.backgroundtransfer.Lifecycle.Run
import exact.backgroundtransfer.Lifecycle.Schedule
import exact.backgroundtransfer.Lifecycle.ScheduleRefused
import exact.backgroundtransfer.Lifecycle.Scheduled
import exact.backgroundtransfer.Lifecycle.SessionEnded
import exact.backgroundtransfer.Lifecycle.SessionStarted
import exact.backgroundtransfer.Lifecycle.SnapshotCommitted
import exact.backgroundtransfer.Lifecycle.SnapshotFailed
import exact.backgroundtransfer.Lifecycle.Start
import exact.backgroundtransfer.Lifecycle.Step
import exact.backgroundtransfer.Lifecycle.Stop
import exact.backgroundtransfer.Lifecycle.Stopping
import exact.backgroundtransfer.Lifecycle.Transmit
import exact.backgroundtransfer.Lifecycle.ViewAttached
import exact.backgroundtransfer.Lifecycle.ViewDetached
import exact.backgroundtransfer.Lifecycle.WakeEnded

/**
 * What [Lifecycle.step] cannot see, for its tests: the platform's jobs, the queue of effects,
 * the operations in flight and the faults. A [Sim] applies an event and queues its effects; the
 * queue performs one effect at a time, as the adapter does, and [Config.lag] is how many may
 * wait there while other events come. The platform answers a schedule in a later event.
 */
internal object LifecycleModel {
    val app = Namespace("app")
    val drive = Namespace("agent-drive", agent = true)
    /** Every session the model's host starts. The machine only compares incarnations, so one value and a stale one cover it. */
    const val LIVE = 1

    class Violation(message: String) : Exception(message)

    /** An operation in flight. One that was not admitted when it started writes nothing and reports only its failure. */
    data class Task(val op: Op, val started: Boolean)

    /** The platform's answer to a [Schedule], not yet delivered. */
    data class Reply(val key: Key, val job: Job, val token: Int, val accepted: Boolean) {
        fun event(): Event = if (accepted) Scheduled(key, job, token) else ScheduleRefused(key, job, token)
    }

    /** What a reboot does to user-initiated jobs: it drops them, keeps them, or keeps them without their standing. */
    enum class Reboot { Drops, Keeps, Demotes }

    /**
     * @property persons whether a start is a person's act.
     * @property denied the platform refuses every user-initiated job.
     * @property starved WorkManager may refuse to store work it does not hold: every step is also taken with it refusing.
     * @property lag how many effects may wait in the queue behind a [Transmit], a [Finish] or a [Cancel] for a later
     *   move to perform them; at 0 every effect is performed with its step, a death tried before each.
     * @property late the platform answers a schedule in a later move; otherwise before the next effect.
     * @property faults how many faults one path may take; unbounded when null.
     * @property straight no operation fails and nothing is aborted.
     */
    data class Config(
        val api: Int = 35, val limits: Limits = Limits(), val namespaces: List<Namespace> = listOf(app),
        val persons: List<Boolean> = listOf(true), val requests: Int = 1, val generations: Int = 1,
        val denied: Boolean = false, val starved: Boolean = false,
        val reboots: List<Reboot> = listOf(Reboot.Drops), val lag: Int = 8, val late: Boolean = true,
        val responses: List<Response> = listOf(Response.Ok, Response.BadStatus),
        val failures: List<Failure> = Failure.values().toList(), val stops: List<Stop> = Stop.values().toList(),
        val outcomes: List<Outcome> = Outcome.values().toList(), val updates: Boolean = true, val faults: Int? = null,
        val straight: Boolean = false)

    /**
     * @property jobs what the platform holds, pending or running.
     * @property limbo work a force-stop removed, which WorkManager schedules again once a process starts.
     * @property active runs the platform started and has not seen end.
     * @property halts runs the platform stopped, and why, until their run ends.
     * @property lost namespaces whose jobs the platform may have dropped since their last recovery.
     * @property restarted namespaces not recovered since a reboot or an update.
     * @property flag the force-stop a process after it can read (API 35).
     * @property token the cancelled custody token, good within one boot and install.
     * @property seen the evidence this process read when it started.
     * @property person a person stopped the app, so nothing runs until they open it.
     * @property halted the package is in the stopped state and receives no broadcast.
     * @property receiver a boot or package-replaced broadcast is owed.
     * @property queue effects the adapter has yet to perform, in order.
     * @property replies the platform's answers to schedules, not yet delivered.
     * @property barred wakes the host could not raise: another namespace's session was live.
     * @property doomed registrations unfinished at a force-stop whose evidence still stands.
     * @property tasked registrations whose user-initiated job a person stopped where no step recorded it.
     * @property demoted user-initiated jobs the platform holds without their standing.
     * @property made generations started in each namespace; a generation is never made twice.
     * @property faults faults taken so far, counted only where the search bounds them.
     */
    data class World(
        val durable: Durable = Durable(), val process: Process? = null, val visible: Boolean = false,
        val jobs: Set<Pair<Key, Job>> = emptySet(), val limbo: Set<Pair<Key, Job>> = emptySet(),
        val active: Map<Int, Pair<Key, Job>> = emptyMap(), val halts: Map<Int, Stop> = emptyMap(),
        val lost: Set<Namespace> = emptySet(), val restarted: Set<Namespace> = emptySet(),
        val flag: Boolean = false, val token: Boolean = false, val seen: Boolean = false,
        val person: Boolean = false, val halted: Boolean = false, val receiver: Boolean = false,
        val queue: List<Effect> = emptyList(), val replies: Set<Reply> = emptySet(), val tasks: Set<Task> = emptySet(),
        val cleans: Set<Key> = emptySet(), val wakes: Map<Key, Int> = emptyMap(), val barred: Set<Key> = emptySet(),
        val asked: Set<Key> = emptySet(), val doomed: Set<Key> = emptySet(), val tasked: Set<Key> = emptySet(),
        val demoted: Set<Key> = emptySet(), val made: Map<Namespace, Int> = emptyMap(), val faults: Int = 0)

    private fun unfinished(r: Record) = r.phase == Phase.Preparing || r.phase == Phase.Ready || r.phase == Phase.Sending

    /** The process ends; what the platform and the disk hold stays. */
    fun die(w: World, c: Config): World = evidence(w.copy(process = null, visible = false, active = emptyMap(),
        halts = emptyMap(), seen = false, queue = emptyList(), replies = emptySet(), tasks = emptySet(),
        cleans = emptySet(), wakes = emptyMap(), barred = emptySet(), asked = emptySet()), c)

    /** A force-stop the next process cannot learn of no longer binds (declared in the design). */
    private fun evidence(w: World, c: Config): World {
        val standing = w.token || w.seen || (c.api >= 35 && w.flag)
        return w.copy(doomed = w.doomed.filter { standing || it.namespace in w.durable.stopping }.toSet())
    }

    /** On API 35 the history of process starts keeps the force-stop for every process until a recovery is acknowledged. */
    fun boot(w: World, c: Config): World =
        w.copy(process = Process(), seen = w.token || (c.api >= 35 && w.flag), halted = false)

    fun reboot(w: World, c: Config, kind: Reboot = Reboot.Drops): World = evidence(die(w, c).let {
        val jobs = it.jobs.filter { job -> job.second == Job.Work || kind != Reboot.Drops }.toSet()
        val users = jobs.filter { job -> job.second == Job.UserJob }.map { job -> job.first }.toSet()
        it.copy(jobs = jobs, demoted = if (kind == Reboot.Demotes) users else it.demoted intersect users, token = false,
            lost = c.namespaces.toSet(), restarted = c.namespaces.toSet(), tasked = emptySet(), receiver = !it.halted)
    }, c)

    fun forceStop(w: World, c: Config): World = die(w, c).let {
        it.copy(limbo = it.limbo + it.jobs.filter { job -> job.second == Job.Work }, jobs = emptySet(), demoted = emptySet(),
            token = true, flag = true, lost = c.namespaces.toSet(), person = true, halted = true, receiver = false,
            doomed = it.doomed + it.durable.records.values.filter(::unfinished).map { r -> r.key })
    }

    fun update(w: World, c: Config, drop: Boolean): World = evidence(die(w, c).let {
        it.copy(jobs = if (drop) emptySet() else it.jobs, demoted = if (drop) emptySet() else it.demoted, token = false,
            lost = c.namespaces.toSet(), restarted = c.namespaces.toSet(), tasked = emptySet(), receiver = !it.halted)
    }, c)

    /** The Task Manager's stop: the process ends with no callback and the running user-initiated job is not run again. */
    fun taskStop(w: World, c: Config): World {
        val stopped = w.active.values.filter { it.second == Job.UserJob }.toSet()
        val recorded = stopped.map { it.first }.filter { w.durable.records[it]?.job == Job.UserJob }
        return die(w, c).let {
            it.copy(jobs = it.jobs - stopped, demoted = it.demoted - stopped.map { job -> job.first }.toSet(),
                lost = c.namespaces.toSet(), person = true, tasked = it.tasked + recorded)
        }
    }

    /**
     * The adapter under a [Sim]. Its steps, its disk and its queue stand where the model's own
     * would, and what it asks of the platform and the host it asks through [Sim.perform].
     */
    interface Backend {
        fun feed(event: Event): Step
        /** Perform the next queued effect, which is [effect]. */
        fun dispatch(effect: Effect)
        /** Run the operation of [task] against a transport that makes it report [event]. */
        fun report(task: Task, event: Event)
        /** Run the cleanup a [Clean] started, against a secret store that removes its credentials when [removed]. */
        fun clean(key: Key, removed: Boolean)
        fun durable(): Durable
        fun state(): Pair<Process, List<Effect>>
    }

    class Sim(var w: World, val c: Config) {
        /** The adapter the events go through; the function itself when null. */
        var backend: Backend? = null
        val crashes = ArrayList<World>()
        /** WorkManager refuses to store work it does not hold. */
        var starve = false
        /** The effects performed, in order. */
        val effects = ArrayList<Effect>()
        val conditions get() = Conditions(c.api, w.visible)

        fun record(key: Key) = w.durable.records[key]

        fun enter(namespace: Namespace): Step = feed(Enter(namespace, w.seen, namespace in w.restarted,
            w.jobs.filter { it.second == Job.UserJob }.map { it.first }.toSet(), conditions))

        /** One step: its writes land and its effects join the queue. */
        fun feed(event: Event): Step {
            val before = w
            val process = w.process ?: throw Violation("$event with no process")
            val real = backend
            val step = real?.feed(event) ?: Lifecycle.step(w.durable, process, event, c.limits)
            checkStep(before, event, step)
            if (real != null) {
                if (real.durable() != step.durable) throw Violation("the disk holds ${real.durable()} after $event, not ${step.durable}")
                mirror(real)
                pass()
                return step
            }
            var durable = w.durable
            for (write in step.writes) {
                crashes += die(w.copy(durable = durable), c)
                durable = durable.with(write)
            }
            // Effects that change nothing here are not queued: asked for again and again behind a slow queue, they would have no end.
            val (silent, queued) = step.effects.partition { it is Promote || it is Expire || it is CancelIo || it is CancelCopy }
            effects += silent
            w = w.copy(durable = step.durable, process = step.process, queue = w.queue + queued)
            pass()
            return step
        }

        /** The platform starts a run of a job it holds. */
        fun run(job: Pair<Key, Job>): Step {
            val id = (0..w.active.size).first { it !in w.active }
            w = w.copy(active = w.active + (id to job))
            return feed(JobStarted(job.first, Run(id, job.second, demoted = job.second == Job.UserJob && job.first in w.demoted), conditions))
        }

        /** The platform stops a run. It does not run again a user-initiated job a person stopped, whatever the service answers. */
        fun stop(id: Int, reason: Stop): Step {
            val job = w.active.getValue(id)
            w = w.copy(halts = w.halts + (id to reason))
            if (reason == Stop.Person && job.second == Job.UserJob) w = w.copy(jobs = w.jobs - job,
                tasked = if (record(job.first)?.job == Job.UserJob) w.tasked + job.first else w.tasked)
            return feed(JobStopped(job.first, id, reason))
        }

        /** The adapter's volatile state and queue, as the model's. */
        private fun mirror(real: Backend) {
            val (process, queue) = real.state()
            w = w.copy(durable = real.durable(), process = process, queue = queue)
        }

        /** An operation reports and is over. */
        fun report(task: Task, event: Event): Step {
            w = w.copy(tasks = w.tasks - task)
            backend?.report(task, event)
            return feed(event)
        }

        /** A cleanup reports and is over: everything [removed], or something left. */
        fun cleaned(key: Key, removed: Boolean = true): Step {
            w = w.copy(cleans = w.cleans - key)
            backend?.clean(key, removed)
            return feed(if (removed) Cleaned(key) else CleanFailed(key))
        }

        fun answer(reply: Reply): Step {
            w = w.copy(replies = w.replies - reply)
            return feed(reply.event())
        }

        /** The queue performs its next effect. */
        fun dispatch() {
            val effect = w.queue.first()
            effects += effect
            val real = backend
            if (real == null) {
                w = w.copy(queue = w.queue.drop(1))
                perform(effect, Lifecycle.admits(w.durable, w.process!!, effect))
            } else {
                real.dispatch(effect)
                mirror(real)
            }
            pass()
        }

        /**
         * The queue holds a [Transmit], a [Finish] and a [Cancel], and whatever follows one, until a later move performs
         * them: the lease and the platform can see their delay. It holds at most [Config.lag] effects. Any other effect
         * with nothing ahead of it is performed with its step, after a death there is tried. Started late, such an
         * operation is not admitted and reports as a failed one would, which the search takes in any case.
         */
        private fun pass() {
            while (true) {
                val head = w.queue.firstOrNull()
                val reply = if (c.late) null else w.replies.firstOrNull()
                if (head != null && (w.queue.size > c.lag || (head !is Transmit && head !is Finish && head !is Cancel))) {
                    crashes += die(w, c)
                    dispatch()
                } else if (reply != null && head == null) {
                    crashes += die(w, c)
                    answer(reply)
                } else break
            }
            forget()
        }

        /** An answer to a schedule that a later one replaced is ignored whenever it comes, so it is delivered here. */
        private fun forget() {
            val p = w.process ?: return
            val stale = w.replies.filter { p.asking[Ask(it.key, it.job)]?.token != it.token }
            for (reply in stale) {
                val step = Lifecycle.step(w.durable, p, reply.event(), c.limits)
                if (step.accepted || step.writes.isNotEmpty() || step.effects.isNotEmpty() || step.process != p)
                    throw Violation("the stale $reply was taken")
            }
            if (stale.isNotEmpty()) w = w.copy(replies = w.replies - stale.toSet())
        }

        private fun task(op: Op, admitted: Boolean) { w = w.copy(tasks = w.tasks + Task(op, admitted)) }

        /** What an effect does to the platform, the host and the operations in flight. */
        fun perform(effect: Effect, admitted: Boolean) {
            val p = w.process!!
            when (effect) {
                is Keep -> task(Op(Kind.Keep, effect.key), admitted)
                is Copy -> task(Op(Kind.Copy, effect.key), admitted)
                is Place -> task(Op(Kind.Place, effect.key), admitted)
                is Transmit -> if (admitted) {
                    val r = record(effect.key)
                    if (p.leases[effect.key] != effect.attempt || r?.phase != Phase.Sending || r.attempt != effect.attempt)
                        throw Violation("$effect starts without its lease")
                    if (p.runs[effect.key]?.demoted == true) throw Violation("$effect sends under a job without its standing")
                    if (effect.key in w.doomed) throw Violation("$effect sends after a force-stop")
                    if (effect.key in w.tasked) throw Violation("$effect sends after a person stopped its job")
                    task(Op(Kind.Exchange, effect.key, effect.attempt), admitted = true)
                } else task(Op(Kind.Exchange, effect.key, effect.attempt), admitted = false)
                is Schedule -> if (admitted) {
                    val job = effect.key to effect.job
                    val accepted = when (effect.job) {
                        Job.Work -> job in w.jobs || !starve
                        Job.UserJob -> c.api >= 34 && w.visible && !c.denied
                    }
                    // A user-initiated job scheduled afresh has its standing.
                    if (accepted) w = w.copy(jobs = w.jobs + job, demoted = if (effect.job == Job.UserJob) w.demoted - effect.key else w.demoted)
                    w = w.copy(replies = w.replies + Reply(effect.key, effect.job, effect.token, accepted))
                }
                is Cancel -> {
                    val named = { job: Pair<Key, Job> -> job.first == effect.key && (effect.job == null || job.second == effect.job) }
                    w = w.copy(jobs = w.jobs.filterNot(named).toSet(), limbo = w.limbo.filterNot(named).toSet(),
                        demoted = if (effect.job == Job.Work) w.demoted else w.demoted - effect.key)
                }
                is CancelCopy, is CancelIo, is Expire, is Promote -> {}
                is Answer -> {
                    if (effect.key !in w.asked) throw Violation("$effect answers no call")
                    w = w.copy(asked = w.asked - effect.key)
                }
                is Raise -> {
                    if (effect.key in w.wakes) throw Violation("$effect while a wake is in flight")
                    w = w.copy(wakes = w.wakes + (effect.key to effect.delivery))
                    // The host boots a session with no view for a wake that finds none, and cannot raise into another namespace's.
                    val session = p.session
                    if (session == null) feed(SessionStarted(effect.key.namespace, LIVE))
                    else if (session.namespace != effect.key.namespace) w = w.copy(barred = w.barred + effect.key)
                }
                is Finish -> {
                    val job = w.active[effect.run] ?: throw Violation("$effect ends a run that is not running")
                    val reason = w.halts[effect.run]
                    // A stopped worker's work stays whatever it answers; a job a person stopped is never run again.
                    val kept = when {
                        reason != null && job.second == Job.Work -> true
                        reason == Stop.Person -> false
                        else -> effect.again
                    }
                    w = w.copy(active = w.active - effect.run, halts = w.halts - effect.run, jobs = if (kept) w.jobs else w.jobs - job)
                }
                is Clean -> {
                    w.tasks.firstOrNull { it.started && it.op.key == effect.key }?.let { throw Violation("$effect while $it may still write") }
                    if (effect.key in w.cleans) throw Violation("$effect while a cleanup of its files runs")
                    w = w.copy(cleans = w.cleans + effect.key)
                }
                // The receiver only enqueues recovery work, so the recovery stays owed until one completes.
                is Acknowledge -> w = w.copy(lost = w.lost - effect.namespace, restarted = w.restarted - effect.namespace,
                    token = false, flag = false, seen = false, person = false, receiver = false)
            }
        }

        private fun checkStep(before: World, event: Event, step: Step) {
            fun fail(message: String): Nothing = throw Violation("$message, on $event")
            val was = before.process!!
            val now = step.process
            if (!step.accepted && step.writes.isNotEmpty()) fail("a refused event wrote")
            if ((event is Start || event is Abort || event is Retire) && step.accepted && was.session?.namespace != event.namespace)
                fail("a call crossed into ${event.namespace}")
            if (event is JobStarted && event.key.namespace.agent && event.namespace in was.crossed && was.runs[event.key] == null &&
                was.session?.namespace != event.namespace && (step.writes.isNotEmpty() || step.effects != listOf(Finish(event.key, event.run.id, again = true))))
                fail("an agent drive's job started outside its session was not kept")
            val shown = now.session.let { it != null && it.namespace == event.namespace && it.view }
            if (event is JobStarted && event.run.job == Job.Work && now.runs[event.key]?.id == event.run.id && shown) {
                val r = step.durable.records[event.key]
                if (r != null && unfinished(r) && Lifecycle.promotes(r.intent, event.conditions) &&
                    Promote(event.key, event.run.id) !in step.effects) fail("a person's work started under a view without foreground execution")
            }
            if (event is Lifecycle.Keyed) {
                val old = before.durable.records[event.key]?.takeIf { it.phase == Phase.Ended }
                val new = step.durable.records[event.key]?.takeIf { it.phase == Phase.Ended }
                if (event is PlaceFailed && old != null && new != null && event.key in was.placing && event.key in was.runs &&
                    event.key !in was.launch && old.wakes < c.limits.wakes && new.wakes <= old.wakes)
                    fail("a failed background placement spent no delivery")
                if (event is WakeEnded && event.outcome == Outcome.Completed && old != null && new != null &&
                    was.waking[event.key] == event.delivery && old.wakes >= c.limits.completedWakes &&
                    (new.job != null || new.wakes < c.limits.wakes)) fail("a completed wake past its limit left a background delivery owed")
            }
            var records = before.durable.records
            for (write in step.writes) when (write) {
                is Stopping -> if (event !is Enter) fail("only recovery marks a force-stop")
                is Delete -> {
                    if (write.key.namespace != event.namespace) fail("wrote ${write.key.namespace}")
                    if (records[write.key]?.phase != Phase.Retired) fail("deleted a record that was not retired")
                    before.tasks.firstOrNull { it.started && it.op.key == write.key }?.let { fail("deleted a record while $it may still write") }
                    records = records - write.key
                }
                is Put -> {
                    val r = write.record
                    val old = records[r.key]
                    if (r.key.namespace != event.namespace) fail("wrote ${r.key.namespace}")
                    if (old != null) {
                        if (old.intent != r.intent) fail("the intent changed")
                        if (old.result != null && old.result != r.result) fail("a result changed from ${old.result} to ${r.result}")
                        if (old.phase == Phase.Ended && r.phase != Phase.Ended && r.phase != Phase.Retired) fail("an ended transfer resumed")
                        if (old.phase == Phase.Retired && r.phase != Phase.Retired) fail("a retired transfer returned")
                        if (r.attempt < old.attempt || r.delivery < old.delivery) fail("a counter went back")
                        val dropped = old.job == Job.UserJob && r.job != Job.UserJob && unfinished(r)
                        if (dropped && (r.key to Job.UserJob) in before.jobs && r.key !in before.demoted)
                            fail("a held user-initiated job was dropped or replaced")
                        if (event is Enter && old.result == null && r.result == Result.Aborted && !event.stopped &&
                            event.namespace !in before.durable.stopping && r.key !in before.tasked)
                            fail("recovery aborted a transfer nobody stopped")
                    }
                    if ((r.phase == Phase.Ended) != (r.result != null) && r.phase != Phase.Retired) fail("phase and result disagree")
                    if (r.parked && (!unfinished(r) || r.job != null)) fail("a parked transfer holds a job or is over")
                    if (r.parked && old?.parked != true && event !is JobStopped && event !is ScheduleRefused) fail("parked for no reason")
                    if (r.parked && r.intent == Intent.Ordinary && event is JobStopped) fail("a program's transfer was parked by a stop")
                    if (old?.result == null && r.result == Result.FetchError && r.intent == Intent.Person && event is JobStopped) fail("a stop failed a person's transfer")
                    records = records + (r.key to r)
                }
            }
            for ((key, lease) in was.leases) if (now.leases[key] != lease) {
                val own = (event is ResponseComplete && event.key == key && event.attempt == lease) ||
                    (event is ExchangeFailed && event.key == key && event.attempt == lease)
                if (!own && (CancelIo(key, lease) !in step.effects || Op(Kind.Exchange, key, lease) !in now.draining))
                    fail("the lease of $key was taken with its exchange neither cancelled nor awaited")
            }
            for (effect in step.effects) {
                val namespace = when (effect) {
                    is Acknowledge -> effect.namespace
                    is Keep -> effect.key.namespace; is Schedule -> effect.key.namespace; is Cancel -> effect.key.namespace
                    is Copy -> effect.key.namespace; is CancelCopy -> effect.key.namespace; is Answer -> effect.key.namespace
                    is Transmit -> effect.key.namespace; is CancelIo -> effect.key.namespace; is Place -> effect.key.namespace
                    is Raise -> effect.key.namespace; is Expire -> effect.key.namespace; is Finish -> effect.key.namespace
                    is Promote -> effect.key.namespace; is Clean -> effect.key.namespace
                }
                if (namespace != event.namespace) fail("$effect reaches $namespace")
                when (effect) {
                    is Transmit -> {
                        val r = records[effect.key]
                        val old = before.durable.records[effect.key]
                        if (r == null || r.phase != Phase.Sending || r.attempt != effect.attempt || r.result != null) fail("$effect without its lease")
                        if (old != null && old.attempt >= effect.attempt) fail("$effect reuses an attempt")
                        if (effect.key.namespace !in now.crossed) fail("$effect before recovery")
                        if (now.draining.any { it.kind == Kind.Exchange && it.key == effect.key }) fail("$effect overlaps a cancelled attempt")
                    }
                    is Raise -> {
                        val r = records[effect.key]
                        val old = before.durable.records[effect.key]
                        if (r == null || r.phase != Phase.Ended || !r.placed || r.delivery != effect.delivery) fail("$effect without its record")
                        if (old == null || old.delivery != effect.delivery - 1) fail("$effect reuses a delivery")
                        if (now.session.let { it != null && it.namespace != effect.key.namespace }) fail("$effect enters ${now.session}")
                    }
                    is Finish -> {
                        val waking = effect.key in was.waking && effect.key in now.waking
                        val own = (event is JobStopped && event.run == effect.run) || (event is JobStarted && event.run.id == effect.run)
                        if (waking && !own) fail("$effect releases a job whose wake is in flight")
                    }
                    is Promote -> {
                        if (c.api >= 35) fail("$effect asks for a capped foreground service")
                        if (!shown) fail("$effect asks for a foreground service with no view")
                    }
                    is Clean -> if (effect.key in now.keeping || effect.key in now.copying || effect.key in now.placing ||
                        effect.key in now.leases || now.draining.any { it.key == effect.key }) fail("$effect while an operation of its has not reported")
                    else -> {}
                }
            }
        }
    }

    /** What must hold in every state, including one a death inside a step left. */
    fun check(w: World, c: Config) {
        fun fail(message: String): Nothing = throw Violation(message)
        val p = w.process
        val records = w.durable.records
        val draining = p?.draining ?: emptySet()
        fun live(op: Op) = when (op.kind) {
            Kind.Keep -> op.key in p!!.keeping
            Kind.Copy -> op.key in p!!.copying
            Kind.Place -> op.key in p!!.placing
            Kind.Exchange -> p!!.leases[op.key] == op.attempt
        }
        for (task in w.tasks) {
            if (task.op !in draining && !live(task.op)) fail("$task is in flight and nothing waits for its report")
            if (!task.started) continue
            if (records[task.op.key] == null) fail("$task outlived its record")
            if (task.op.key in w.cleans) fail("$task may write while its files are removed")
            val r = records.getValue(task.op.key)
            if (task.op.kind == Kind.Exchange && task.op !in draining && (r.phase != Phase.Sending || r.attempt != task.op.attempt))
                fail("$task sends without the lease of $r")
        }
        if (w.tasks.filter { it.op.kind == Kind.Exchange }.groupBy { it.op.key }.any { it.value.size > 1 }) fail("two exchanges for one transfer")
        if (p != null) {
            val starts = w.queue.mapNotNull {
                when (it) {
                    is Keep -> Op(Kind.Keep, it.key); is Copy -> Op(Kind.Copy, it.key); is Place -> Op(Kind.Place, it.key)
                    is Transmit -> Op(Kind.Exchange, it.key, it.attempt); else -> null
                }
            }
            val waited = draining + p.keeping.map { Op(Kind.Keep, it) } + p.copying.map { Op(Kind.Copy, it) } +
                p.placing.map { Op(Kind.Place, it) } + p.leases.map { Op(Kind.Exchange, it.key, it.value) }
            val reporting = w.tasks.map { it.op }.toSet() + starts
            waited.firstOrNull { it !in reporting }?.let { fail("the machine waits for $it, which nothing will report") }
            val finishing = w.queue.filterIsInstance<Finish>().map { it.run }
            if (w.active.keys != p.runs.values.map { it.id }.toSet() + finishing) fail("runs ${w.active} and ${p.runs} disagree")
            for ((key, delivery) in p.waking) if (w.wakes[key] != delivery && Raise(key, delivery) !in w.queue) fail("the wake of $key is nowhere")
            for ((key, delivery) in w.wakes) if (p.waking[key] != delivery) fail("the wake of $key is not recorded")
            for (key in p.starting.keys) if (key !in p.keeping && key !in p.copying && p.asking.keys.none { it.key == key } &&
                w.queue.none { it is Answer && it.key == key }) fail("nothing is left to answer the start of $key")
            for (key in p.cleaning) if (key !in w.cleans && Clean(key) !in w.queue) fail("the machine waits for a cleanup of $key, which nothing will report")
            for (key in w.cleans) if (key !in p.cleaning) fail("the cleanup of $key is not recorded")
        } else if (w.active.isNotEmpty()) fail("runs with no process")
        for (r in records.values) {
            if (unfinished(r) && !r.parked && r.stops >= c.limits.stops) fail("a transfer stopped ${r.stops} times goes on: $r")
            val recovering = p == null || r.key.namespace !in p.crossed
            val asking = p != null && p.asking.keys.any { it.key == r.key }
            val owes = (unfinished(r) && !r.parked) || (r.phase == Phase.Ended && r.wakes < c.limits.wakes)
            if (owes && r.job == null && !recovering && !asking) fail("no job is recorded for $r")
            if (r.job != null && (r.key to r.job) !in w.jobs && r.key.namespace !in w.lost && r.key !in w.tasked &&
                (p == null || Ask(r.key, r.job) !in p.asking)) fail("the platform holds no job for $r")
        }
    }

    private fun <T> Set<T>.tight(): Set<T> = when (size) { 0 -> emptySet(); 1 -> java.util.Collections.singleton(first()); else -> this }
    private fun <K, V> Map<K, V>.tight(): Map<K, V> = when (size) {
        0 -> emptyMap()
        1 -> entries.first().let { java.util.Collections.singletonMap(it.key, it.value) }
        else -> this
    }

    /** The same state whatever numbers its attempts, deliveries, tokens and runs carry, in the least memory. */
    fun canonical(w: World): World {
        val records = w.durable.records
        fun attempt(key: Key, value: Int) = if (records[key]?.attempt == value) 0 else -1
        fun delivery(key: Key, value: Int) = if (records[key].let { it == null || it.delivery == value }) 0 else -1
        fun token(key: Key, job: Job, value: Int) = if (w.process?.asking?.get(Ask(key, job))?.token == value) 0 else -1
        fun op(op: Op) = if (op.kind == Kind.Exchange) op.copy(attempt = attempt(op.key, op.attempt)) else op
        val ids = w.active.entries.sortedWith(compareBy({ it.value.first.namespace.name }, { it.value.first.generation }, { it.value.second }))
            .mapIndexed { index, entry -> entry.key to index }.toMap()
        fun effect(e: Effect): Effect = when (e) {
            is Schedule -> e.copy(token = token(e.key, e.job, e.token))
            is Transmit -> e.copy(attempt = attempt(e.key, e.attempt))
            is Raise -> e.copy(delivery = delivery(e.key, e.delivery))
            is Finish -> e.copy(run = ids.getValue(e.run))
            else -> e
        }
        return w.copy(
            durable = Durable(records.mapValues { it.value.copy(attempt = 0, delivery = 0) }.tight(), w.durable.stopping.tight()),
            process = w.process?.let { p -> Process(
                crossed = p.crossed.tight(), session = p.session,
                runs = p.runs.mapValues { it.value.copy(id = ids.getValue(it.value.id)) }.tight(),
                leases = p.leases.mapValues { attempt(it.key, it.value) }.tight(),
                starting = p.starting.tight(), keeping = p.keeping.tight(), copying = p.copying.tight(), placing = p.placing.tight(),
                draining = p.draining.map(::op).toSet().tight(),
                waking = p.waking.mapValues { delivery(it.key, it.value) }.tight(), launch = p.launch.tight(),
                asking = p.asking.mapValues { Pending(0, it.value.from) }.tight(), asks = 0, cleaning = p.cleaning.tight()) },
            jobs = w.jobs.tight(), limbo = w.limbo.tight(), active = w.active.mapKeys { ids.getValue(it.key) }.tight(),
            halts = w.halts.mapKeys { ids.getValue(it.key) }.tight(), lost = w.lost.tight(), restarted = w.restarted.tight(),
            // A schedule that a later one replaced is not performed.
            queue = if (w.queue.isEmpty()) emptyList() else w.queue.map(::effect).filter { it !is Schedule || it.token == 0 },
            replies = w.replies.map { it.copy(token = token(it.key, it.job, it.token)) }.toSet().tight(),
            tasks = w.tasks.map { it.copy(op = op(it.op)) }.toSet().tight(), cleans = w.cleans.tight(),
            wakes = w.wakes.mapValues { delivery(it.key, it.value) }.tight(), barred = w.barred.tight(), asked = w.asked.tight(),
            doomed = w.doomed.tight(), tasked = w.tasked.filter { records[it]?.job == Job.UserJob }.toSet().tight(),
            demoted = w.demoted.filter { (it to Job.UserJob) in w.jobs }.toSet().tight(), made = w.made.tight())
    }

    /** @property background whether the step needs no person: an entrant the platform owes, or work it finishes. */
    data class Move(val label: String, val world: World, val background: Boolean)

    /** A session event that names no live session is refused and changes nothing. */
    private fun staleSessionEventsAreRefused(w: World, c: Config) {
        val p = w.process ?: return
        for (namespace in c.namespaces) if (namespace in p.crossed) {
            val events = listOf(SessionEnded(namespace, LIVE + 1), ViewDetached(namespace, LIVE + 1),
                ViewAttached(namespace, LIVE + 1, Conditions(c.api, w.visible))) +
                if (p.session != null) listOf(SessionStarted(namespace, LIVE + 1)) else emptyList()
            for (event in events) {
                val step = Lifecycle.step(w.durable, p, event, c.limits)
                if (step.accepted || step.process != p || step.writes.isNotEmpty() || step.effects.isNotEmpty())
                    throw Violation("$event was taken beside ${p.session}")
            }
        }
    }

    /** An id is taken until the record of its retired generation is deleted: a start of it is refused and changes nothing. */
    private fun aTakenIdIsNotStartedAgain(w: World, c: Config) {
        val p = w.process ?: return
        val session = p.session ?: return
        for (r in w.durable.records.values) if (r.key.namespace == session.namespace) {
            val event = Start(r.key.copy(generation = "another"), Intent.Ordinary, c.requests, Conditions(c.api, w.visible))
            val step = Lifecycle.step(w.durable, p, event, c.limits)
            if (step.accepted || step.process != p || step.writes.isNotEmpty() || step.effects.isNotEmpty())
                throw Violation("$event was taken beside $r")
        }
    }

    /** Every event and fault the state admits. */
    fun moves(w: World, c: Config): List<Move> {
        val out = ArrayList<Move>()
        val faults = c.faults == null || w.faults < c.faults
        fun fault(label: String, world: World) {
            if (faults) out += Move(label, canonical(if (c.faults == null) world else world.copy(faults = w.faults + 1)), false)
        }
        fun act(label: String, background: Boolean = false, block: Sim.() -> Unit) {
            fun take(label: String, background: Boolean, starve: Boolean): World {
                val sim = Sim(w, c)
                sim.starve = starve
                try { sim.block() } catch (e: Violation) { throw Violation("$label: ${e.message}") }
                val world = canonical(sim.w)
                out += Move(label, world, background)
                sim.crashes.forEach { fault("$label, dying inside it", it) }
                return world
            }
            val world = take(label, background, starve = false)
            if (c.starved) {
                val before = out.size
                if (take("$label, with WorkManager refusing work", false, starve = true) == world) out.subList(before, out.size).clear()
            }
        }
        for (kind in c.reboots) fault("reboot that ${kind.name.lowercase()} user-initiated jobs", reboot(w, c, kind))
        fault("force-stop", forceStop(w, c))
        if (c.updates) {
            fault("update keeping jobs", update(w, c, drop = false))
            fault("update dropping jobs", update(w, c, drop = true))
        }
        val p = w.process
        if (p == null) {
            out += Move("the platform starts the process", canonical(boot(w, c)), !w.halted && (w.jobs.isNotEmpty() || w.receiver))
            out += Move("a person opens the app", canonical(boot(w, c).copy(visible = true)), false)
            return out
        }
        staleSessionEventsAreRefused(w, c)
        aTakenIdIsNotStartedAgain(w, c)
        fault("death", die(w, c))
        if (w.active.values.any { it.second == Job.UserJob }) fault("Task Manager stop", taskStop(w, c))
        if (w.limbo.isNotEmpty()) out += Move("WorkManager schedules again", canonical(w.copy(jobs = w.jobs + w.limbo, limbo = emptySet())), false)
        w.queue.firstOrNull()?.let { head -> act("the queue performs ${show(head)}", background = true) { dispatch() } }
        for (reply in w.replies)
            act("the platform answers ${reply.job} ${if (reply.accepted) "held" else "refused"} ${name(reply.key)}", background = true) { answer(reply) }
        val session = p.session
        if (!w.visible) out += Move("a person opens the app", w.copy(visible = true), false)
        else act("the activity leaves") {
            this.w = this.w.copy(visible = false)
            if (session?.view == true) feed(ViewDetached(session.namespace, session.incarnation))
        }
        for (namespace in c.namespaces) {
            if (namespace !in p.crossed) act("recovery of ${namespace.name}", background = true) { enter(namespace) }
            else if (session == null && w.visible) act("a session of ${namespace.name} starts") { feed(SessionStarted(namespace, LIVE)) }
        }
        if (session != null) {
            if (w.visible && !session.view) act("a view attaches") { feed(ViewAttached(session.namespace, session.incarnation, conditions)) }
            if (!session.view) act("the session ends", background = true) { feed(SessionEnded(session.namespace, session.incarnation)) }
            val taken = w.durable.records.values.filter { it.key.namespace == session.namespace }
            val made = w.made[session.namespace] ?: 0
            if (taken.isEmpty() && made < c.generations) for (person in c.persons) {
                val key = Key(session.namespace, "a", "g${made + 1}")
                act("a ${if (person) "person" else "program"} starts ${key.generation}") {
                    this.w = this.w.copy(asked = this.w.asked + key, made = this.w.made + (session.namespace to made + 1))
                    feed(Start(key, Lifecycle.intent(person && this.w.visible, conditions), c.requests, conditions))
                }
            }
            for (key in w.durable.records.keys) {
                if (!c.straight) act("abort ${name(key)}") { feed(Abort(key)) }
                act("retire ${name(key)}") { feed(Retire(key)) }
            }
        }
        for (task in w.tasks) {
            val key = task.op.key
            fun report(label: String, background: Boolean, event: Event) = act("$label ${name(key)}", background) { report(task, event) }
            // An operation that was not admitted only reports that; one that ran may also fail unless the search is straight.
            val fails = !task.started || !c.straight
            when (task.op.kind) {
                Kind.Keep -> {
                    if (task.started) report("credentials kept", true, CredentialsKept(key))
                    if (fails) report("credentials refused", !task.started, CredentialsRefused(key))
                }
                Kind.Copy -> {
                    if (task.started) report("snapshot committed", true, SnapshotCommitted(key))
                    if (fails) report("snapshot failed", !task.started, SnapshotFailed(key))
                }
                Kind.Place -> {
                    if (task.started) report("placed", true, Placed(key))
                    if (fails) report("placement failed", !task.started, PlaceFailed(key))
                }
                Kind.Exchange -> {
                    val attempt = task.op.attempt
                    if (task.started) for (response in c.responses)
                        report("response $response", response == Response.Ok, ResponseComplete(key, attempt, response))
                    for (failure in if (task.started) c.failures else listOf(Failure.Cancelled))
                        report("exchange $failure", !task.started, ExchangeFailed(key, attempt, failure))
                }
            }
        }
        for (key in w.cleans) {
            act("cleaned ${name(key)}", background = true) { cleaned(key) }
            if (!c.straight) act("cleanup failed ${name(key)}") { cleaned(key, removed = false) }
        }
        for ((key, delivery) in w.wakes) for (outcome in if (key in w.barred) listOf(Outcome.Superseded) else c.outcomes)
            act("wake $outcome ${name(key)}", background = outcome == Outcome.Completed || key in w.barred) {
                this.w = this.w.copy(wakes = this.w.wakes - key, barred = this.w.barred - key)
                feed(WakeEnded(key, delivery, outcome))
            }
        for (job in w.jobs) if (job !in w.active.values) act("${job.second} runs ${name(job.first)}", background = true) { run(job) }
        for ((id, job) in w.active) if (id !in w.halts) for (reason in c.stops)
            act("${job.second} stopped $reason ${name(job.first)}") { stop(id, reason) }
        return out
    }

    private fun name(key: Key) = "${key.namespace.name}:${key.generation}"

    private fun show(effect: Effect) = when (effect) {
        is Schedule -> "Schedule ${effect.job} ${name(effect.key)}"
        is Cancel -> "Cancel ${effect.job ?: "every job"} ${name(effect.key)}"
        is Finish -> "Finish ${if (effect.again) "to run again" else "for good"} ${name(effect.key)}"
        is Acknowledge -> "Acknowledge ${effect.namespace.name}"
        is Keep -> "Keep ${name(effect.key)}"; is Copy -> "Copy ${name(effect.key)}"; is Transmit -> "Transmit ${name(effect.key)}"
        is Place -> "Place ${name(effect.key)}"; is Raise -> "Raise ${name(effect.key)}"; is Clean -> "Clean ${name(effect.key)}"
        else -> effect.toString()
    }

    /** [edges] holds, for each state, its successors: twice the state's index, plus one for a background step. */
    class Graph(val states: List<World>, val edges: List<IntArray>)

    /** Every reachable state, each checked; a violation names the shortest path to it. */
    fun explore(c: Config, most: Int = 400_000): Graph {
        val index = HashMap<World, Int>()
        val states = ArrayList<World>()
        val parent = ArrayList<Pair<Int, String>>()
        val edges = ArrayList<IntArray>()
        fun path(state: Int, tail: String): String {
            val steps = ArrayList<String>()
            var at = state
            while (at > 0) { steps += parent[at].second; at = parent[at].first }
            return (steps.reversed() + tail).joinToString("\n  ", prefix = "\n  ")
        }
        // The states share their equal parts: the test JVM's heap is also Robolectric's.
        val pool = HashMap<Any, Any>()
        @Suppress("UNCHECKED_CAST") fun <T : Any> one(part: T): T = pool.getOrPut(part) { part } as T
        fun shared(w: World) = w.copy(durable = one(w.durable), process = w.process?.let(::one), jobs = one(w.jobs),
            limbo = one(w.limbo), active = one(w.active), halts = one(w.halts), lost = one(w.lost), restarted = one(w.restarted),
            queue = one(w.queue), replies = one(w.replies), tasks = one(w.tasks), cleans = one(w.cleans), wakes = one(w.wakes),
            barred = one(w.barred), asked = one(w.asked), doomed = one(w.doomed), tasked = one(w.tasked), demoted = one(w.demoted),
            made = one(w.made))
        index[World()] = 0; states += World(); parent += -1 to ""
        var next = 0
        while (next < states.size) {
            if (states.size > most) throw AssertionError("the search passed $most states: narrow $c")
            val w = states[next]
            val moves = try { moves(w, c) } catch (e: Violation) { throw AssertionError(e.message + path(next, "")) }
            val out = IntArray(moves.size)
            for ((i, move) in moves.withIndex()) {
                val known = index[move.world]
                val to = known ?: states.size.also {
                    val world = shared(move.world)
                    index[world] = it; states += world; parent += next to one(move.label)
                    try { check(world, c) } catch (e: Violation) { throw AssertionError(e.message + path(it, "")) }
                }
                out[i] = to * 2 + if (move.background) 1 else 0
            }
            edges += out
            next++
        }
        return Graph(states, edges)
    }

    /** The states from which some path reaches a goal, over every edge or only the ones no person makes. */
    fun reaching(graph: Graph, backgroundOnly: Boolean, goal: (World) -> Boolean): BooleanArray {
        val size = graph.states.size
        fun counts(edge: Int) = edge % 2 == 1 || !backgroundOnly
        // The predecessors of state i are back[first[i]] until back[first[i + 1]].
        val first = IntArray(size + 1)
        for (out in graph.edges) for (edge in out) if (counts(edge)) first[edge / 2 + 1]++
        for (i in 0 until size) first[i + 1] += first[i]
        val back = IntArray(first[size])
        val filled = first.copyOf()
        graph.edges.forEachIndexed { from, out -> for (edge in out) if (counts(edge)) back[filled[edge / 2]++] = from }
        val reached = BooleanArray(size)
        val queue = IntArray(size)
        var tail = 0
        graph.states.forEachIndexed { i, w -> if (goal(w)) { reached[i] = true; queue[tail++] = i } }
        var head = 0
        while (head < tail) {
            val to = queue[head++]
            for (at in first[to] until first[to + 1]) {
                val from = back[at]
                if (!reached[from]) { reached[from] = true; queue[tail++] = from }
            }
        }
        return reached
    }
}
