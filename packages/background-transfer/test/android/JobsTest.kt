package exact.backgroundtransfer

import android.app.ActivityManager
import android.app.ApplicationStartInfo
import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.content.Context
import android.content.Intent
import android.os.Looper
import android.os.PersistableBundle
import android.provider.Settings
import androidx.work.Data
import androidx.work.ListenableWorker
import androidx.work.NetworkType
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.impl.WorkManagerImpl
import androidx.work.testing.TestWorkerBuilder
import androidx.work.testing.WorkManagerTestInitHelper
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Outcome
import exact.backgroundtransfer.Lifecycle.Phase
import exact.backgroundtransfer.Lifecycle.Result
import exact.backgroundtransfer.Lifecycle.SessionStarted
import exact.backgroundtransfer.Lifecycle.ViewAttached
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadow.api.Shadow
import org.robolectric.shadows.ShadowActivityManager
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executor
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** [Jobs], [TransferWorker], [TransferJob] and [TransferBoot] over Robolectric's schedulers and WorkManager's test driver. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class JobsTest {
    /** The process start history, which Robolectric does not keep. */
    @Implements(ActivityManager::class)
    class StartHistory : ShadowActivityManager() {
        companion object { var starts = emptyList<ApplicationStartInfo>() }
        @Implementation fun getHistoricalProcessStartReasons(max: Int): List<ApplicationStartInfo> = starts.take(max)
    }

    private val app: Context get() = RuntimeEnvironment.getApplication()
    private val work get() = WorkManager.getInstance(app) as WorkManagerImpl
    private val drive = Namespace("0123456789abcdef", agent = true)
    private lateinit var root: File
    private lateinit var transport: FakeTransport
    private lateinit var jobs: Jobs
    private lateinit var transfers: Transfers
    private val host = FakeHost()
    private val threads = ArrayList<ExecutorService>()

    @Before fun setup() {
        WorkManagerTestInitHelper.initializeTestWorkManager(app)
        root = Files.createTempDirectory("transfer-jobs-").toFile()
        transport = FakeTransport(root)
        Settings.Global.putInt(app.contentResolver, "boot_count", 10)
        Jobs.visible = { false }
    }
    @After fun cleanup() {
        Jobs.installed = null
        Jobs.visible = null
        Jobs.finishers.clear()
        Jobs.promoters.clear()
        Jobs.shown.clear()
        threads.forEach { it.shutdownNow() }
        root.deleteRecursively()
        WorkManagerTestInitHelper.closeWorkDatabase()
    }

    /**
     * A process: its transfers over what the last one left. Each effect is performed where [control] runs it, inline
     * by default, unless [threaded]: then as in the app.
     */
    private fun process(namespace: Namespace? = Fixture.app, threaded: Boolean = false, control: Executor = Fixture.inline,
                        store: Registrations = Registrations(File(root, "private"))): Transfers {
        jobs = Jobs(app)
        transfers = if (threaded) Transfers(store, transport::exchange, jobs, host, Executors.newSingleThreadExecutor().also { threads += it })
            else Transfers(store, transport::exchange, jobs, host, control, Fixture.inline { transfers })
        Jobs.installed = transfers
        if (namespace != null) {
            transfers.enter(namespace)
            transfers.feed(SessionStarted(namespace, 1))
        }
        return transfers
    }
    private fun start(id: String = "video", person: Boolean = false, namespace: Namespace = Fixture.app, upload: Boolean = false): Key {
        val key = Fixture.key(id, namespace)
        if (person) Jobs.visible = { true }
        assertTrue(Fixture.start(transfers, key, root, Lifecycle.intent(person, transfers.conditions), upload))
        Jobs.visible = { false }
        return key
    }
    private fun record(key: Key) = transfers.store.durable().records[key]!!
    private fun scheduler(namespace: Namespace = Fixture.app) = jobs.scheduler(namespace)
    private fun spec(key: Key) = work.getWorkInfosForUniqueWork(Jobs.name(key)).get().single { !it.state.isFinished }
        .let { work.workDatabase.workSpecDao().getWorkSpec(it.id.toString())!! }
    private fun pending(key: Key) = work.getWorkInfosForUniqueWork(Jobs.name(key)).get().count { !it.state.isFinished }
    private fun worker(key: Key) = TestWorkerBuilder<TransferWorker>(app, Executor { it.run() }, Data.Builder()
        .putString("namespace", key.namespace.name).putBoolean("agent", key.namespace.agent).putString("id", key.id)
        .putString("generation", key.generation).build()).build()
    private fun custody() = PendingIntent.getService(app, 0, Intent(app, TransferJob::class.java).setAction("exact.transfer.custody"),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_NO_CREATE)
    private fun params(job: JobInfo, userInitiated: Boolean = true, stop: Int = JobParameters.STOP_REASON_UNDEFINED): JobParameters =
        Shadow.newInstanceOf(JobParameters::class.java).also {
            ReflectionHelpers.setField(it, "jobId", job.id)
            ReflectionHelpers.setField(it, "extras", PersistableBundle(job.extras))
            ReflectionHelpers.setField(it, "mIsUserInitiated", userInitiated)
            ReflectionHelpers.setField(it, "mStopReason", stop)
        }
    /** A process start as the platform remembers it: [launched] is null for one with no launch time. */
    private fun started(launched: Long?, forced: Boolean): ApplicationStartInfo =
        ReflectionHelpers.callConstructor(ApplicationStartInfo::class.java).also {
            if (launched != null) ReflectionHelpers.callInstanceMethod<Unit>(it, "addStartupTimestamp",
                ClassParameter.from(Int::class.javaPrimitiveType, ApplicationStartInfo.START_TIMESTAMP_LAUNCH),
                ClassParameter.from(Long::class.javaPrimitiveType, launched))
            ReflectionHelpers.callInstanceMethod<Unit>(it, "setForceStopped", ClassParameter.from(Boolean::class.javaPrimitiveType, forced))
        }
    /**
     * A person's transfer whose user-initiated job a reboot lost, then the boot's broadcast in the next process over
     * [store]. That process performs its effects only when the test runs those waiting in the answer.
     */
    private fun rebooted(store: Registrations = Registrations(File(root, "private"))): Pair<Key, ArrayDeque<Runnable>> {
        process()
        val key = start(person = true)
        assertEquals(Job.UserJob, record(key).job)
        scheduler().cancelAll()
        Settings.Global.putInt(app.contentResolver, "boot_count", 11)
        val effects = ArrayDeque<Runnable>()
        process(namespace = null, control = { effects += it }, store = store)
        TransferBoot().onReceive(app, Intent(Intent.ACTION_BOOT_COMPLETED))
        return key to effects
    }
    private fun perform(effects: ArrayDeque<Runnable>) { while (effects.isNotEmpty()) effects.removeFirst().run() }
    private fun recovery() = work.getWorkInfosForUniqueWork("exact.transfers.recover").get().single()
    private fun blocked(answer: String): Pair<CountDownLatch, CountDownLatch> {
        val sending = CountDownLatch(1)
        val release = CountDownLatch(1)
        transport.run = { sending.countDown(); assertTrue(release.await(5, TimeUnit.SECONDS)); JSONObject().put("failure", answer) }
        return sending to release
    }

    @Test fun aUserInitiatedJobCarriesOnlyItsIdentityAndRequiresNetwork() {
        process()
        val key = start(person = true)
        assertEquals(Lifecycle.Intent.Person to Job.UserJob, record(key).let { it.intent to it.job })
        val job = scheduler().allPendingJobs.single()
        assertTrue(job.isUserInitiated)
        assertNotNull(job.requiredNetwork)
        assertEquals(0L, job.estimatedNetworkUploadBytes)
        assertEquals(JobInfo.NETWORK_BYTES_UNKNOWN.toLong(), job.estimatedNetworkDownloadBytes)
        assertEquals(setOf("namespace", "agent", "id", "generation"), job.extras.keySet())
        assertEquals(key, Jobs.key(job.extras.getString("namespace"), job.extras.getBoolean("agent"), job.extras.getString("id"), job.extras.getString("generation")))
        assertEquals(setOf(key), jobs.held(Fixture.app))
        assertTrue(jobs.held(drive).isEmpty())
        assertEquals(0, work.getWorkInfosForUniqueWork(Jobs.name(key)).get().size)
        jobs.cancel(key, Job.Work)
        assertEquals(setOf(key), jobs.held(Fixture.app))
        jobs.cancel(key, Job.UserJob)
        assertTrue(jobs.held(Fixture.app).isEmpty())
    }

    @Test fun anUploadNotYetCopiedHasNoUploadEstimate() {
        process()
        transport.snapshot = {
            assertEquals(JobInfo.NETWORK_BYTES_UNKNOWN.toLong(), scheduler().allPendingJobs.single().estimatedNetworkUploadBytes)
            it.writeText("upload")
        }
        val key = start(person = true, upload = true)
        assertEquals(6L, transfers.read(key)!!.getLong("uploadTotal"))
    }

    @Test fun aRefusedUserInitiatedJobIsBridgedByWork() {
        process()
        val key = Fixture.key("video")
        shadowOf(scheduler()).failOnJob(Jobs.notificationId(key))
        Jobs.visible = { true }
        assertTrue(Fixture.start(transfers, key, root, Lifecycle.Intent.Person))
        assertEquals(Lifecycle.Intent.Person to Job.Work, record(key).let { it.intent to it.job })
        assertTrue(scheduler().allPendingJobs.isEmpty())
        assertEquals(1, pending(key))
    }

    @Test @Config(sdk = [33]) fun belowApi34ThereIsNoUserInitiatedJob() {
        process()
        var held: Boolean? = null
        val key = start()
        jobs.schedule(key, Job.UserJob, transfers.store.get(key)!!) { held = it }
        assertEquals(false, held)
        assertTrue(jobs.held(Fixture.app).isEmpty())
        jobs.cancel(key, null)
        assertEquals(0, pending(key))
    }

    @Test fun workCarriesItsIdentityBacksOffAndIsKept() {
        process()
        val key = start()
        val spec = spec(key)
        assertEquals(setOf("namespace", "agent", "id", "generation"), spec.input.keyValueMap.keys)
        assertEquals(key.generation, spec.input.getString("generation"))
        assertEquals(30_000L, spec.backoffDelayDuration)
        assertEquals(NetworkType.CONNECTED, spec.constraints.requiredNetworkType)
        var held: Boolean? = null
        jobs.schedule(key, Job.Work, transfers.store.get(key)!!) { held = it }
        assertEquals(true, held)
        assertEquals("work the platform holds is kept", spec.id, spec(key).id)
        jobs.cancel(key, Job.UserJob)
        assertEquals(1, pending(key))
        jobs.cancel(key, Job.Work)
        assertEquals(0, pending(key))
    }

    @Test fun eachNamespaceHasItsOwnJobs() {
        process(drive)
        val driven = start(namespace = drive)
        process()
        val key = start()
        assertEquals("exact.transfer.agent.0123456789abcdef.${driven.generation}", Jobs.name(driven))
        assertEquals("exact.transfer.app.app.${key.generation}", Jobs.name(key))
        assertEquals(1, pending(driven))
        assertEquals(true, spec(driven).input.getBoolean("agent", false))
        jobs.cancel(key, null)
        assertEquals(1, pending(driven))
        assertNull(Jobs.key("app", false, "video", "not-a-generation"))
        assertNull(Jobs.key("app", false, null, key.generation))
    }

    @Test fun theNotificationHasTitleProgressAndAnImmutableActivation() {
        process()
        val key = start()
        val record = transfers.store.get(key)!!.put("uploadTotal", 100).put("uploaded", 50)
        val notification = Jobs.notification(app, key, record)
        assertEquals("Upload video", notification.extras.getString(Notification.EXTRA_TITLE))
        assertEquals(500, notification.extras.getInt(Notification.EXTRA_PROGRESS))
        assertEquals("exact.transfers", notification.channelId)
        assertTrue(notification.contentIntent.isImmutable)
        // Progress reaches only a notification the platform shows.
        val manager = shadowOf(app.getSystemService(NotificationManager::class.java))
        jobs.progress(key, record)
        assertNull(manager.getNotification(Jobs.notificationId(key)))
        Jobs.shown += key
        jobs.progress(key, record)
        assertNotNull(manager.getNotification(Jobs.notificationId(key)))
    }

    @Test fun conditionsAreReadFromThePlatform() {
        jobs = Jobs(app)
        assertEquals(Lifecycle.Conditions(34, visible = false, notifications = true), jobs.conditions)
        Jobs.visible = { true }
        shadowOf(app.getSystemService(NotificationManager::class.java)).setNotificationsEnabled(false)
        assertEquals(Lifecycle.Conditions(34, visible = true, notifications = false), jobs.conditions)
        // With no module loaded nothing shows.
        Jobs.visible = null
        assertFalse(jobs.conditions.visible)
    }

    @Test fun aCancelledCustodyTokenIsAForceStopUntilItIsAcknowledged() {
        jobs = Jobs(app)
        assertFalse("nothing was kept", jobs.stopped())
        assertNull("reading keeps nothing", custody())
        jobs.acknowledge(Fixture.app)
        assertFalse(jobs.stopped())
        custody()!!.cancel()
        assertTrue(jobs.stopped())
        assertTrue("the evidence stays until it is acknowledged", jobs.stopped())
        jobs.acknowledge(drive)
        assertFalse(jobs.stopped())
        // A token lost to a reboot is no force-stop.
        custody()!!.cancel()
        Settings.Global.putInt(app.contentResolver, "boot_count", 11)
        assertFalse(jobs.stopped())
    }

    @Test @Config(sdk = [35]) fun fromApi35AForceStopIsReadFromTheStartHistory() {
        jobs = Jobs(app)
        var starts = listOf(30L to false, 20L to true, 10L to false)
        jobs.starts = { starts }
        assertTrue(jobs.stopped())
        jobs.acknowledge(Fixture.app)
        assertNull("no token is kept from API 35", custody())
        assertFalse(jobs.stopped())
        starts = listOf(50L to false, 40L to true) + starts
        assertTrue(jobs.stopped())
        assertTrue(jobs.stopped())
        jobs.acknowledge(Fixture.app)
        assertFalse(jobs.stopped())
        assertFalse(Jobs.forced(emptyList(), -1))
        assertFalse(Jobs.forced(listOf(30L to false, 20L to true), 30))
        // A start the platform forgot leaves every one it remembers as new.
        assertTrue(Jobs.forced(listOf(30L to false, 20L to true), 5))
        // A start with no launch time is new too, since the acknowledged time never names it.
        assertTrue(Jobs.forced(listOf(null to true, 30L to false), 30))
        assertFalse(Jobs.forced(listOf(null to false, 30L to true), 30))
    }

    @Test @Config(sdk = [35], shadows = [StartHistory::class]) fun aStartWithNoLaunchTimeIsNeverTheOneAcknowledged() {
        jobs = Jobs(app)
        StartHistory.starts = listOf(started(null, forced = false))
        assertFalse(jobs.stopped())
        jobs.acknowledge(Fixture.app)
        // A person force-stops the app, and the start that follows has no launch time either.
        StartHistory.starts = listOf(started(null, forced = true)) + StartHistory.starts
        assertTrue(jobs.stopped())
        // Nothing names that start, so acknowledging it rearms nothing.
        jobs.acknowledge(Fixture.app)
        assertTrue(jobs.stopped())
        // A later start with a launch time is one to acknowledge, and what came before it is then old.
        StartHistory.starts = listOf(started(100, forced = false)) + StartHistory.starts
        assertTrue(jobs.stopped())
        jobs.acknowledge(Fixture.app)
        assertFalse(jobs.stopped())
        StartHistory.starts = listOf(started(null, forced = false)) + StartHistory.starts
        assertFalse(jobs.stopped())
        jobs.acknowledge(Fixture.app)
        StartHistory.starts = listOf(started(null, forced = true)) + StartHistory.starts
        assertTrue(jobs.stopped())
    }

    @Test fun aRebootOrAnUpdateIsReadOnceByEachNamespace() {
        jobs = Jobs(app)
        assertTrue(jobs.restarted(Fixture.app))
        jobs.acknowledge(Fixture.app)
        assertFalse(jobs.restarted(Fixture.app))
        assertTrue(jobs.restarted(drive))
        Settings.Global.putInt(app.contentResolver, "boot_count", 11)
        assertTrue(jobs.restarted(Fixture.app))
        jobs.acknowledge(Fixture.app)
        shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).lastUpdateTime = 77
        assertTrue(jobs.restarted(Fixture.app))
    }

    @Test fun aWorkerRunsUntilTheLifecycleFinishesIt() {
        process()
        val key = start()
        // A later process: the worker is its first entrant.
        process(namespace = null)
        val worker = worker(key)
        var result: ListenableWorker.Result? = null
        val running = Thread { result = worker.doWork() }.apply { start() }
        Fixture.await("the ending's wake") { host.raised.isNotEmpty() }
        assertEquals(setOf(Fixture.app), transfers.state().first.crossed)
        assertEquals(Result.Success, record(key).result)
        assertTrue("the run is held through the wake", running.isAlive)
        host.raised.single().third(Outcome.Completed)
        running.join(5000)
        assertEquals("the work delivers the ending again", ListenableWorker.Result.retry(), result)
        assertTrue(Jobs.finishers.isEmpty())
    }

    @Test fun aWorkersStopIsReportedWithItsReason() {
        process(threaded = true)
        Fixture.await("the session") { transfers.state().first.session != null }
        val kept = start("kept")
        val stopped = start("stopped")
        Fixture.await("both starts") { host.answers.size == 2 }
        for ((key, reason) in listOf(kept to WorkInfo.STOP_REASON_TIMEOUT, stopped to WorkInfo.STOP_REASON_USER)) {
            val (sending, release) = blocked("cancelled")
            val worker = worker(key)
            var result: ListenableWorker.Result? = null
            val running = Thread { result = worker.doWork() }.apply { start() }
            assertTrue(sending.await(5, TimeUnit.SECONDS))
            worker.stop(reason)
            release.countDown()
            running.join(5000)
            if (key == kept) {
                assertEquals(ListenableWorker.Result.retry(), result)
                assertEquals(Triple(Phase.Ready, 1, Job.Work), record(key).let { Triple(it.phase, it.stops, it.job) })
                assertEquals(1, pending(key))
            } else {
                assertEquals(ListenableWorker.Result.success(), result)
                assertEquals(Result.Aborted to null, record(key).let { it.result to it.job })
                Fixture.await("the work's release") { pending(key) == 0 }
            }
        }
    }

    @Test @Config(sdk = [30]) fun belowApi31EveryStopIsThePlatforms() {
        process(threaded = true)
        Fixture.await("the session") { transfers.state().first.session != null }
        val key = start()
        Fixture.await("the start") { host.answers.isNotEmpty() }
        val (sending, release) = blocked("cancelled")
        val worker = worker(key)
        val running = Thread { worker.doWork() }.apply { start() }
        assertTrue(sending.await(5, TimeUnit.SECONDS))
        worker.stop(WorkInfo.STOP_REASON_USER)
        release.countDown()
        running.join(5000)
        assertEquals(Triple(Phase.Ready, 1, null), record(key).let { Triple(it.phase, it.stops, it.result) })
    }

    @Test fun aStopBeforeTheWorkerRunsStartsNothing() {
        process()
        val key = start()
        val worker = worker(key)
        worker.stop(WorkInfo.STOP_REASON_TIMEOUT)
        assertEquals(ListenableWorker.Result.retry(), worker.doWork())
        assertTrue(transfers.state().first.runs.isEmpty())
        assertFalse("run" in transport.ops())
        assertEquals(0, record(key).stops)
    }

    @Test fun aWorkerThatFollowsAForceStopSendsNothing() {
        process()
        val key = start()
        custody()!!.cancel()
        process(namespace = null)
        assertEquals(ListenableWorker.Result.success(), worker(key).doWork())
        assertFalse("run" in transport.ops())
        assertEquals(Triple(Result.Aborted, null, 3), record(key).let { Triple(it.result, it.job, it.wakes) })
        assertEquals(0, pending(key))
        assertNotNull("the evidence is rearmed", custody())
        assertTrue(transfers.store.durable().stopping.isEmpty())
    }

    @Test fun aUserInitiatedJobShowsItsNotificationAndRunsUntilFinished() {
        process()
        val key = start(person = true)
        val service = Robolectric.buildService(TransferJob::class.java).create().get()
        val params = params(scheduler().allPendingJobs.single())
        assertTrue(service.onStartJob(params))
        assertTrue(key in Jobs.shown)
        assertEquals(key to 1, host.raised.single().let { it.first to it.second })
        assertFalse(shadowOf(service).isJobFinished)
        // The wake is over: work delivers the ending again, and the job ends with its notification.
        host.raised.single().third(Outcome.Completed)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(shadowOf(service).isJobFinished)
        assertTrue(shadowOf(service).isRescheduleNeeded)
        assertFalse(key in Jobs.shown)
        assertEquals(Job.Work, record(key).job)
        assertEquals(1, pending(key))
        assertTrue(scheduler().allPendingJobs.isEmpty())
        assertFalse("the run is over", service.onStopJob(params))
    }

    @Test fun aStoppedUserInitiatedJobAnswersAsTheLifecycleFinishesIt() {
        process(threaded = true)
        Fixture.await("the session") { transfers.state().first.session != null }
        val kept = start("kept", person = true)
        val stopped = start("stopped", person = true)
        Fixture.await("both starts") { host.answers.size == 2 }
        val service = Robolectric.buildService(TransferJob::class.java).create().get()
        for ((key, reason) in listOf(kept to JobParameters.STOP_REASON_DEVICE_STATE, stopped to JobParameters.STOP_REASON_USER)) {
            val job = scheduler().allPendingJobs.single { it.extras.getString("generation") == key.generation }
            val (sending, release) = blocked("cancelled")
            assertTrue(service.onStartJob(params(job)))
            assertTrue(sending.await(5, TimeUnit.SECONDS))
            val again = service.onStopJob(params(job, stop = reason))
            release.countDown()
            Fixture.await("the exchange's report") { transfers.state().first.draining.isEmpty() }
            if (key == kept) {
                assertTrue("the platform runs the job again", again)
                // A user-initiated job has no deadline: its stops are not counted.
                assertEquals(Triple(Phase.Ready, 0, Job.UserJob), record(key).let { Triple(it.phase, it.stops, it.job) })
            } else {
                assertFalse(again)
                assertEquals(Result.Aborted to null, record(key).let { it.result to it.job })
            }
        }
    }

    @Test fun aDemotedUserInitiatedJobHandsItsTransferToWork() {
        process()
        val key = start(person = true)
        val service = Robolectric.buildService(TransferJob::class.java).create().get()
        assertTrue(service.onStartJob(params(scheduler().allPendingJobs.single(), userInitiated = false)))
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse("run" in transport.ops())
        assertTrue(shadowOf(service).isJobFinished)
        assertEquals(Lifecycle.Intent.Person to Job.Work, record(key).let { it.intent to it.job })
        assertEquals(1, pending(key))
        assertTrue(scheduler().allPendingJobs.isEmpty())
    }

    @Test @Config(sdk = [33]) fun aLaunchPromotesAPersonsRunningWork() {
        process(threaded = true)
        Fixture.await("the session") { transfers.state().first.session != null }
        val key = start(person = true)
        Fixture.await("the start") { host.answers.isNotEmpty() }
        assertEquals(Lifecycle.Intent.Person to Job.Work, record(key).let { it.intent to it.job })
        val (sending, release) = blocked("network")
        val worker = worker(key)
        val running = Thread { worker.doWork() }.apply { start() }
        try {
            assertTrue(sending.await(5, TimeUnit.SECONDS))
            assertFalse(key in Jobs.shown)
            Jobs.visible = { true }
            transfers.feed(ViewAttached(Fixture.app, 1, transfers.conditions))
            Fixture.await("foreground execution") { key in Jobs.shown }
        } finally { release.countDown() }
        running.join(5000)
        assertFalse(key in Jobs.shown)
    }

    @Test @Config(sdk = [33]) fun aPersonsWorkThatStartsUnderAViewRunsInTheForeground() {
        process(threaded = true)
        Fixture.await("the session") { transfers.state().first.session != null }
        val key = start(person = true)
        Fixture.await("the start") { host.answers.isNotEmpty() }
        Jobs.visible = { true }
        transfers.feed(ViewAttached(Fixture.app, 1, transfers.conditions))
        val (sending, release) = blocked("network")
        val worker = worker(key)
        val running = Thread { worker.doWork() }.apply { start() }
        try {
            assertTrue(sending.await(5, TimeUnit.SECONDS))
            Fixture.await("foreground execution") { key in Jobs.shown }
        } finally { release.countDown() }
        running.join(5000)
        assertFalse(key in Jobs.shown)
    }

    @Test fun aBootEnqueuesTheRecoveryThatEveryNamespaceCrosses() {
        process(drive)
        val driven = start(namespace = drive)
        process()
        val key = start(person = true)
        assertEquals(Job.UserJob, record(key).job)
        // A reboot loses the user-initiated job and keeps the work.
        scheduler().cancelAll()
        Settings.Global.putInt(app.contentResolver, "boot_count", 11)
        process(namespace = null)
        val receiver = TransferBoot()
        receiver.onReceive(app, Intent(Intent.ACTION_TIME_CHANGED))
        assertTrue(work.getWorkInfosForUniqueWork("exact.transfers.recover").get().isEmpty())
        receiver.onReceive(app, Intent(Intent.ACTION_BOOT_COMPLETED))
        assertEquals(WorkInfo.State.SUCCEEDED, recovery().state)
        assertEquals(setOf(Fixture.app, drive), transfers.state().first.crossed)
        // Work bridges the person's transfer until a launch takes the job back.
        assertEquals(Lifecycle.Intent.Person to Job.Work, record(key).let { it.intent to it.job })
        assertEquals(1, pending(key))
        assertEquals(1, pending(driven))
        assertFalse(jobs.restarted(Fixture.app) || jobs.restarted(drive))
        assertFalse("run" in transport.ops())
    }

    @Test fun theRecoveryRunsUntilTheJobsItAskedForAreRecorded() {
        val (key, effects) = rebooted()
        // A process killed here has only this work to start it again.
        assertEquals(WorkInfo.State.RUNNING, recovery().state)
        assertNull(record(key).job)
        assertEquals(0, pending(key))
        perform(effects)
        assertEquals(WorkInfo.State.SUCCEEDED, recovery().state)
        assertEquals(Lifecycle.Intent.Person to Job.Work, record(key).let { it.intent to it.job })
        assertEquals(1, pending(key))
    }

    @Test fun aRecoveryWhoseJobsCannotBeRecordedRunsAgain() {
        var failing = false
        val (key, effects) = rebooted(object : Registrations(File(root, "private")) {
            @Synchronized override fun write(write: Lifecycle.Write, admission: Admission?, response: Response?) {
                if (failing) { failing = false; throw IOException("disk full") }
                super.write(write, admission, response)
            }
        })
        failing = true
        perform(effects)
        assertFalse("the write was tried", failing)
        // WorkManager runs it again after its backoff.
        assertEquals(WorkInfo.State.ENQUEUED to 1, recovery().let { it.state to it.runAttemptCount })
        assertNull(record(key).job)
    }

    @Test fun anUpdateEnqueuesTheRecoveryToo() {
        process(namespace = null)
        TransferBoot().onReceive(app, Intent(Intent.ACTION_MY_PACKAGE_REPLACED))
        assertEquals(1, work.getWorkInfosForUniqueWork("exact.transfers.recover").get().size)
    }
}
