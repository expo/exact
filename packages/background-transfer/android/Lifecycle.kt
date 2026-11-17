package exact.backgroundtransfer

/**
 * The Android transfer lifecycle as one transition function (LLP 1133.017 D4, D5, D7, D10).
 *
 * [step] takes the persisted state, the process's volatile state and one event. It answers the
 * records to write, in order, then the effects to perform, in order. A caller holds one lock
 * across a step and its writes, then appends the effects to the process's one queue, which
 * performs them one at a time off the lock. Events arrive between any two effects.
 *
 * An effect that starts a long operation (keeping credentials, a copy, an exchange, a
 * placement) only starts it. The operation asks [admits] under the lock before it touches
 * anything, and reports back as an event exactly once: when it finished, failed, was cancelled,
 * or was not admitted.
 *
 * The process may die after any write or any effect, so every prefix is a state the next
 * process recovers from. Three orders make that hold. A platform job is recorded after the
 * platform accepts it and its release is recorded before the platform is told, so a record
 * that names a job always has one; a job that replaces another is recorded before the other is
 * released. A result, an attempt and a delivery are recorded before the bytes, the response or
 * the handler they permit. A registration's files are removed only after every operation that
 * could write them has reported.
 *
 * Nothing here names an Android type. The adapter turns platform callbacks into events and
 * effects into platform calls. The host owns the wake a [Raise] asks for.
 */
internal object Lifecycle {
    /**
     * Background deliveries after which a handler that completed without retiring its ending
     * has had its wakes. At [Limits.wakes] a completed wake is delivered again like an expired
     * one; at 1 the first completed wake leaves the ending to the launches.
     */
    const val COMPLETED_WAKES = 3

    /** One storage root and the scheduler names derived from it. An agent drive has its own. */
    data class Namespace(val name: String, val agent: Boolean = false)

    data class Key(val namespace: Namespace, val id: String, val generation: String)

    /**
     * Who started the transfer, written once. [Person] is a call inside a person's activation
     * window while the app is visible (API 31 or later); it has no execution deadline.
     * Everything else is [Ordinary].
     */
    enum class Intent { Person, Ordinary }

    /** The platform object that keeps the process: a user-initiated job, or WorkManager work. */
    enum class Job { UserJob, Work }

    enum class Phase {
        /** Recorded; credentials are being kept. No job, no bytes. */
        Admitting,
        /** Credentials kept and a job held; the upload snapshot is not committed. No bytes. */
        Preparing,
        /** Nothing is being sent; the held job sends when it runs. */
        Ready,
        /** One attempt holds the lease and may send. */
        Sending,
        /** The result is final. The ending is placed, then delivered until retired. */
        Ended,
        /** Retired by the app; credentials and files are being removed. */
        Retired,
    }

    enum class Result { Success, Aborted, BadStatus, FetchError, DownloadTotalExceeded }
    /** Who stopped a run: a [Person], or the platform for any reason of its own. */
    enum class Stop { Person, System }
    enum class Response { Ok, BadStatus, Invalid, TotalExceeded }
    enum class Failure { Network, Fatal, Cancelled }
    enum class Outcome { Completed, Expired, Superseded }

    /**
     * @property stops consecutive stops of work, with no response kept between them, that end a
     *   program's transfer and park a person's.
     * @property wakes background deliveries of one ending.
     */
    data class Limits(val stops: Int = 3, val networkRetries: Int = 5, val wakes: Int = 3,
                      val completedWakes: Int = COMPLETED_WAKES)
    data class Conditions(val api: Int, val visible: Boolean)

    /**
     * @property job the platform job held for this registration.
     * @property parked an unfinished transfer that holds no job until a launch with a view.
     * @property attempt counts attempts; in [Phase.Sending] it is the lease.
     * @property done requests whose responses are kept.
     * @property stops consecutive stops of work since the last kept response.
     * @property wakes background deliveries tried; at the limit the ending waits for a launch.
     * @property delivery counts deliveries; a wake is named by it.
     */
    data class Record(
        val key: Key, val intent: Intent, val requests: Int, val phase: Phase = Phase.Admitting,
        val job: Job? = null, val parked: Boolean = false, val attempt: Int = 0, val done: Int = 0, val stops: Int = 0,
        val retries: Int = 0, val result: Result? = null, val placed: Boolean = false, val wakes: Int = 0,
        val delivery: Int = 0)

    /** [stopping] names the namespaces that have yet to apply a force-stop; it is one file for the app. */
    data class Durable(val records: Map<Key, Record> = emptyMap(), val stopping: Set<Namespace> = emptySet()) {
        fun with(write: Write): Durable = when (write) {
            is Put -> copy(records = records + (write.record.key to write.record))
            is Delete -> copy(records = records - write.key)
            is Stopping -> copy(stopping = write.namespaces)
        }
    }

    /** [demoted] is a user-initiated job the platform runs without its standing (`isUserInitiatedJob` is false). */
    data class Run(val id: Int, val job: Job, val demoted: Boolean = false)
    /** [incarnation] is the host's, never reused in a process: it tells a session from an earlier one of its namespace. */
    data class Session(val namespace: Namespace, val incarnation: Int, val view: Boolean)

    data class Ask(val key: Key, val job: Job)
    /** The latest [Schedule] of its kind: its [token], and the job the record named when it was asked. */
    data class Pending(val token: Int, val from: Job?)

    enum class Kind { Keep, Copy, Exchange, Place }
    data class Op(val kind: Kind, val key: Key, val attempt: Int = 0)

    /**
     * What dies with the process.
     * @property crossed namespaces whose recovery this process has done; no other event is taken before it.
     * @property starting unanswered starts and the job each asks for.
     * @property draining operations cut short that have not reported.
     * @property waking deliveries whose outcome the host has not reported.
     * @property launch endings a launch with a view has yet to deliver.
     * @property asking schedules the platform has not answered.
     * @property cleaning retired registrations whose removal has not reported.
     */
    data class Process(
        val crossed: Set<Namespace> = emptySet(), val session: Session? = null,
        val runs: Map<Key, Run> = emptyMap(), val leases: Map<Key, Int> = emptyMap(),
        val starting: Map<Key, Job> = emptyMap(), val keeping: Set<Key> = emptySet(),
        val copying: Set<Key> = emptySet(), val placing: Set<Key> = emptySet(),
        val draining: Set<Op> = emptySet(), val waking: Map<Key, Int> = emptyMap(),
        val launch: Set<Key> = emptySet(), val asking: Map<Ask, Pending> = emptyMap(), val asks: Int = 0,
        val cleaning: Set<Key> = emptySet())

    sealed interface Event { val namespace: Namespace }
    sealed interface Keyed : Event { val key: Key; override val namespace: Namespace get() = key.namespace }

    /**
     * The recovery every entrant makes first: a worker, a job, the module, a receiver, a headless session.
     * @property stopped the entrant read evidence of a force-stop.
     * @property restarted the device rebooted or the package was updated since this namespace's last [Acknowledge].
     * @property held the registrations whose user-initiated job the platform holds, pending or running.
     */
    data class Enter(override val namespace: Namespace, val stopped: Boolean, val restarted: Boolean, val held: Set<Key>,
                     val conditions: Conditions) : Event
    data class SessionStarted(override val namespace: Namespace, val incarnation: Int) : Event
    /** A launch with a view: an activity shows the session. */
    data class ViewAttached(override val namespace: Namespace, val incarnation: Int, val conditions: Conditions) : Event
    data class ViewDetached(override val namespace: Namespace, val incarnation: Int) : Event
    data class SessionEnded(override val namespace: Namespace, val incarnation: Int) : Event
    data class Start(override val key: Key, val intent: Intent, val requests: Int, val conditions: Conditions) : Keyed
    data class CredentialsKept(override val key: Key) : Keyed
    data class CredentialsRefused(override val key: Key) : Keyed
    data class Scheduled(override val key: Key, val job: Job, val token: Int) : Keyed
    data class ScheduleRefused(override val key: Key, val job: Job, val token: Int) : Keyed
    data class SnapshotCommitted(override val key: Key) : Keyed
    data class SnapshotFailed(override val key: Key) : Keyed
    /** [conditions] are those the run starts under. */
    data class JobStarted(override val key: Key, val run: Run, val conditions: Conditions) : Keyed
    data class JobStopped(override val key: Key, val run: Int, val reason: Stop) : Keyed
    data class ResponseComplete(override val key: Key, val attempt: Int, val response: Response) : Keyed
    data class ExchangeFailed(override val key: Key, val attempt: Int, val failure: Failure) : Keyed
    data class Abort(override val key: Key) : Keyed
    /** Of a registration retired already, it asks again for the removal of what an earlier one left. */
    data class Retire(override val key: Key) : Keyed
    data class Cleaned(override val key: Key) : Keyed
    /** The removal left something: the record stays retired, its id taken, until a removal succeeds. */
    data class CleanFailed(override val key: Key) : Keyed
    data class Placed(override val key: Key) : Keyed
    data class PlaceFailed(override val key: Key) : Keyed
    data class WakeEnded(override val key: Key, val delivery: Int, val outcome: Outcome) : Keyed

    sealed interface Write
    data class Put(val record: Record) : Write
    data class Delete(val key: Key) : Write
    data class Stopping(val namespaces: Set<Namespace>) : Write

    sealed interface Effect
    data class Keep(val key: Key) : Effect
    /**
     * Hold a job of this kind. Work the platform already holds is kept; a user-initiated job is
     * scheduled afresh. The answer, [Scheduled] or [ScheduleRefused], carries the [token].
     */
    data class Schedule(val key: Key, val job: Job, val token: Int) : Effect
    /** Cancel the platform's jobs for the key: of one kind, or of every kind. */
    data class Cancel(val key: Key, val job: Job?) : Effect
    data class Copy(val key: Key) : Effect
    data class CancelCopy(val key: Key) : Effect
    data class Answer(val key: Key, val registered: Boolean) : Effect
    data class Transmit(val key: Key, val attempt: Int, val request: Int) : Effect
    data class CancelIo(val key: Key, val attempt: Int) : Effect
    /** [again] is a placement after a failed one: the placer pauses before it tries. */
    data class Place(val key: Key, val again: Boolean) : Effect
    /** The host raises the ending in the namespace's session, booting one with no view when none is live. */
    data class Raise(val key: Key, val delivery: Int) : Effect
    data class Expire(val key: Key, val delivery: Int) : Effect
    /** The run ends. [again] asks the platform to run its job again. */
    data class Finish(val key: Key, val run: Int, val again: Boolean) : Effect
    data class Promote(val key: Key, val run: Int) : Effect
    data class Clean(val key: Key) : Effect
    /** The recovery is recorded: the adapter may rearm the evidence of a force-stop and of a restart. */
    data class Acknowledge(val namespace: Namespace) : Effect

    /** [accepted] is false for an event the state does not admit; such a step writes nothing. */
    data class Step(val durable: Durable, val process: Process, val writes: List<Write>, val effects: List<Effect>,
                    val accepted: Boolean)

    fun intent(person: Boolean, conditions: Conditions): Intent =
        if (person && conditions.visible && conditions.api >= 31) Intent.Person
        else Intent.Ordinary

    /** The job a registration may hold now. A person's transfer holds work only as a bridge. */
    fun entitled(intent: Intent, conditions: Conditions): Job =
        if (intent == Intent.Person && conditions.visible && conditions.api >= 34) Job.UserJob
        else Job.Work

    /**
     * Whether a work run asks for foreground execution: where no user-initiated job exists. From
     * API 35 a `dataSync` foreground service is capped, so the bridge there is ordinary work. A
     * run is asked when it starts under a view, or at the next launch with one.
     */
    fun promotes(intent: Intent, conditions: Conditions): Boolean =
        intent == Intent.Person && conditions.api in 31..34

    /**
     * Whether an operation or a schedule may start now. One that may not touches nothing; an
     * operation still reports, as failed or cancelled.
     */
    fun admits(durable: Durable, process: Process, effect: Effect): Boolean = when (effect) {
        is Keep -> effect.key in process.keeping
        is Schedule -> process.asking[Ask(effect.key, effect.job)]?.token == effect.token
        is Copy -> effect.key in process.copying
        is Transmit -> process.leases[effect.key] == effect.attempt &&
            durable.records[effect.key].let { it != null && it.phase == Phase.Sending && it.attempt == effect.attempt }
        is Place -> effect.key in process.placing
        else -> true
    }

    fun step(durable: Durable, process: Process, event: Event, limits: Limits = Limits()): Step =
        Turn(durable, process, limits).also { it.on(event) }.step()

    private class Turn(var durable: Durable, var process: Process, val limits: Limits) {
        val writes = ArrayList<Write>()
        val effects = ArrayList<Effect>()
        var accepted = true

        fun step() = Step(durable, process, writes, effects, accepted)

        private fun write(write: Write) { writes += write; durable = durable.with(write) }
        private fun put(record: Record) { if (durable.records[record.key] != record) write(Put(record)) }
        private fun emit(effect: Effect) { effects += effect }
        private fun get(key: Key) = durable.records[key]
        private fun records(namespace: Namespace) = durable.records.values.filter { it.key.namespace == namespace }
            .sortedWith(compareBy({ it.key.id }, { it.key.generation }))
        private fun mine(key: Key) = process.session?.namespace == key.namespace
        private fun live(namespace: Namespace, incarnation: Int) =
            process.session.let { it != null && it.namespace == namespace && it.incarnation == incarnation }
        private fun unfinished(r: Record) = r.phase == Phase.Preparing || r.phase == Phase.Ready || r.phase == Phase.Sending
        private fun owed(r: Record) = r.phase == Phase.Ended && r.wakes < limits.wakes
        /** Whether the registration has a use for a platform job. */
        private fun wanted(r: Record) = unfinished(r) || owed(r) || (r.phase == Phase.Admitting && r.key in process.starting)
        private fun refuse() { accepted = false }

        fun on(event: Event) {
            if (event is Enter) return enter(event)
            if (event.namespace !in process.crossed) {
                refuse()
                if (event is JobStarted) emit(Finish(event.key, event.run.id, again = true))
                return
            }
            when (event) {
                is Enter -> {}
                // One session a process: the host ends one before it starts the next.
                is SessionStarted -> if (process.session != null) refuse()
                    else process = process.copy(session = Session(event.namespace, event.incarnation, view = false))
                is ViewAttached -> viewAttached(event)
                is ViewDetached -> if (!live(event.namespace, event.incarnation)) refuse()
                    else process = process.copy(session = process.session?.copy(view = false), launch = emptySet())
                is SessionEnded -> if (!live(event.namespace, event.incarnation)) refuse()
                    else process = process.copy(session = null, launch = emptySet())
                is Start -> start(event)
                is CredentialsKept -> credentials(event.key, kept = true)
                is CredentialsRefused -> credentials(event.key, kept = false)
                is Scheduled -> answered(event.key, event.job, event.token, accepted = true)
                is ScheduleRefused -> answered(event.key, event.job, event.token, accepted = false)
                is SnapshotCommitted -> snapshot(event.key, committed = true)
                is SnapshotFailed -> snapshot(event.key, committed = false)
                is JobStarted -> jobStarted(event)
                is JobStopped -> jobStopped(event)
                is ResponseComplete -> responseComplete(event)
                is ExchangeFailed -> exchangeFailed(event)
                is Abort -> {
                    val r = get(event.key)
                    if (!mine(event.key) || r == null || r.phase == Phase.Ended || r.phase == Phase.Retired) refuse()
                    else end(r, Result.Aborted)
                }
                is Retire -> {
                    val r = get(event.key)
                    if (!mine(event.key) || r == null) refuse() else if (r.phase == Phase.Retired) clean(event.key) else retire(r)
                }
                is Cleaned -> if (event.key !in process.cleaning) refuse() else {
                    process = process.copy(cleaning = process.cleaning - event.key)
                    write(Delete(event.key))
                }
                is CleanFailed -> if (event.key !in process.cleaning) refuse()
                    else process = process.copy(cleaning = process.cleaning - event.key)
                is Placed -> placement(event.key, placed = true)
                is PlaceFailed -> placement(event.key, placed = false)
                is WakeEnded -> wakeEnded(event)
            }
        }

        private fun enter(e: Enter) {
            val namespace = e.namespace
            if (namespace in process.crossed) return
            // The evidence is the app's, and this entrant rearms it: every namespace is marked first.
            if (e.stopped) write(Stopping(durable.stopping + durable.records.keys.map { it.namespace } + namespace))
            val stopped = namespace in durable.stopping
            for (r in records(namespace)) {
                // A user-initiated job gone with no reboot or update to explain it is one a person stopped.
                val gone = stopped || (r.job == Job.UserJob && r.key !in e.held && !e.restarted)
                when (r.phase) {
                    Phase.Retired -> clean(r.key)
                    Phase.Admitting -> {
                        // A start no process finished: its credentials are incomplete and nobody was answered.
                        put(r.copy(phase = Phase.Retired, job = null))
                        emit(Cancel(r.key, null))
                        clean(r.key)
                    }
                    Phase.Preparing, Phase.Ready, Phase.Sending -> if (gone) {
                        put(r.copy(phase = Phase.Ended, result = Result.Aborted, job = null, parked = false, wakes = limits.wakes))
                        emit(Cancel(r.key, null))
                    } else {
                        val waiting = if (r.phase == Phase.Sending) r.copy(phase = Phase.Ready) else r
                        put(waiting)
                        hold(waiting, e)
                        if (waiting.phase == Phase.Preparing) copy(r.key)
                    }
                    Phase.Ended -> if (gone || !owed(r)) {
                        if (r.job != null || r.wakes < limits.wakes) put(r.copy(job = null, wakes = limits.wakes))
                        if (stopped || r.job != null) emit(Cancel(r.key, null))
                    } else hold(r, e)
                }
            }
            if (stopped) write(Stopping(durable.stopping - namespace))
            emit(Acknowledge(namespace))
            process = process.copy(crossed = process.crossed + namespace)
        }

        /** Hold a job again for a registration that owes work. A parked one waits for a launch with a view. */
        private fun hold(r: Record, e: Enter) {
            when {
                r.job == Job.UserJob && r.key in e.held -> {}
                // A process that died before the platform's answer left a job the record does not name.
                r.job == null && r.key in e.held -> put(r.copy(job = Job.UserJob, parked = false, stops = if (r.parked) 0 else r.stops))
                r.parked -> {}
                r.job == Job.Work -> ask(r, Job.Work)
                else -> {
                    // The loss is recorded before the evidence of the restart is rearmed: a later entrant must not read it as a person's stop.
                    val bare = r.copy(job = null).also(::put)
                    ask(bare, if (r.phase == Phase.Ended) Job.Work else entitled(r.intent, e.conditions))
                }
            }
        }

        private fun ask(r: Record, job: Job) {
            val token = process.asks + 1
            process = process.copy(asks = token, asking = process.asking + (Ask(r.key, job) to Pending(token, r.job)))
            emit(Schedule(r.key, job, token))
        }

        private fun viewAttached(e: ViewAttached) {
            val session = process.session
            if (session == null || !live(e.namespace, e.incarnation)) return refuse()
            process = process.copy(session = session.copy(view = true))
            for (r in records(e.namespace)) {
                val run = process.runs[r.key]
                val entitled = entitled(r.intent, e.conditions)
                when {
                    r.phase == Phase.Ended -> {
                        if (r.key !in process.waking) process = process.copy(launch = process.launch + r.key)
                        settle(r.key)
                    }
                    !unfinished(r) -> {}
                    r.parked -> ask(r, entitled)
                    r.intent != Intent.Person || r.job != Job.Work -> {}
                    // A running bridge is not restarted.
                    run == null -> if (entitled == Job.UserJob) ask(r, Job.UserJob)
                    promotes(r.intent, e.conditions) -> emit(Promote(r.key, run.id))
                }
            }
        }

        private fun start(e: Start) {
            // Kept responses are published under the id (D3), so it is free only when its retired generation's files are gone.
            val taken = records(e.namespace).any { it.key.id == e.key.id }
            if (!mine(e.key) || taken || e.requests < 1) return refuse()
            put(Record(e.key, e.intent, e.requests))
            process = process.copy(starting = process.starting + (e.key to entitled(e.intent, e.conditions)),
                keeping = process.keeping + e.key)
            emit(Keep(e.key))
        }

        private fun credentials(key: Key, kept: Boolean) {
            if (key !in process.keeping) { if (!drained(Op(Kind.Keep, key))) refuse(); return }
            process = process.copy(keeping = process.keeping - key)
            val r = get(key)
            val job = process.starting[key]
            if (r == null || r.phase != Phase.Admitting || job == null) return
            if (kept) ask(r, job) else retire(r)
        }

        private fun answered(key: Key, job: Job, token: Int, accepted: Boolean) {
            val ask = Ask(key, job)
            val pending = process.asking[ask]
            // The answer to a schedule that a later one of its kind replaced.
            if (pending?.token != token) return refuse()
            process = process.copy(asking = process.asking - ask)
            if (accepted) accept(key, job, pending.from) else refused(key, job, pending.from)
        }

        /** The platform holds [job]. It is recorded, and only then is the job it replaces released. */
        private fun accept(key: Key, job: Job, from: Job?) {
            val r = get(key)
            when {
                r == null -> emit(Cancel(key, job))
                r.job == job -> {}
                !wanted(r) -> emit(Cancel(key, job))
                // The record moved on while the platform answered, or a run of the job held is attached and carries on.
                r.job != from || key in process.runs -> emit(Cancel(key, job))
                r.phase == Phase.Admitting -> { put(r.copy(phase = Phase.Preparing, job = job)); copy(key) }
                else -> {
                    put(r.copy(job = job, parked = false, stops = if (r.parked) 0 else r.stops))
                    if (r.job != null) emit(Cancel(key, r.job))
                }
            }
        }

        private fun refused(key: Key, job: Job, from: Job?) {
            val r = get(key)
            if (r == null || !wanted(r) || (r.job != from && r.job != job)) return
            when {
                // Work carries the transfer until a launch can take the job, on Android 15 too.
                job == Job.UserJob -> if (r.job != Job.Work) ask(r, Job.Work)
                // WorkManager could not store the work: a start still unanswered fails.
                r.phase == Phase.Admitting -> retire(r)
                // The job held stays. A user-initiated job runs again and asks again.
                r.job == Job.UserJob || key in process.runs -> {}
                unfinished(r) -> put(r.copy(job = null, parked = true))
                else -> put(r.copy(job = null, wakes = limits.wakes))
            }
        }

        private fun copy(key: Key) {
            if (key in process.copying) return
            process = process.copy(copying = process.copying + key)
            emit(Copy(key))
        }

        private fun snapshot(key: Key, committed: Boolean) {
            if (key !in process.copying) { if (!drained(Op(Kind.Copy, key))) refuse(); return }
            process = process.copy(copying = process.copying - key)
            val r = get(key)
            if (r?.phase != Phase.Preparing) return
            if (committed) {
                val ready = r.copy(phase = Phase.Ready)
                answer(key, true)
                if (key in process.runs) begin(ready) else put(ready)
            } else if (key in process.starting) retire(r) else end(r, Result.FetchError)
        }

        private fun answer(key: Key, registered: Boolean) {
            if (key !in process.starting) return
            emit(Answer(key, registered))
            process = process.copy(starting = process.starting - key)
        }

        private fun begin(r: Record) {
            // A cancelled attempt has yet to report: its report begins this one.
            if (process.draining.any { it.kind == Kind.Exchange && it.key == r.key }) return put(r.copy(phase = Phase.Ready))
            val attempt = r.attempt + 1
            put(r.copy(phase = Phase.Sending, attempt = attempt))
            process = process.copy(leases = process.leases + (r.key to attempt))
            emit(Transmit(r.key, attempt, r.done))
        }

        private fun jobStarted(e: JobStarted) {
            val key = e.key
            val id = e.run.id
            // A run that came before the answer to its schedule is not this step's to judge: it runs again after the answer.
            val unanswered = Ask(key, e.run.job) in process.asking
            if (process.runs[key] != null) return emit(Finish(key, id, again = unanswered))
            // An agent drive's transfer runs only beside its session; its job is kept for then.
            if (key.namespace.agent && !mine(key)) { refuse(); return emit(Finish(key, id, again = true)) }
            val r = get(key)
            // A run of a job the record does not name is one a death or a handoff left behind.
            if (r == null || r.phase == Phase.Retired || r.job != e.run.job) return emit(Finish(key, id, again = unanswered))
            if (e.run.demoted) {
                // Work carries the transfer and a launch takes the job back. The job stays until the work is accepted.
                ask(r, Job.Work)
                return emit(Finish(key, id, again = true))
            }
            process = process.copy(runs = process.runs + (key to e.run))
            // A foreground service starts only under a view; work that started without one is asked at the launch.
            val shown = process.session.let { it != null && it.namespace == key.namespace && it.view }
            if (e.run.job == Job.Work && unfinished(r) && shown && promotes(r.intent, e.conditions)) emit(Promote(key, id))
            when (r.phase) {
                Phase.Admitting, Phase.Retired -> {}
                Phase.Preparing -> copy(key)
                Phase.Ready, Phase.Sending -> begin(r)
                Phase.Ended -> settle(key)
            }
        }

        private fun jobStopped(e: JobStopped) {
            val key = e.key
            val run = process.runs[key]
            if (run?.id != e.run) {
                // A person's stop reaches the registration though its run had been let go.
                val r = get(key)
                if (e.reason != Stop.Person || r == null || r.phase == Phase.Retired || (r.phase == Phase.Ended && r.job == null)) return refuse()
                if (r.phase == Phase.Ended) put(r.copy(job = null, wakes = limits.wakes)) else end(r.copy(job = null), Result.Aborted)
                return emit(Cancel(key, null))
            }
            process.waking[key]?.let { emit(Expire(key, it)) }
            process = process.copy(runs = process.runs - key)
            val r = get(key)
            if (r != null && r.phase != Phase.Retired) when {
                e.reason == Stop.Person ->
                    // A person stopped it: nothing runs for it again until they open the app.
                    if (r.phase == Phase.Ended) put(r.copy(job = null, wakes = limits.wakes))
                    else end(r.copy(job = null), Result.Aborted)
                r.phase == Phase.Ended -> if (!owed(r)) put(r.copy(job = null))
                else -> {
                    release(key)
                    // Every stop of work counts. A user-initiated job has no deadline, and a person sees it and can stop it.
                    val stops = if (run.job == Job.Work) r.stops + 1 else r.stops
                    val waiting = r.copy(phase = if (r.phase == Phase.Sending) Phase.Ready else r.phase, stops = stops)
                    when {
                        stops < limits.stops -> put(waiting)
                        r.intent == Intent.Ordinary -> end(waiting, Result.FetchError)
                        else -> put(waiting.copy(job = null, parked = true))
                    }
                }
            }
            val again = get(key)?.job != null
            emit(Finish(key, e.run, again))
            // WorkManager schedules a stopped worker again whatever it answers.
            if (!again) emit(Cancel(key, run.job))
        }

        /** The attempt loses its lease. It is counted until it reports, so the next one never overlaps it. */
        private fun release(key: Key) {
            val lease = process.leases[key] ?: return
            emit(CancelIo(key, lease))
            process = process.copy(leases = process.leases - key, draining = process.draining + Op(Kind.Exchange, key, lease))
        }

        private fun responseComplete(e: ResponseComplete) {
            val r = leased(e.key, e.attempt)
            if (r == null) { if (!drained(Op(Kind.Exchange, e.key, e.attempt))) refuse(); return }
            process = process.copy(leases = process.leases - e.key)
            // A kept response is progress: the stops before it no longer count.
            val kept = r.copy(done = r.done + 1, retries = 0, stops = 0)
            when (e.response) {
                Response.Ok -> if (kept.done < r.requests) begin(kept) else end(kept, Result.Success)
                Response.BadStatus -> end(kept, Result.BadStatus)
                Response.Invalid -> end(r, Result.FetchError)
                Response.TotalExceeded -> end(r, Result.DownloadTotalExceeded)
            }
        }

        private fun exchangeFailed(e: ExchangeFailed) {
            val r = leased(e.key, e.attempt)
            if (r == null) { if (!drained(Op(Kind.Exchange, e.key, e.attempt))) refuse(); return }
            process = process.copy(leases = process.leases - e.key)
            val retries = if (e.failure == Failure.Network) r.retries + 1 else r.retries
            if (e.failure == Failure.Fatal || retries >= limits.networkRetries) return end(r.copy(retries = retries), Result.FetchError)
            put(r.copy(phase = Phase.Ready, retries = retries))
            finish(e.key, again = true)
        }

        /** The record, when this attempt holds its lease in this process. */
        private fun leased(key: Key, attempt: Int): Record? =
            get(key)?.takeIf { it.phase == Phase.Sending && it.attempt == attempt && process.leases[key] == attempt }

        /** Operations cut short are counted until they report: nothing of theirs is removed before. */
        private fun abandon(key: Key) {
            var draining = process.draining
            if (key in process.keeping) draining = draining + Op(Kind.Keep, key)
            if (key in process.copying) { emit(CancelCopy(key)); draining = draining + Op(Kind.Copy, key) }
            if (key in process.placing) draining = draining + Op(Kind.Place, key)
            process = process.copy(keeping = process.keeping - key, copying = process.copying - key,
                placing = process.placing - key, draining = draining)
        }

        /** The report of an operation that was cut short. It ends the wait for it and decides nothing else. */
        private fun drained(op: Op): Boolean {
            if (op !in process.draining) return false
            process = process.copy(draining = process.draining - op)
            val r = get(op.key) ?: return true
            if (r.phase == Phase.Retired) clean(op.key)
            else if (op.kind == Kind.Exchange && r.phase == Phase.Ready && op.key in process.runs) begin(r)
            return true
        }

        /** One removal at a time, and none while an operation may still write: what a registration kept is under its id. */
        private fun clean(key: Key) {
            if (key in process.cleaning || process.draining.any { it.key == key }) return
            emit(Clean(key))
            process = process.copy(cleaning = process.cleaning + key)
        }

        private fun end(r: Record, result: Result) {
            val key = r.key
            release(key)
            abandon(key)
            // With no job, only a launch with a view delivers the ending.
            put(r.copy(phase = Phase.Ended, result = result, parked = false, wakes = if (r.job == null) limits.wakes else r.wakes))
            answer(key, true)
            val session = process.session
            if (session != null && session.namespace == key.namespace && session.view && key !in process.runs)
                process = process.copy(launch = process.launch + key)
            settle(key)
        }

        /** Move an ending toward its delivery: place it, then raise it, held by a run or shown by a launch. */
        private fun settle(key: Key) {
            val r = get(key) ?: return
            if (r.phase != Phase.Ended || key in process.waking || key in process.placing) return
            val run = process.runs[key]
            val held = run != null && owed(r)
            val launch = key in process.launch
            val session = process.session
            when {
                !held && !launch -> if (run != null) { put(r.copy(job = null)); finish(key, again = false) }
                // One session a process: the job runs again, on the platform's backoff, until its own session is the live one.
                session != null && session.namespace != key.namespace -> finish(key, again = true)
                !r.placed -> place(key, again = false)
                else -> {
                    val raised = r.copy(delivery = r.delivery + 1, wakes = if (held) r.wakes + 1 else r.wakes)
                    put(raised)
                    process = process.copy(waking = process.waking + (key to raised.delivery), launch = process.launch - key)
                    emit(Raise(key, raised.delivery))
                }
            }
        }

        private fun place(key: Key, again: Boolean) {
            process = process.copy(placing = process.placing + key)
            emit(Place(key, again))
        }

        private fun placement(key: Key, placed: Boolean) {
            if (key !in process.placing) { if (!drained(Op(Kind.Place, key))) refuse(); return }
            process = process.copy(placing = process.placing - key)
            val r = get(key)
            if (r?.phase != Phase.Ended) return
            when {
                placed -> { put(r.copy(placed = true)); settle(key) }
                // The view is still up: the placer tries again.
                key in process.launch -> place(key, again = true)
                key !in process.runs -> {}
                else -> {
                    // A failed placement spends a background delivery, so files that cannot be placed end at the launches.
                    val spent = r.copy(wakes = minOf(r.wakes + 1, limits.wakes))
                    if (owed(spent)) put(spent) else put(spent.copy(job = null))
                    finish(key, again = owed(spent))
                }
            }
        }

        private fun wakeEnded(e: WakeEnded) {
            val key = e.key
            if (process.waking[key] != e.delivery) return refuse()
            process = process.copy(waking = process.waking - key)
            val r = get(key)
            val run = process.runs[key]
            when {
                // Retired inside its own wake.
                r == null || r.phase != Phase.Ended -> finish(key, again = false)
                !owed(r) || (e.outcome == Outcome.Completed && r.wakes >= limits.completedWakes) -> {
                    // Its background deliveries are spent: the next is a launch with a view.
                    put(r.copy(job = null, wakes = limits.wakes))
                    if (run != null) finish(key, again = false) else if (r.job != null) emit(Cancel(key, r.job))
                }
                // A launch's delivery spends nothing, and a run that was stopped runs again by itself.
                run == null -> {}
                else -> {
                    // Work delivers again. A user-initiated job stays until that work is accepted, then ends with its notification.
                    if (run.job == Job.UserJob) ask(r, Job.Work)
                    finish(key, again = true)
                }
            }
        }

        private fun retire(r: Record) {
            val key = r.key
            put(r.copy(phase = Phase.Retired, job = null, parked = false))
            release(key)
            abandon(key)
            // A handler that retires inside its own wake keeps its run until the wake's outcome.
            if (key in process.runs) { if (key !in process.waking) finish(key, again = false) }
            else if (r.job != null) emit(Cancel(key, r.job))
            answer(key, false)
            process = process.copy(launch = process.launch - key)
            clean(key)
        }

        private fun finish(key: Key, again: Boolean) {
            val run = process.runs[key] ?: return
            emit(Finish(key, run.id, again))
            process = process.copy(runs = process.runs - key)
        }
    }
}
