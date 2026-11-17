package exact.backgroundtransfer

import android.app.ActivityManager
import android.app.ApplicationStartInfo
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import exact.backgroundtransfer.Lifecycle.Conditions
import exact.backgroundtransfer.Lifecycle.Finish
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.JobStarted
import exact.backgroundtransfer.Lifecycle.JobStopped
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Phase
import exact.backgroundtransfer.Lifecycle.Run
import exact.backgroundtransfer.Lifecycle.Stop
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.resume

/**
 * The platform's half of the lifecycle on Android (LLP 1133.017 D4): user-initiated jobs,
 * WorkManager work and their notification, and what an entrant reads. Each callback of
 * [TransferWorker], [TransferJob] and [TransferBoot] becomes one event of [Transfers].
 */
internal class Jobs(private val app: Context) : Platform {
    companion object {
        /** The process's transfers, made by its first entrant: a worker, a job, a receiver's work or the module. */
        @Volatile var installed: Transfers? = null
        fun transfers(context: Context): Transfers = installed ?: synchronized(this) {
            installed ?: context.applicationContext.let { app ->
                // A scheduler can start this process without an activity to initialize TLS.
                dev.exact.canvas.Native.trust(app)
                Sessions.application = app
                Transfers(Registrations(File(app.noBackupFilesDir, "exact-transfers")), NativeTransfer::exchange, Jobs(app), Sessions,
                    Executors.newSingleThreadExecutor { work -> Thread(work, "exact-transfer-control") }).also { installed = it }
            }
        }

        val runs = AtomicInteger()
        /** How each run the platform started is ended, by its id, until the lifecycle's [Finish]. */
        val finishers = ConcurrentHashMap<Int, (Boolean) -> Unit>()
        /** How each running worker is given foreground execution. */
        val promoters = ConcurrentHashMap<Int, (JSONObject) -> Unit>()
        /** The registrations whose notification the platform shows now. */
        val shown: MutableSet<Key> = ConcurrentHashMap.newKeySet()
        /** Whether an activity shows the app, while the module that knows is loaded. */
        @Volatile var visible: (() -> Boolean)? = null

        private fun space(namespace: Namespace) = (if (namespace.agent) "agent." else "app.") + namespace.name
        fun name(key: Key) = "exact.transfer.${space(key.namespace)}.${key.generation}"
        fun notificationId(key: Key) = key.generation.hashCode() and 0x7fffffff

        /** A registration as a job's extras and a worker's input carry it: its identity and nothing else. */
        fun key(namespace: String?, agent: Boolean, id: String?, generation: String?): Key? {
            if (namespace == null || id == null || generation == null) return null
            if (runCatching { UUID.fromString(generation).toString() == generation }.getOrDefault(false).not()) return null
            return Key(Namespace(namespace, agent), id, generation)
        }

        /**
         * Whether a process start newer than the one last acknowledged followed a force-stop. [starts] are newest
         * first. A start with no launch time is never the acknowledged one, so it and every start before it count
         * as newer.
         */
        fun forced(starts: List<Pair<Long?, Boolean>>, acknowledged: Long): Boolean =
            starts.takeWhile { it.first != acknowledged }.any { it.second }

        fun notification(app: Context, key: Key, record: JSONObject): Notification {
            val manager = app.getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(NotificationChannel("exact.transfers", "Transfers", NotificationManager.IMPORTANCE_LOW))
            // The host's unexported activation alias carries a tap even with no live session.
            val intent = dev.exact.canvas.ExtensionContext.activation(app, "background-transfer", "backgroundfetchclick",
                JSONArray().put(Registrations.publicRecord(record))).setData(Uri.parse("exact-transfer:${key.generation}"))
            val tap = PendingIntent.getActivity(app, 0, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            val total = record.optLong("uploadTotal") + record.optLong("downloadTotal")
            val done = record.optLong("uploaded") + record.optLong("downloaded") + record.optLong("receiving")
            val icon = app.resources.getIdentifier("exact_notification", "drawable", app.packageName).takeIf { it != 0 }
                ?: android.R.drawable.stat_sys_upload
            return Notification.Builder(app, "exact.transfers").setSmallIcon(icon).setContentTitle(record.optString("title"))
                .setContentText("${android.text.format.Formatter.formatShortFileSize(app, done)} transferred")
                .setContentIntent(tap).setOnlyAlertOnce(true).setOngoing(true)
                .setProgress(1000, if (total > 0) ((done.toDouble() / total) * 1000).toInt().coerceIn(0, 1000) else 0, total <= 0).build()
        }
    }

    /**
     * The launch time of each process start the platform remembers, newest first, and whether a
     * force-stop came before it
     * ([ActivityManager.getHistoricalProcessStartReasons](https://developer.android.com/reference/android/app/ActivityManager#getHistoricalProcessStartReasons(int)),
     * API 35). The launch time names a start; it is null for a start the platform recorded none for
     * ([ApplicationStartInfo.getStartupTimestamps](https://developer.android.com/reference/android/app/ApplicationStartInfo#getStartupTimestamps())).
     */
    var starts: () -> List<Pair<Long?, Boolean>> = {
        if (Build.VERSION.SDK_INT < 35) emptyList()
        else app.getSystemService(ActivityManager::class.java).getHistoricalProcessStartReasons(32).map {
            it.startupTimestamps[ApplicationStartInfo.START_TIMESTAMP_LAUNCH] to it.wasForceStopped()
        }
    }

    private val prefs get() = app.getSharedPreferences("exact.transfer.custody", Context.MODE_PRIVATE)
    private val custody get() = Intent(app, TransferJob::class.java).setAction("exact.transfer.custody")
    private val boot get() = android.provider.Settings.Global.getInt(app.contentResolver, "boot_count", -1)
    private val updated get() = app.packageManager.getPackageInfo(app.packageName, 0).lastUpdateTime
    private val schedulers = ConcurrentHashMap<Namespace, JobScheduler>()
    /** The namespace's own job ids: a registration of one cannot name a job of another. */
    fun scheduler(namespace: Namespace): JobScheduler = schedulers.computeIfAbsent(namespace) {
        app.getSystemService(JobScheduler::class.java).forNamespace("exact.transfers.${space(it)}")
    }
    private fun key(job: JobInfo, namespace: Namespace): Key? =
        key(namespace.name, namespace.agent, job.extras.getString("id"), job.extras.getString("generation"))

    override val conditions: Conditions get() = Conditions(Build.VERSION.SDK_INT, visible?.invoke() ?: false,
        app.getSystemService(NotificationManager::class.java).areNotificationsEnabled())

    /**
     * From API 35 the platform says which process starts followed a force-stop. Below it, a
     * force-stop is read from the cancelled pending intent this app kept, which is good only
     * within the boot and the install it was kept in.
     */
    override fun stopped(): Boolean {
        if (Build.VERSION.SDK_INT >= 35) return forced(starts(), prefs.getLong("start", -1))
        val kept = PendingIntent.getService(app, 0, custody, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_NO_CREATE)
        return kept == null && prefs.getBoolean("kept", false) && boot >= 0 && prefs.getInt("boot", -2) == boot &&
            prefs.getLong("updated", -1) == updated
    }

    override fun restarted(namespace: Namespace): Boolean = space(namespace).let {
        prefs.getInt("boot.$it", -2) != boot || prefs.getLong("updated.$it", -1) != updated
    }

    override fun held(namespace: Namespace): Set<Key> =
        if (Build.VERSION.SDK_INT < 34) emptySet() else scheduler(namespace).allPendingJobs.mapNotNull { key(it, namespace) }.toSet()

    override fun acknowledge(namespace: Namespace) {
        val edit = prefs.edit().putInt("boot.${space(namespace)}", boot).putLong("updated.${space(namespace)}", updated)
        // The newest start that has a launch time: one with none cannot be named, and stays evidence.
        if (Build.VERSION.SDK_INT >= 35) starts().firstNotNullOfOrNull { it.first }?.let { edit.putLong("start", it) } else {
            PendingIntent.getService(app, 0, custody, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
            edit.putBoolean("kept", true).putInt("boot", boot).putLong("updated", updated)
        }
        check(edit.commit())
    }

    override fun schedule(key: Key, job: Job, record: JSONObject, answer: (Boolean) -> Unit) {
        if (job == Job.UserJob) return answer(Build.VERSION.SDK_INT >= 34 && runCatching { userJob(key, record) }.getOrDefault(false))
        val stored = runCatching {
            val request = OneTimeWorkRequestBuilder<TransferWorker>()
                .setInputData(Data.Builder().putString("namespace", key.namespace.name).putBoolean("agent", key.namespace.agent)
                    .putString("id", key.id).putString("generation", key.generation).build())
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()).build()
            // Work the platform already holds is kept.
            WorkManager.getInstance(app).enqueueUniqueWork(name(key), ExistingWorkPolicy.KEEP, request).result
        }.getOrNull() ?: return answer(false)
        stored.addListener({ answer(runCatching { stored.get() }.isSuccess) }, Runnable::run)
    }

    @androidx.annotation.RequiresApi(34)
    private fun userJob(key: Key, record: JSONObject): Boolean {
        val scheduler = scheduler(key.namespace)
        var id = notificationId(key)
        while (scheduler.getPendingJob(id)?.let { key(it, key.namespace) != key } == true) id++
        val extras = PersistableBundle().apply {
            putString("namespace", key.namespace.name); putBoolean("agent", key.namespace.agent)
            putString("id", key.id); putString("generation", key.generation)
        }
        // The upload's size is unknown until its snapshot is committed.
        val copying = record.getJSONObject("state").getString("phase").let { it == Phase.Admitting.name || it == Phase.Preparing.name }
        val requests = record.getJSONArray("requests")
        val upload = if (copying && (0 until requests.length()).any { requests.getJSONObject(it).getBoolean("body") }) null
            else record.getLong("uploadTotal")
        val download = if (record.isNull("downloadTotal")) null else record.getLong("downloadTotal")
        val unknown = JobInfo.NETWORK_BYTES_UNKNOWN.toLong()
        val request = JobInfo.Builder(id, ComponentName(app, TransferJob::class.java)).setExtras(extras)
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY).setUserInitiated(true)
            .setEstimatedNetworkBytes(download ?: unknown, upload ?: unknown).build()
        return scheduler.schedule(request) == JobScheduler.RESULT_SUCCESS
    }

    override fun cancel(key: Key, job: Job?) {
        if (job != Job.UserJob) WorkManager.getInstance(app).cancelUniqueWork(name(key))
        if (job != Job.Work && Build.VERSION.SDK_INT >= 34) scheduler(key.namespace).let { scheduler ->
            scheduler.allPendingJobs.filter { key(it, key.namespace) == key }.forEach { scheduler.cancel(it.id) }
        }
    }

    override fun finish(key: Key, run: Int, again: Boolean) { finishers.remove(run)?.invoke(again) }
    override fun promote(key: Key, run: Int, record: JSONObject) { promoters[run]?.invoke(record) }
    override fun progress(key: Key, record: JSONObject) {
        if (key in shown) app.getSystemService(NotificationManager::class.java).notify(notificationId(key), notification(app, key, record))
    }
}

/** WorkManager's run of a transfer. It ends when the lifecycle finishes it; a stop is reported and finishes it too. */
class TransferWorker(app: Context, params: WorkerParameters) : Worker(app, params) {
    private val run = Jobs.runs.incrementAndGet()
    private var stopped = false
    private val key get() = Jobs.key(inputData.getString("namespace"), inputData.getBoolean("agent", false),
        inputData.getString("id"), inputData.getString("generation"))

    override fun doWork(): Result {
        val key = key ?: return Result.failure()
        val finished = CompletableFuture<Boolean>()
        Jobs.finishers[run] = { again -> finished.complete(again) }
        Jobs.promoters[run] = { record ->
            runCatching {
                setForegroundAsync(ForegroundInfo(Jobs.notificationId(key), Jobs.notification(applicationContext, key, record),
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)).let { it.addListener({ if (runCatching { it.get() }.isSuccess) Jobs.shown += key }, Runnable::run) }
            }
        }
        return try {
            val transfers = Jobs.transfers(applicationContext)
            transfers.enter(key.namespace)
            synchronized(this) {
                // A stop that came first was reported without this run, which therefore never starts.
                if (stopped) return Result.retry()
                transfers.feed(JobStarted(key, Run(run, Job.Work), transfers.conditions))
            }
            if (finished.get()) Result.retry() else Result.success()
        } catch (_: Exception) { Result.retry() } finally {
            Jobs.finishers.remove(run)
            Jobs.promoters.remove(run)
            Jobs.shown -= key
        }
    }

    override fun onStopped() {
        val key = key ?: return
        synchronized(this) { stopped = true }
        // Below API 31 the platform gives no reason, and every stop is its own.
        val person = Build.VERSION.SDK_INT >= 31 && stopReason == WorkInfo.STOP_REASON_USER
        runCatching {
            val transfers = Jobs.transfers(applicationContext)
            transfers.enter(key.namespace)
            transfers.feed(JobStopped(key, run, if (person) Stop.Person else Stop.System))
        }
    }
}

/** A user-initiated job's run of a transfer (API 34). */
class TransferJob : JobService() {
    /** One run: ended once, by the lifecycle's [Finish] or by the platform's stop. [again] is the lifecycle's answer. */
    private class Hold(val key: Key, val run: Int) { var again: Boolean? = null }
    private val holds = HashMap<Int, Hold>()
    private val main = Handler(Looper.getMainLooper())

    private fun key(params: JobParameters): Key? = params.extras.let {
        Jobs.key(it.getString("namespace"), it.getBoolean("agent", false), it.getString("id"), it.getString("generation"))
    }

    override fun onStartJob(params: JobParameters): Boolean {
        val key = key(params) ?: return false
        val hold = Hold(key, Jobs.runs.incrementAndGet())
        holds[params.jobId] = hold
        Jobs.finishers[hold.run] = { again ->
            synchronized(hold) { if (hold.again == null) hold.again = again }
            main.post { if (holds.remove(params.jobId, hold)) { Jobs.shown -= key; jobFinished(params, again) } }
        }
        try {
            val transfers = Jobs.transfers(this)
            transfers.enter(key.namespace)
            // A user-initiated job shows its notification as soon as it starts.
            transfers.store.get(key)?.let {
                setNotification(params, Jobs.notificationId(key), Jobs.notification(this, key, it), JOB_END_NOTIFICATION_POLICY_REMOVE)
                Jobs.shown += key
            }
            transfers.feed(JobStarted(key, Run(hold.run, Job.UserJob, demoted = !params.isUserInitiatedJob), transfers.conditions))
        } catch (_: Exception) { Jobs.finishers.remove(hold.run)?.invoke(true) }
        return true
    }

    /** Whether the platform runs the job again is the lifecycle's [Finish] for this run, read from the step that takes the stop. */
    override fun onStopJob(params: JobParameters): Boolean {
        val hold = holds.remove(params.jobId) ?: return false
        Jobs.shown -= hold.key
        val reason = if (params.stopReason == JobParameters.STOP_REASON_USER) Stop.Person else Stop.System
        val step = runCatching { Jobs.transfers(this).feed(JobStopped(hold.key, hold.run, reason)) }.getOrNull()
        val asked = step?.effects?.firstNotNullOfOrNull { effect -> (effect as? Finish)?.takeIf { it.run == hold.run }?.again }
        return synchronized(hold) { hold.again } ?: asked ?: (step == null)
    }
}

/**
 * The recovery a boot or an update owes: every namespace crosses the barrier, as any entrant does. A reboot leaves a
 * transfer whose user-initiated job it lost with nothing but this work to start its process, so the work runs until
 * the job that replaces each is recorded ([Transfers.recover]), and runs again when that did not happen. It waits on
 * no thread: WorkManager's own executor may be the one that stores the replacement.
 */
class TransferRecovery(app: Context, params: WorkerParameters) : CoroutineWorker(app, params) {
    override suspend fun doWork(): Result = try {
        val recorded = suspendCancellableCoroutine { waiting -> Jobs.transfers(applicationContext).recover { waiting.resume(it) } }
        if (recorded) Result.success() else Result.retry()
    } catch (e: CancellationException) { throw e } catch (_: Exception) { Result.retry() }
}

/** The receiver only enqueues the recovery, which is work and so survives the broadcast's short life. */
class TransferBoot : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED && intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
        val pending = goAsync()
        try {
            val operation = WorkManager.getInstance(context).enqueueUniqueWork("exact.transfers.recover",
                ExistingWorkPolicy.KEEP, OneTimeWorkRequestBuilder<TransferRecovery>().build())
            operation.result.addListener({ pending?.finish() }, context.mainExecutor)
        } catch (e: Exception) { pending?.finish(); throw e }
    }
}
