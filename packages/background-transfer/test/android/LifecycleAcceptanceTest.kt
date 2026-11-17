package exact.backgroundtransfer

import exact.backgroundtransfer.Lifecycle.Abort
import exact.backgroundtransfer.Lifecycle.Answer
import exact.backgroundtransfer.Lifecycle.Cancel
import exact.backgroundtransfer.Lifecycle.CancelCopy
import exact.backgroundtransfer.Lifecycle.CancelIo
import exact.backgroundtransfer.Lifecycle.Clean
import exact.backgroundtransfer.Lifecycle.Cleaned
import exact.backgroundtransfer.Lifecycle.Conditions
import exact.backgroundtransfer.Lifecycle.Copy
import exact.backgroundtransfer.Lifecycle.CredentialsKept
import exact.backgroundtransfer.Lifecycle.Effect
import exact.backgroundtransfer.Lifecycle.Event
import exact.backgroundtransfer.Lifecycle.ExchangeFailed
import exact.backgroundtransfer.Lifecycle.Failure
import exact.backgroundtransfer.Lifecycle.Finish
import exact.backgroundtransfer.Lifecycle.Intent
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.JobStarted
import exact.backgroundtransfer.Lifecycle.JobStopped
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Kind
import exact.backgroundtransfer.Lifecycle.Limits
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Op
import exact.backgroundtransfer.Lifecycle.Outcome
import exact.backgroundtransfer.Lifecycle.Phase
import exact.backgroundtransfer.Lifecycle.Place
import exact.backgroundtransfer.Lifecycle.PlaceFailed
import exact.backgroundtransfer.Lifecycle.Placed
import exact.backgroundtransfer.Lifecycle.Promote
import exact.backgroundtransfer.Lifecycle.Raise
import exact.backgroundtransfer.Lifecycle.Response
import exact.backgroundtransfer.Lifecycle.ResponseComplete
import exact.backgroundtransfer.Lifecycle.Result
import exact.backgroundtransfer.Lifecycle.Retire
import exact.backgroundtransfer.Lifecycle.Run
import exact.backgroundtransfer.Lifecycle.Schedule
import exact.backgroundtransfer.Lifecycle.Scheduled
import exact.backgroundtransfer.Lifecycle.Session
import exact.backgroundtransfer.Lifecycle.SessionEnded
import exact.backgroundtransfer.Lifecycle.SessionStarted
import exact.backgroundtransfer.Lifecycle.SnapshotCommitted
import exact.backgroundtransfer.Lifecycle.SnapshotFailed
import exact.backgroundtransfer.Lifecycle.Start
import exact.backgroundtransfer.Lifecycle.Step
import exact.backgroundtransfer.Lifecycle.Stop
import exact.backgroundtransfer.Lifecycle.Transmit
import exact.backgroundtransfer.Lifecycle.ViewAttached
import exact.backgroundtransfer.Lifecycle.ViewDetached
import exact.backgroundtransfer.Lifecycle.WakeEnded
import exact.backgroundtransfer.LifecycleModel.Config
import exact.backgroundtransfer.LifecycleModel.LIVE
import exact.backgroundtransfer.LifecycleModel.Reboot
import exact.backgroundtransfer.LifecycleModel.Sim
import exact.backgroundtransfer.LifecycleModel.Task
import exact.backgroundtransfer.LifecycleModel.World
import exact.backgroundtransfer.LifecycleModel.app
import exact.backgroundtransfer.LifecycleModel.drive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The behaviour the lifecycle must have, a case each. Each case runs twice:
 * [LifecycleAcceptanceTest] drives [Lifecycle.step] through the model alone, and
 * [LifecycleAdapterTest] drives the adapter, where the steps are [Transfers]', the records are
 * read back from the disk and the operations are the adapter's own.
 */
abstract class LifecycleAcceptance(private val adapted: Boolean) {
    private val roots = ArrayList<java.io.File>()
    @org.junit.After fun cleanup() { roots.forEach { it.deleteRecursively() } }

    /** A generation the store accepts as a directory name. */
    private fun generation(n: Int) = "00000000-0000-4000-8000-00000000000$n"

    private inner class Scene(val c: Config = Config()) {
        val sim = Sim(World(), c)
        private val adapter = if (!adapted) null else
            LifecycleAdapter(sim, java.nio.file.Files.createTempDirectory("transfer-lifecycle-").toFile().also { roots += it }).also { sim.backend = it }
        val a = Key(app, "a", generation(1))
        private var mark = 0
        /** Whether each step's effects are performed and each schedule answered before the case goes on. */
        var prompt = true

        val w get() = sim.w
        val conditions get() = sim.conditions
        fun record(key: Key = a) = sim.record(key)!!
        /** The effects performed since the last call. */
        fun effects(): List<Effect> = sim.effects.subList(mark, sim.effects.size).toList().also { mark = sim.effects.size }
        /** Perform every queued effect; the platform's answers stay undelivered. */
        fun perform() { while (w.queue.isNotEmpty()) sim.dispatch() }
        fun answer(job: Job, key: Key = a): Step = after(sim.answer(w.replies.single { it.key == key && it.job == job }))
        private fun after(step: Step): Step {
            if (prompt) while (true) {
                perform()
                sim.answer(w.replies.minByOrNull { it.token } ?: break)
            }
            LifecycleModel.check(sim.w, c)
            return step
        }
        fun feed(event: Event): Step = after(sim.feed(event))
        fun set(world: World) {
            adapter?.let {
                if (world.process == null) it.die() else if (w.process == null) it.boot()
                assertEquals(it.durable(), world.durable)
            }
            sim.w = world
            LifecycleModel.check(world, c)
        }
        /** The process dies after [writes] writes of the step [block] takes. */
        fun dying(writes: Int, block: () -> Unit) {
            val before = sim.crashes.size
            if (adapter == null) {
                block()
                return set(sim.crashes[before + writes])
            }
            adapter.dying(writes)
            try { block(); throw AssertionError("the step made fewer than ${writes + 1} writes") } catch (_: LifecycleAdapter.Death) {}
            set(LifecycleModel.die(w.copy(durable = adapter.durable()), c))
        }

        fun boot() { if (w.process == null) set(LifecycleModel.boot(w, c)) }
        fun enter(namespace: Namespace = app): Step { boot(); return after(sim.enter(namespace)) }
        fun attach(namespace: Namespace = app): Step { set(w.copy(visible = true)); return feed(ViewAttached(namespace, LIVE, conditions)) }
        fun open(namespace: Namespace = app) {
            boot()
            set(w.copy(visible = true))
            if (namespace !in w.process!!.crossed) enter(namespace)
            if (w.process!!.session == null) feed(SessionStarted(namespace, LIVE))
            attach(namespace)
        }
        fun leave() { set(w.copy(visible = false)); feed(ViewDetached(w.process!!.session!!.namespace, LIVE)) }
        fun start(person: Boolean = false, key: Key = a, requests: Int = 1): Step {
            sim.w = w.copy(asked = w.asked + key)
            return feed(Start(key, Lifecycle.intent(person, conditions), requests, conditions))
        }
        private fun task(kind: Kind, key: Key) = w.tasks.single { it.op.kind == kind && it.op.key == key }
        fun kept(key: Key = a): Step = after(sim.report(task(Kind.Keep, key), CredentialsKept(key)))
        fun committed(key: Key = a): Step = after(sim.report(task(Kind.Copy, key), SnapshotCommitted(key)))
        fun ready(person: Boolean = false, key: Key = a, requests: Int = 1) { start(person, key, requests); kept(key); committed(key) }
        fun run(key: Key = a, job: Job? = null): Step =
            after(sim.run(w.jobs.single { it.first == key && it !in w.active.values && (job == null || it.second == job) }))
        fun respond(response: Response = Response.Ok, key: Key = a): Step =
            task(Kind.Exchange, key).let { after(sim.report(it, ResponseComplete(key, it.op.attempt, response))) }
        fun fail(failure: Failure, key: Key = a): Step =
            task(Kind.Exchange, key).let { after(sim.report(it, ExchangeFailed(key, it.op.attempt, failure))) }
        fun placed(key: Key = a): Step = after(sim.report(task(Kind.Place, key), Placed(key)))
        fun unplaced(key: Key = a): Step = after(sim.report(task(Kind.Place, key), PlaceFailed(key)))
        fun woke(outcome: Outcome = Outcome.Completed, key: Key = a): Step {
            val delivery = w.wakes.getValue(key)
            sim.w = w.copy(wakes = w.wakes - key)
            return feed(WakeEnded(key, delivery, outcome))
        }
        fun cleaned(key: Key = a): Step = after(sim.cleaned(key))
        fun uncleaned(key: Key = a): Step = after(sim.cleaned(key, removed = false))
        /** The platform stops the run. With [reported], the exchange it cut short reports that at once. */
        fun stop(reason: Stop, key: Key = a, reported: Boolean = true): Step {
            val step = after(sim.stop(w.active.entries.single { it.value.first == key }.key, reason))
            if (reported && w.tasks.any { it.op.kind == Kind.Exchange && it.op.key == key }) fail(Failure.Cancelled, key)
            return step
        }
        /** The transfer runs to a successful ending whose wake is raised. */
        fun end(key: Key = a) { run(key); respond(Response.Ok, key); placed(key) }

        fun death() = set(LifecycleModel.die(w, c))
        fun reboot(kind: Reboot = Reboot.Drops) = set(LifecycleModel.reboot(w, c, kind))
        fun forceStop() = set(LifecycleModel.forceStop(w, c))
        fun taskStop() = set(LifecycleModel.taskStop(w, c))
        fun update(drop: Boolean) = set(LifecycleModel.update(w, c, drop))
        fun reschedule() = set(w.copy(jobs = w.jobs + w.limbo, limbo = emptySet()))
    }

    private inline fun <reified T : Effect> List<Effect>.of(): List<T> = filterIsInstance<T>()
    private fun exchange(key: Key, attempt: Int, started: Boolean = true) = Task(Op(Kind.Exchange, key, attempt), started)

    // Force-stop and the recovery barrier.

    @Test fun aWorkerThatRunsBeforeRecoveryAfterAForceStopSendsNothing() {
        val s = Scene()
        s.open(); s.ready(); s.run()
        s.forceStop()
        s.boot(); s.reschedule(); s.effects()
        assertFalse(s.run().accepted)
        assertEquals(listOf(Finish(s.a, 0, again = true)), s.effects())
        s.enter()
        assertEquals(Result.Aborted, s.record().result)
        assertNull(s.record().job)
        assertTrue(s.w.jobs.isEmpty())
        s.open()
        s.placed()
        assertTrue(s.effects().of<Transmit>().isEmpty())
        assertEquals(1, s.w.wakes.size)
    }

    @Test fun aForceStopOutlivesADeathInsideItsRecoveryAndTheLossOfItsEvidence() {
        val s = Scene()
        s.open(); s.ready()
        s.forceStop()
        s.boot()
        // The mark is written and nothing else: the process dies, and a reboot takes the platform's evidence.
        s.dying(writes = 1) { s.enter() }
        assertEquals(setOf(app), s.w.durable.stopping)
        assertNull(s.record().result)
        s.set(s.w.copy(token = false, flag = false))
        s.boot(); s.effects()
        assertFalse(s.w.seen)
        s.enter()
        assertEquals(Result.Aborted, s.record().result)
        assertTrue(s.w.durable.stopping.isEmpty())
        assertTrue(s.effects().of<Schedule>().isEmpty())
    }

    @Test fun theBootBroadcastAfterAForceStopAbortsAndSchedulesNothing() {
        val s = Scene()
        s.open(); s.ready()
        s.forceStop(); s.effects()
        s.enter()
        assertEquals(Result.Aborted, s.record().result)
        val effects = s.effects()
        assertTrue(effects.of<Schedule>().isEmpty())
        assertTrue(Cancel(s.a, null) in effects)
    }

    @Test fun aForceStopEndsTheTransferAbortedAtTheNextLaunch() {
        val s = Scene()
        s.open(); s.ready(person = true); s.run()
        s.forceStop()
        s.open()
        assertEquals(Result.Aborted, s.record().result)
        s.placed()
        assertEquals(1, s.effects().of<Raise>().size)
        // The handler did not retire it: it waits for the next launch, with no job.
        s.woke(Outcome.Expired)
        assertTrue(s.w.jobs.isEmpty())
    }

    @Test fun aForceStopIsReadByTheProcessAfterOneThatDiedBeforeRecovery() {
        val s = Scene()
        s.open(); s.ready(); s.run()
        s.forceStop()
        s.set(s.w.copy(token = false))
        s.boot(); s.death(); s.effects()
        s.boot(); s.reschedule()
        s.run()
        s.enter()
        assertEquals(Result.Aborted, s.record().result)
        assertTrue(s.effects().of<Transmit>().isEmpty())
    }

    // Control operations are not queued behind copies.

    @Test fun anAbortTakesEffectWhileAnotherTransfersCopyIsInFlight() {
        val s = Scene()
        val b = Key(app, "b", generation(2))
        s.open(); s.ready(); s.run(); s.effects()
        s.start(key = b); s.kept(b)
        val copy = Task(Op(Kind.Copy, b), started = true)
        assertTrue(copy in s.w.tasks)
        s.effects()
        assertTrue(s.feed(Abort(s.a)).accepted)
        assertEquals(Result.Aborted, s.record().result)
        assertEquals(listOf(CancelIo(s.a, 1)), s.effects().of<CancelIo>())
        assertTrue(copy in s.w.tasks)
    }

    // Host-owned headless completion (D10).

    @Test fun aTransferThatEndsWithNoSessionKeepsItsJobThroughTheHeadlessHandler() {
        val s = Scene()
        s.open(); s.ready()
        s.death()
        s.enter(); s.run(); s.respond(); s.effects()
        assertNull(s.w.process!!.session)
        s.placed()
        // The host boots the session for the wake; the job is still running.
        assertEquals(Session(app, LIVE, view = false), s.w.process!!.session)
        assertEquals(1, s.effects().of<Raise>().size)
        assertEquals(1, s.w.active.size)
        // The handler retires inside its wake: the job stays until the wake's outcome, storage included.
        assertTrue(s.feed(Retire(s.a)).accepted)
        assertTrue(s.effects().of<Finish>().isEmpty())
        s.woke(Outcome.Completed)
        assertEquals(listOf(Finish(s.a, 0, again = false)), s.effects().of<Finish>())
        assertTrue(s.w.jobs.isEmpty())
    }

    @Test fun aCompletedUnretiredEndingIsRaisedThreeTimesInTheBackgroundThenAtEachLaunch() {
        val s = Scene()
        s.open(); s.ready()
        s.death()
        s.enter(); s.end()
        repeat(2) {
            s.woke(Outcome.Completed)
            assertEquals(Phase.Ended, s.record().phase)
            assertTrue((s.a to Job.Work) in s.w.jobs)
            s.run()
        }
        assertEquals(3, s.record().wakes)
        s.woke(Outcome.Completed)
        assertTrue(s.w.jobs.isEmpty() && s.w.active.isEmpty())
        s.effects()
        s.open()
        assertEquals(1, s.effects().of<Raise>().size)
        // The other reading of "at most three wakes" is the one constant: a completed wake is the last in the background.
        assertEquals(Limits().wakes, Lifecycle.COMPLETED_WAKES)
        val t = Scene(Config(limits = Limits(completedWakes = 1)))
        t.open(); t.ready()
        t.death()
        t.enter(); t.end()
        t.woke(Outcome.Completed)
        assertTrue(t.w.jobs.isEmpty() && t.w.active.isEmpty())
    }

    @Test fun eachJobIsHeldUntilItsOwnWakeEnds() {
        val s = Scene()
        val b = Key(app, "b", generation(2))
        s.open(); s.ready(); s.ready(key = b)
        s.death()
        s.enter(); s.end(); s.end(b)
        assertEquals(2, s.w.active.size)
        s.effects()
        s.woke(Outcome.Completed)
        assertEquals(listOf(s.a), s.effects().of<Finish>().map { it.key })
        assertEquals(listOf(b), s.w.active.values.map { it.first })
    }

    @Test fun threeExpiredWakesThenTheEndingWaitsForALaunch() {
        val s = Scene()
        s.open(); s.ready()
        s.death()
        s.enter(); s.end()
        repeat(2) {
            s.woke(Outcome.Expired)
            assertTrue((s.a to Job.Work) in s.w.jobs)
            s.run()
        }
        assertEquals(3, s.record().wakes)
        s.woke(Outcome.Superseded)
        assertTrue(s.w.jobs.isEmpty())
        s.effects()
        s.open()
        assertEquals(1, s.effects().of<Raise>().size)
        s.woke(Outcome.Completed)
        s.leave(); s.effects()
        s.attach()
        assertEquals(1, s.effects().of<Raise>().size)
    }

    @Test fun aUserInitiatedJobIsKeptUntilTheWorkThatReplacesItIsAccepted() {
        val s = Scene()
        s.open(); s.ready(person = true)
        assertEquals(Job.UserJob, s.record().job)
        s.end()
        s.prompt = false
        s.effects()
        // The wake expires and WorkManager cannot store the work that would deliver again.
        s.sim.starve = true
        s.woke(Outcome.Expired)
        s.sim.starve = false
        assertEquals(listOf(Job.Work), s.effects().of<Schedule>().map { it.job })
        assertEquals(listOf<Effect>(Finish(s.a, 0, again = true)), s.w.queue)
        s.perform()
        s.answer(Job.Work)
        // The job is still held and recorded, and delivers again.
        assertEquals(Job.UserJob, s.record().job)
        assertEquals(setOf(s.a to Job.UserJob), s.w.jobs)
        s.prompt = true
        s.effects()
        s.run()
        assertEquals(1, s.effects().of<Raise>().size)
        // Accepted: the record names the work, and only then is the job released.
        s.prompt = false
        s.woke(Outcome.Superseded); s.perform()
        assertEquals(setOf(s.a to Job.UserJob, s.a to Job.Work), s.w.jobs)
        assertEquals(Job.UserJob, s.record().job)
        s.answer(Job.Work)
        assertEquals(Job.Work, s.record().job)
        assertEquals(listOf<Effect>(Cancel(s.a, Job.UserJob)), s.w.queue)
        s.perform()
        assertEquals(setOf(s.a to Job.Work), s.w.jobs)
    }

    // Scheduling is answered later.

    @Test fun aLateAnswerForWorkDoesNotReplaceTheUserInitiatedJobTakenSince() {
        val s = Scene()
        s.open(); s.ready(person = true)
        s.reboot(); s.enter()
        assertEquals(Job.Work, s.record().job)
        s.death()
        s.prompt = false
        s.boot(); s.set(s.w.copy(visible = true))
        // Recovery asks for the work again, and the launch for the job, before either is answered.
        s.enter(); s.feed(SessionStarted(app, LIVE)); s.attach(); s.perform()
        assertEquals(setOf(Job.Work, Job.UserJob), s.w.replies.map { it.job }.toSet())
        s.answer(Job.UserJob); s.perform()
        assertEquals(Job.UserJob, s.record().job)
        assertEquals(setOf(s.a to Job.UserJob), s.w.jobs)
        s.answer(Job.Work); s.perform()
        assertEquals(Job.UserJob, s.record().job)
        assertEquals(setOf(s.a to Job.UserJob), s.w.jobs)
    }

    @Test fun anAnswerToAScheduleThatALaterOneReplacedIsIgnored() {
        val s = Scene()
        s.open(); s.ready(person = true)
        s.reboot(); s.enter()
        s.prompt = false
        s.feed(SessionStarted(app, LIVE)); s.effects()
        // Two launches ask for the job, and the first one's answer comes after the second was asked.
        s.attach(); s.leave(); s.attach()
        val (first, second) = s.effects().of<Schedule>()
        assertFalse(Lifecycle.admits(s.w.durable, s.w.process!!, first))
        val before = s.w
        val late = s.feed(Scheduled(s.a, Job.UserJob, first.token))
        assertFalse(late.accepted)
        assertTrue(late.writes.isEmpty() && late.effects.isEmpty())
        assertEquals(before, s.w)
        assertEquals(second.token, s.w.replies.single().token)
        s.answer(Job.UserJob)
        assertEquals(Job.UserJob, s.record().job)
    }

    @Test fun aBridgeThatStartsBeforeItsUpgradeIsAnsweredCarriesOn() {
        val s = Scene()
        s.open(); s.ready(person = true)
        s.reboot(); s.enter()
        s.prompt = false
        s.feed(SessionStarted(app, LIVE)); s.attach(); s.perform()
        s.run(job = Job.Work); s.perform()
        assertEquals(setOf(exchange(s.a, 1)), s.w.tasks)
        // The job is accepted while the bridge sends: the bridge carries on and the job is let go.
        s.answer(Job.UserJob); s.perform()
        assertEquals(Job.Work, s.record().job)
        assertEquals(setOf(s.a to Job.Work), s.w.jobs)
        s.prompt = true
        s.stop(Stop.System); s.effects()
        assertTrue(s.run().accepted)
        assertEquals(listOf(Transmit(s.a, 2, 0)), s.effects().of<Transmit>())
    }

    @Test fun aJobThatRunsBeforeItsAcceptanceIsReportedIsKeptAndRunsAgain() {
        val s = Scene()
        s.open(); s.ready(person = true)
        s.reboot(); s.enter()
        s.prompt = false
        s.feed(SessionStarted(app, LIVE)); s.attach(); s.perform()
        s.effects()
        s.run(job = Job.UserJob); s.perform()
        assertEquals(listOf<Effect>(Finish(s.a, 0, again = true)), s.effects())
        assertEquals(Job.Work, s.record().job)
        // The answer names the job in the record and releases the work; the job's next run sends.
        s.answer(Job.UserJob); s.perform()
        assertEquals(Job.UserJob, s.record().job)
        assertEquals(setOf(s.a to Job.UserJob), s.w.jobs)
        s.run(); s.perform()
        assertEquals(setOf(exchange(s.a, 1)), s.w.tasks)
    }

    @Test fun workThatCannotBeStoredParksTheTransferUntilALaunchAndFailsAStart() {
        val s = Scene()
        s.open(); s.ready()
        s.update(drop = true)
        s.prompt = false
        s.sim.starve = true
        s.enter()
        s.sim.starve = false
        s.answer(Job.Work)
        assertTrue(s.record().parked)
        assertNull(s.record().job)
        assertNull(s.record().result)
        s.prompt = true
        s.open()
        assertFalse(s.record().parked)
        assertEquals(Job.Work, s.record().job)

        val t = Scene()
        t.open()
        t.prompt = false
        t.start()
        t.sim.starve = true
        t.kept()
        t.answer(Job.Work)
        assertEquals(Phase.Retired, t.record().phase)
        assertTrue(Answer(t.a, false) in t.effects())
    }

    // Operations start by their lease and report once.

    @Test fun aStopBeforeTheExchangeStartsSendsNoBytes() {
        val s = Scene()
        s.open(); s.ready()
        s.prompt = false
        s.run()
        val transmit = Transmit(s.a, 1, 0)
        assertEquals(listOf<Effect>(transmit), s.w.queue)
        s.stop(Stop.System, reported = false)
        assertFalse(Lifecycle.admits(s.w.durable, s.w.process!!, transmit))
        s.perform()
        // It was not admitted, so it wrote nothing; it still reports, and the next attempt waits for that.
        assertEquals(setOf(exchange(s.a, 1, started = false)), s.w.tasks)
        s.prompt = true
        s.run()
        assertEquals(setOf(exchange(s.a, 1, started = false)), s.w.tasks)
        s.fail(Failure.Cancelled)
        assertEquals(setOf(exchange(s.a, 2)), s.w.tasks)
    }

    @Test fun theNextAttemptWaitsForTheCancelledOneToReport() {
        val s = Scene()
        s.open(); s.ready(); s.run(); s.effects()
        s.stop(Stop.System, reported = false)
        s.run()
        assertTrue(s.effects().of<Transmit>().isEmpty())
        assertEquals(setOf(exchange(s.a, 1)), s.w.tasks)
        // The exchange had finished; its answer arrives after the stop and reports no result.
        assertTrue(s.respond(Response.Ok).accepted)
        assertNull(s.record().result)
        assertEquals(0, s.record().done)
        assertEquals(listOf(Transmit(s.a, 2, 0)), s.effects().of<Transmit>())
    }

    @Test fun aRetirementRemovesNothingUntilEveryOperationHasReported() {
        val s = Scene()
        s.open(); s.start(); s.effects()
        // Retired while its credentials are being kept.
        assertTrue(s.feed(Retire(s.a)).accepted)
        val retired = s.effects()
        assertTrue(Answer(s.a, false) in retired && retired.of<Clean>().isEmpty())
        s.kept()
        assertEquals(listOf<Effect>(Clean(s.a)), s.effects())
        s.cleaned()
        assertNull(s.sim.record(s.a))

        // Retired while its snapshot is being copied.
        val t = Scene()
        t.open(); t.start(); t.kept(); t.effects()
        assertTrue(t.feed(Retire(t.a)).accepted)
        val effects = t.effects()
        assertTrue(CancelCopy(t.a) in effects && effects.of<Clean>().isEmpty())
        // The copy was told to stop and reports that; the cleanup waited for it.
        t.sim.report(t.w.tasks.single(), SnapshotFailed(t.a)); t.perform()
        assertEquals(listOf<Effect>(Clean(t.a)), t.effects())
        assertEquals(Phase.Retired, t.record().phase)
    }

    // Sessions.

    @Test fun aLateEventOfAnEarlierSessionLeavesTheLiveOneAlone() {
        val s = Scene(Config(namespaces = listOf(app, drive)))
        s.boot(); s.set(s.w.copy(visible = true))
        s.enter(); s.enter(drive)
        s.feed(SessionStarted(app, 1)); s.feed(ViewAttached(app, 1, s.conditions))
        assertTrue(s.feed(SessionEnded(app, 1)).accepted)
        s.feed(SessionStarted(drive, 2)); s.feed(ViewAttached(drive, 2, s.conditions))
        val live = Session(drive, 2, view = true)
        assertFalse(s.feed(SessionEnded(app, 1)).accepted)
        assertFalse(s.feed(ViewDetached(app, 1)).accepted)
        assertEquals(live, s.w.process!!.session)
        // An earlier session of the same namespace is told from the live one.
        assertFalse(s.feed(SessionEnded(drive, 1)).accepted)
        assertFalse(s.feed(ViewDetached(drive, 1)).accepted)
        assertEquals(live, s.w.process!!.session)
        // One session a process.
        assertFalse(s.feed(SessionStarted(app, 3)).accepted)
        assertEquals(live, s.w.process!!.session)
        assertTrue(s.feed(SessionEnded(drive, 2)).accepted)
        assertNull(s.w.process!!.session)
    }

    // Storage namespaces.

    @Test fun anAgentDriveNeitherHearsNorRetiresTheAppsRegistrations() {
        val s = Scene(Config(namespaces = listOf(app, drive)))
        s.open(); s.ready()
        s.death()
        s.enter(); s.end(); s.woke(Outcome.Completed)
        s.death(); s.effects()
        s.open(drive)
        assertTrue(s.effects().none { it is Raise || it is Place })
        assertFalse(s.feed(Retire(s.a)).accepted)
        assertFalse(s.feed(Abort(s.a)).accepted)
        assertEquals(Phase.Ended, s.record().phase)
    }

    @Test fun anAgentDrivesJobStartedOutsideItsSessionIsKeptForItsSession() {
        val s = Scene(Config(namespaces = listOf(app, drive)))
        val d = Key(drive, "a", generation(1))
        s.open(drive); s.ready(key = d)
        s.death(); s.effects()
        // Its job starts the process with no drive: nothing is sent, and the job stays recorded and held.
        s.enter(drive)
        assertFalse(s.run(d).accepted)
        val outside = s.effects()
        assertEquals(listOf(Finish(d, 0, again = true)), outside.of<Finish>())
        assertTrue(outside.of<Transmit>().isEmpty())
        assertEquals(Job.Work, s.record(d).job)
        assertEquals(setOf(d to Job.Work), s.w.jobs)
        // The app's launch does not see it.
        s.open()
        assertTrue(s.effects().none { it is Raise || it is Schedule || it is Transmit })
        // Beside the drive's next session the job runs and sends.
        s.death()
        s.open(drive); s.effects()
        assertTrue(s.run(d).accepted)
        assertEquals(listOf(Transmit(d, 1, 0)), s.effects().of<Transmit>())
    }

    @Test fun anEndingWaitsOutAnotherNamespacesSessionAndIsRaisedWhenItEnds() {
        val s = Scene(Config(namespaces = listOf(app, drive)))
        s.open(); s.ready()
        s.death()
        s.open(drive); s.enter(); s.run(); s.effects()
        s.respond()
        val effects = s.effects()
        assertTrue(effects.none { it is Raise })
        assertEquals(listOf(Finish(s.a, 0, again = true)), effects.of<Finish>())
        assertEquals(0, s.record().wakes)
        // The work runs again on the platform's backoff; once the drive's session has ended it delivers.
        s.leave(); s.feed(SessionEnded(drive, LIVE))
        s.run(); s.placed()
        assertEquals(1, s.effects().of<Raise>().size)
        assertEquals(Session(app, LIVE, view = false), s.w.process!!.session)
    }

    // Initiation intent and execution mode.

    @Test fun aRebootBridgesAPersonsTransferWithWorkAndTheNextLaunchRestoresItsJob() {
        val s = Scene()
        s.open(); s.ready(person = true)
        assertEquals(Job.UserJob, s.record().job)
        s.reboot()
        s.enter()
        assertEquals(Job.Work, s.record().job)
        assertEquals(Intent.Person, s.record().intent)
        repeat(2) { s.run(); s.stop(Stop.System) }
        assertNull(s.record().result)
        s.effects()
        s.open()
        assertEquals(Job.UserJob, s.record().job)
        assertEquals(setOf(s.a to Job.UserJob), s.w.jobs)
        assertTrue(Cancel(s.a, Job.Work) in s.effects())
    }

    @Test fun aBridgedPersonsTransferParksAfterThreeStopsWithNothingKeptAndALaunchResumesIt() {
        val s = Scene()
        s.open(); s.ready(person = true)
        s.reboot(); s.enter()
        repeat(2) { s.run(); s.stop(Stop.System) }
        s.run(); s.effects()
        s.stop(Stop.System)
        val effects = s.effects()
        assertTrue(Finish(s.a, 0, again = false) in effects && Cancel(s.a, Job.Work) in effects)
        assertTrue(s.record().parked)
        assertNull(s.record().result)
        assertNull(s.record().job)
        assertEquals(Intent.Person, s.record().intent)
        assertTrue(s.w.jobs.isEmpty())
        // No entrant resumes it in the background.
        s.death()
        s.enter()
        assertTrue(s.effects().of<Schedule>().isEmpty())
        // A launch with a view does, on the job it is entitled to, with its stops forgotten.
        s.open()
        assertEquals(Job.UserJob, s.record().job)
        assertFalse(s.record().parked)
        assertEquals(0, s.record().stops)
    }

    @Test fun aBridgeThatIsSendingIsNotRestartedByALaunch() {
        val s = Scene(Config(api = 33))
        s.open(); s.ready(person = true)
        s.reboot()
        s.enter(); s.run(); s.effects()
        s.open()
        val effects = s.effects()
        assertEquals(listOf(Promote(s.a, 0)), effects.of<Promote>())
        assertTrue(effects.none { it is CancelIo || it is Cancel })
    }

    @Test fun aPersonsWorkThatStartsUnderAViewAsksForForegroundExecution() {
        val s = Scene(Config(api = 33))
        s.open(); s.ready(person = true); s.effects()
        assertEquals(Intent.Person to Job.Work, s.record().let { it.intent to it.job })
        s.run()
        val effects = s.effects()
        assertEquals(listOf(Promote(s.a, 0)), effects.of<Promote>())
        assertTrue(effects.indexOf(Promote(s.a, 0)) < effects.indexOfFirst { it is Transmit })

        // A foreground service may not start from the background: with no view the launch asks.
        val t = Scene(Config(api = 33))
        t.open(); t.ready(person = true); t.leave(); t.effects()
        t.run()
        assertTrue(t.effects().of<Promote>().isEmpty())
        t.attach()
        assertEquals(listOf(Promote(t.a, 0)), t.effects().of<Promote>())

        val program = Scene(Config(api = 33))
        program.open(); program.ready(); program.run()
        assertTrue(program.effects().of<Promote>().isEmpty())
    }

    @Test fun aProgramsStartWhileTheAppShowsIsOrdinaryWork() {
        val shown = Conditions(api = 35, visible = true)
        assertEquals(Intent.Ordinary, Lifecycle.intent(person = false, shown))
        assertEquals(Intent.Person, Lifecycle.intent(person = true, shown))
        assertEquals(Intent.Ordinary, Lifecycle.intent(person = true, shown.copy(visible = false)))
        assertEquals(Intent.Ordinary, Lifecycle.intent(person = true, shown.copy(api = 30)))
        val s = Scene()
        s.open(); s.start(); s.kept()
        assertEquals(Job.Work, s.record().job)
    }

    @Test fun aRefusedUserInitiatedJobIsOrdinaryWorkAtOnceOnAndroid15() {
        val s = Scene(Config(denied = true))
        s.open(); s.effects()
        s.start(person = true)
        assertTrue(s.kept().accepted)
        assertEquals(listOf(Job.UserJob, Job.Work), s.effects().of<Schedule>().map { it.job })
        assertEquals(Job.Work, s.record().job)
        assertEquals(Phase.Preparing, s.record().phase)
        s.committed(); s.run(); s.leave(); s.effects()
        s.attach()
        assertTrue(s.effects().of<Promote>().isEmpty())
        val shown = Conditions(api = 35, visible = true)
        assertFalse(Lifecycle.promotes(Intent.Person, shown))
        assertTrue(Lifecycle.promotes(Intent.Person, shown.copy(api = 34)))
        assertTrue(Lifecycle.promotes(Intent.Person, shown.copy(api = 31)))
        assertFalse(Lifecycle.promotes(Intent.Person, shown.copy(api = 30)))
    }

    @Test fun everyStopOfAProgramsWorkCountsAndTheThirdWithNothingKeptEndsIt() {
        val s = Scene()
        s.open(); s.ready(requests = 2)
        repeat(2) { s.run(); s.stop(Stop.System) }
        assertEquals(2, s.record().stops)
        // A kept response is progress.
        s.run(); s.respond()
        assertEquals(0, s.record().stops)
        repeat(3) { assertNull(s.record().result); s.stop(Stop.System); if (s.record().result == null) s.run() }
        assertEquals(Result.FetchError, s.record().result)
        assertFalse(s.record().parked)
        // The work the platform schedules again delivers the ending.
        assertTrue((s.a to Job.Work) in s.w.jobs)
    }

    @Test fun aPersonsStopEndsTheTransferAbortedAndNothingRunsUntilTheyOpenTheApp() {
        val s = Scene()
        s.open(); s.ready(person = true); s.run(); s.leave(); s.effects()
        s.stop(Stop.Person)
        assertEquals(Result.Aborted, s.record().result)
        val effects = s.effects()
        assertEquals(listOf(Finish(s.a, 0, again = false)), effects.of<Finish>())
        assertTrue(effects.of<Schedule>().isEmpty() && s.w.jobs.isEmpty())
        s.attach(); s.placed()
        assertEquals(1, s.effects().of<Raise>().size)
    }

    @Test fun aTaskManagerStopAbortsItsTransferAtTheNextEntrant() {
        val s = Scene()
        s.open(); s.ready(person = true); s.run()
        s.taskStop(); s.effects()
        // Another registration's work starts the process: the job is gone and nothing explains it.
        s.enter()
        assertEquals(Result.Aborted, s.record().result)
        assertNull(s.record().job)
        assertEquals(Limits().wakes, s.record().wakes)
        assertTrue(s.effects().none { it is Schedule || it is Transmit })
        assertTrue(s.w.jobs.isEmpty())

        // A reboot explains it: the transfer is bridged.
        val t = Scene()
        t.open(); t.ready(person = true); t.run()
        t.reboot(); t.enter()
        assertNull(t.record().result)
        assertEquals(Job.Work, t.record().job)
    }

    @Test fun aDeathWhileTheBridgeIsAskedForIsNotReadAsAPersonsStop() {
        val s = Scene()
        s.open(); s.ready(person = true)
        s.reboot()
        s.prompt = false
        s.enter(); s.perform()
        // The loss is recorded and the restart's evidence rearmed; the work is held and its answer dies with the process.
        assertNull(s.record().job)
        assertEquals(setOf(s.a to Job.Work), s.w.jobs)
        assertTrue(s.w.restarted.isEmpty())
        s.death()
        s.prompt = true
        s.enter()
        assertNull(s.record().result)
        assertEquals(Job.Work, s.record().job)
    }

    @Test fun aUserInitiatedJobAcceptedAsItsProcessDiedIsTheTransfersAtTheNextEntrant() {
        val s = Scene()
        s.open(); s.ready(person = true)
        s.reboot()
        s.prompt = false
        s.open(); s.perform()
        assertNull(s.record().job)
        assertEquals(setOf(s.a to Job.UserJob), s.w.jobs)
        s.death(); s.effects()
        s.prompt = true
        s.enter()
        assertTrue(s.effects().of<Schedule>().isEmpty())
        assertEquals(Job.UserJob, s.record().job)
        s.run()
        assertEquals(listOf(Transmit(s.a, 1, 0)), s.effects().of<Transmit>())
    }

    @Test fun aUserInitiatedJobThatOutlivesARebootIsKeptAndOneWithoutItsStandingIsBridged() {
        val s = Scene()
        s.open(); s.ready(person = true)
        s.reboot(Reboot.Keeps); s.effects()
        s.enter()
        assertTrue(s.effects().of<Schedule>().isEmpty())
        assertEquals(Job.UserJob, s.record().job)
        s.run()
        assertEquals(listOf(Transmit(s.a, 1, 0)), s.effects().of<Transmit>())

        val t = Scene()
        t.open(); t.ready(person = true)
        t.reboot(Reboot.Demotes)
        t.enter(); t.effects()
        // It runs under ordinary limits: work takes the transfer, and the job is released once the work is held.
        t.run()
        val effects = t.effects()
        assertTrue(effects.of<Transmit>().isEmpty())
        assertTrue(effects.indexOfFirst { it is Schedule } < effects.indexOf(Cancel(t.a, Job.UserJob)))
        assertEquals(Job.Work, t.record().job)
        assertEquals(setOf(t.a to Job.Work), t.w.jobs)
        // A launch takes the job back, with its standing.
        t.open()
        assertEquals(Job.UserJob, t.record().job)
        assertEquals(setOf(t.a to Job.UserJob), t.w.jobs)
        assertTrue(t.w.demoted.isEmpty())
    }

    // Starting.

    @Test fun aStartHoldsItsJobBeforeTheSnapshotCopyAndAnswersAfterIt() {
        val s = Scene()
        s.open(); s.effects()
        s.start(person = true); s.kept()
        val effects = s.effects()
        assertTrue(effects.indexOfFirst { it is Schedule && it.job == Job.UserJob } < effects.indexOf(Copy(s.a)))
        assertEquals(Phase.Preparing, s.record().phase)
        assertTrue(effects.of<Answer>().isEmpty())
        s.committed()
        assertEquals(listOf(Answer(s.a, true)), s.effects().of<Answer>())
    }

    @Test fun aFailedSnapshotInALiveStartReleasesItsJobItsCredentialsAndItsId() {
        val s = Scene()
        s.open(); s.start(); s.kept(); s.effects()
        s.sim.report(s.w.tasks.single(), SnapshotFailed(s.a)); s.perform()
        val effects = s.effects()
        assertEquals(Phase.Retired, s.record().phase)
        assertTrue(Cancel(s.a, Job.Work) in effects && Answer(s.a, false) in effects && Clean(s.a) in effects)
        assertTrue(s.w.jobs.isEmpty())
        s.cleaned()
        assertNull(s.sim.record(s.a))
        assertTrue(s.start(key = Key(app, "a", generation(2))).accepted)
    }

    @Test fun aStartThatDiedKeepingItsCredentialsIsRetiredAtRecovery() {
        val s = Scene()
        s.open(); s.start()
        s.death(); s.effects()
        s.enter()
        assertEquals(Phase.Retired, s.record().phase)
        assertTrue(Clean(s.a) in s.effects())
        s.cleaned()
        assertNull(s.sim.record(s.a))
    }

    @Test fun aDeathDuringTheSnapshotResumesTheCopyAndAFailedResumeIsTerminal() {
        val s = Scene()
        s.open(); s.start(); s.kept()
        s.death(); s.effects()
        s.enter()
        assertEquals(listOf(Copy(s.a)), s.effects().of<Copy>())
        assertEquals(Phase.Preparing, s.record().phase)
        s.run()
        assertTrue(s.effects().of<Transmit>().isEmpty())
        s.committed()
        assertEquals(listOf(Transmit(s.a, 1, 0)), s.effects().of<Transmit>())

        val t = Scene()
        t.open(); t.start(); t.kept()
        t.death()
        t.enter()
        t.sim.report(t.w.tasks.single(), SnapshotFailed(t.a)); t.perform()
        assertEquals(Result.FetchError, t.record().result)
    }

    @Test fun aSecondStartOfALiveIdIsRefused() {
        val s = Scene()
        s.open(); s.ready()
        assertFalse(s.feed(Start(Key(app, "a", generation(2)), Intent.Ordinary, 1, s.conditions)).accepted)
    }

    // Attempts, stops and results.

    @Test fun aStopThatRacesAFinishedExchangeNeverReportsSuccess() {
        val s = Scene()
        s.open(); s.ready(); s.run(); s.effects()
        s.stop(Stop.System, reported = false)
        val effects = s.effects()
        assertTrue(CancelIo(s.a, 1) in effects && Finish(s.a, 0, again = true) in effects)
        // The exchange had finished; its answer arrives after the stop.
        s.respond(Response.Ok)
        assertNull(s.record().result)
        assertEquals(0, s.record().done)
        s.run()
        assertEquals(listOf(Transmit(s.a, 2, 0)), s.effects().of<Transmit>())
    }

    @Test fun openingTheAppDuringARunningJobStartsNothingElse() {
        val s = Scene()
        s.open(); s.ready(person = true); s.run(); s.leave(); s.effects()
        s.attach()
        assertTrue(s.effects().none { it is Transmit || it is Cancel || it is CancelIo })
        assertEquals(setOf(s.a to Job.UserJob), s.w.jobs)
        // A second run of the registration is released at once.
        s.sim.w = s.w.copy(active = s.w.active + (7 to (s.a to Job.Work)))
        s.feed(JobStarted(s.a, Run(7, Job.Work), s.conditions))
        assertEquals(listOf<Effect>(Finish(s.a, 7, again = false)), s.effects())
    }

    @Test fun anInvalidStatusEndsTheExchange() {
        val s = Scene()
        s.open(); s.ready(); s.run()
        s.respond(Response.Invalid)
        assertEquals(Result.FetchError, s.record().result)
        assertEquals(1, s.w.active.size)
    }

    @Test fun networkFailuresAskAgainFourTimesAndTheFifthFails() {
        val s = Scene()
        s.open(); s.ready()
        repeat(4) {
            s.run(); s.effects()
            s.fail(Failure.Network)
            assertEquals(listOf(Finish(s.a, 0, again = true)), s.effects().of<Finish>())
        }
        s.run(); s.fail(Failure.Network)
        assertEquals(Result.FetchError, s.record().result)
    }

    @Test fun aRequestWhoseResponseIsKeptIsNotSentAgain() {
        val s = Scene()
        s.open(); s.ready(requests = 2); s.run(); s.effects()
        s.respond()
        assertEquals(listOf(Transmit(s.a, 2, 1)), s.effects().of<Transmit>())
        s.death()
        s.enter(); s.run()
        assertEquals(listOf(Transmit(s.a, 3, 1)), s.effects().of<Transmit>())
        s.respond()
        assertEquals(Result.Success, s.record().result)
    }

    // Placement.

    @Test fun aFailedPlacementIsTriedAgainWhileTheViewStaysUp() {
        // A response is kept before the stop, so the ending has a file to place.
        val s = Scene()
        s.open(); s.ready(requests = 2); s.run(); s.respond()
        s.forceStop()
        s.open(); s.effects()
        repeat(2) {
            s.unplaced()
            assertEquals(listOf<Effect>(Place(s.a, again = true)), s.effects())
        }
        s.placed()
        assertEquals(1, s.effects().of<Raise>().size)

        // Once the view has left, the next launch places it.
        val t = Scene()
        t.open(); t.ready(requests = 2); t.run(); t.respond()
        t.forceStop()
        t.open(); t.leave(); t.effects()
        t.unplaced()
        assertTrue(t.effects().isEmpty())
        t.attach()
        assertEquals(listOf<Effect>(Place(t.a, again = false)), t.effects())
    }

    @Test fun aFailedBackgroundPlacementSpendsADeliveryAndTheThirdLeavesItToTheLaunches() {
        val s = Scene()
        s.open(); s.ready()
        s.death()
        s.enter(); s.run(); s.respond()
        repeat(2) {
            s.effects()
            s.unplaced()
            assertEquals(listOf<Effect>(Finish(s.a, 0, again = true)), s.effects())
            s.run()
        }
        s.unplaced()
        assertEquals(Limits().wakes, s.record().wakes)
        assertNull(s.record().job)
        assertTrue(s.w.jobs.isEmpty() && s.w.active.isEmpty())
        s.effects()
        s.open()
        assertEquals(listOf<Effect>(Place(s.a, again = false)), s.effects())
    }

    // Retirement.

    @Test fun aRetiredGenerationDeliversNothingAndItsLateEventsMissItsSuccessor() {
        val s = Scene()
        val next = Key(app, "a", generation(2))
        s.open(); s.ready(); s.run(); s.respond(); s.effects()
        // Retired between the ending and its placement: its files stay until the placement has reported.
        assertTrue(s.feed(Retire(s.a)).accepted)
        assertTrue(s.effects().of<Clean>().isEmpty())
        s.placed()
        assertEquals(listOf<Effect>(Clean(s.a)), s.effects())
        s.cleaned()
        s.ready(key = next); s.run(next); s.effects()
        assertFalse(s.feed(ResponseComplete(s.a, 1, Response.Ok)).accepted)
        assertFalse(s.feed(WakeEnded(s.a, 1, Outcome.Completed)).accepted)
        assertFalse(s.feed(JobStopped(s.a, 9, Stop.System)).accepted)
        assertTrue(s.effects().isEmpty())
        assertEquals(Phase.Sending, s.record(next).phase)
    }

    @Test fun aRetiredIdIsTakenUntilItsFilesAreRemoved() {
        val s = Scene()
        val next = Key(app, "a", generation(2))
        s.open(); s.ready()
        assertTrue(s.feed(Retire(s.a)).accepted)
        // Its kept responses are published under the id, so the next generation waits for their removal.
        assertFalse(s.feed(Start(next, Intent.Ordinary, 1, s.conditions)).accepted)
        s.cleaned()
        assertNull(s.sim.record(s.a))
        assertTrue(s.start(key = next).accepted)
    }

    @Test fun aFailedCleanupLeavesTheRecordRetiredUntilARetirementCleansAgain() {
        val s = Scene()
        val next = Key(app, "a", generation(2))
        s.open(); s.ready(); s.effects()
        assertTrue(s.feed(Retire(s.a)).accepted)
        assertEquals(listOf<Effect>(Clean(s.a)), s.effects().of<Clean>())
        // The secret store kept its credentials: nothing is deleted, and the id stays taken.
        val failed = s.uncleaned()
        assertTrue(failed.accepted && failed.writes.isEmpty() && failed.effects.isEmpty())
        assertEquals(Phase.Retired, s.record().phase)
        assertFalse(s.feed(Start(next, Intent.Ordinary, 1, s.conditions)).accepted)
        // The app retires it again: its files are removed again, and the record goes when they are.
        val again = s.feed(Retire(s.a))
        assertTrue(again.accepted && again.writes.isEmpty())
        assertEquals(listOf<Effect>(Clean(s.a)), s.effects())
        assertEquals(listOf<Lifecycle.Write>(Lifecycle.Delete(s.a)), s.cleaned().writes)
        assertNull(s.sim.record(s.a))
        assertTrue(s.start(key = next).accepted)
    }

    @Test fun aRetirementWhileACleanupRunsStartsNoSecond() {
        val s = Scene()
        s.open(); s.ready(); s.effects()
        assertTrue(s.feed(Retire(s.a)).accepted)
        assertEquals(listOf<Effect>(Clean(s.a)), s.effects().of<Clean>())
        val joined = s.feed(Retire(s.a))
        assertTrue(joined.accepted && joined.writes.isEmpty() && joined.effects.isEmpty())
        s.cleaned()
        assertNull(s.sim.record(s.a))
        // Nothing waits for a second report.
        assertFalse(s.feed(Cleaned(s.a)).accepted)
        assertFalse(s.feed(Lifecycle.CleanFailed(s.a)).accepted)
    }

    @Test fun aCleanupThatFailedIsTriedAgainAtTheNextRecovery() {
        val s = Scene()
        s.open(); s.ready(); s.effects()
        s.feed(Retire(s.a))
        s.uncleaned(); s.effects()
        s.death()
        s.enter()
        assertEquals(listOf<Effect>(Clean(s.a)), s.effects().of<Clean>())
        s.cleaned()
        assertNull(s.sim.record(s.a))
    }

    @Test fun anotherSessionsRetirementCleansNothing() {
        val s = Scene(Config(namespaces = listOf(app, drive)))
        s.open(); s.ready()
        s.feed(Retire(s.a))
        s.uncleaned()
        // The next process recovers the app's namespace, and its session is a drive's.
        s.death()
        s.enter()
        s.uncleaned(); s.effects()
        s.open(drive)
        assertFalse(s.feed(Retire(s.a)).accepted)
        assertTrue(s.effects().of<Clean>().isEmpty())
        assertEquals(Phase.Retired, s.record().phase)
    }

    @Test fun retiringAnUnfinishedTransferStopsItsBytesAndItsJob() {
        val s = Scene()
        s.open(); s.ready(); s.run(); s.effects()
        s.feed(Retire(s.a))
        val effects = s.effects()
        assertTrue(CancelIo(s.a, 1) in effects && Finish(s.a, 0, again = false) in effects && effects.of<Clean>().isEmpty())
        assertTrue(s.w.jobs.isEmpty())
        s.respond(Response.Ok)
        assertEquals(listOf<Effect>(Clean(s.a)), s.effects())
        assertNull(s.record().result)
    }

    // Reboot and update.

    @Test fun aRebootIsRecoveredWithoutOpeningTheApp() {
        val s = Scene()
        s.open(); s.ready()
        s.reboot()
        assertTrue(s.w.receiver)
        s.enter(); s.run()
        assertEquals(1, s.effects().of<Transmit>().size)
    }

    @Test fun anUpdateThatDropsTheJobsIsRecoveredByItsBroadcast() {
        val s = Scene()
        s.open(); s.ready()
        s.update(drop = true)
        assertTrue(s.w.jobs.isEmpty() && s.w.receiver)
        s.enter()
        assertEquals(setOf(s.a to Job.Work), s.w.jobs)
    }
}

/** The cases over [Lifecycle.step] and the model's own bookkeeping. */
class LifecycleAcceptanceTest : LifecycleAcceptance(adapted = false)

/** The cases over the Android adapter: [Transfers], its store on disk and its operations. */
@org.junit.runner.RunWith(org.robolectric.RobolectricTestRunner::class)
@org.robolectric.annotation.Config(sdk = [35], manifest = org.robolectric.annotation.Config.NONE)
class LifecycleAdapterTest : LifecycleAcceptance(adapted = true)
