package dev.exact.canvas

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ApplicationInfo
import android.media.AudioAttributes
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import java.io.File
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import org.json.JSONObject

/** Manifest policy for Contract effects. Mixing never requests focus or ducks another app. */
internal data class SoundPolicy(val usage: Int, val content: Int, val focus: String?, val obeyRinger: Boolean) {
    fun attributes(): AudioAttributes = AudioAttributes.Builder().setUsage(usage).setContentType(content).build()
    companion object {
        fun of(session: String) = when (session) {
            "playback" -> SoundPolicy(AudioAttributes.USAGE_MEDIA, AudioAttributes.CONTENT_TYPE_MUSIC, "playback", false)
            "playback-mix" -> SoundPolicy(AudioAttributes.USAGE_MEDIA, AudioAttributes.CONTENT_TYPE_MUSIC, null, false)
            else -> SoundPolicy(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION, AudioAttributes.CONTENT_TYPE_SONIFICATION, null, true)
        }
    }
}

/** The native library has no initializer reachable before the first play. */
internal object SoundNative {
    init { System.loadLibrary("exact_sound") }
    @JvmStatic external fun open(files: Array<String>, usage: Int, content: Int, decoded: BooleanArray): Long
    @JvmStatic external fun resume(handle: Long, files: Array<String>, usage: Int, content: Int, decoded: BooleanArray): Boolean
    @JvmStatic external fun pause(handle: Long)
    @JvmStatic external fun op(handle: Long, kind: Int, id: Long, sound: Int, at: Double, gain: Float, rate: Float, pan: Float, ramp: Double): Boolean
    @JvmStatic external fun counts(handle: Long): DoubleArray
    @JvmStatic external fun close(handle: Long)
}

internal interface SoundBackend {
    fun open(files: Array<String>, policy: SoundPolicy, decoded: BooleanArray): Long
    fun resume(handle: Long, files: Array<String>, policy: SoundPolicy, decoded: BooleanArray): Boolean = true
    fun pause(handle: Long) {}
    fun op(handle: Long, op: JSONObject): Boolean
    fun counts(handle: Long): DoubleArray
    fun close(handle: Long)
}

private object NativeSoundBackend : SoundBackend {
    override fun open(files: Array<String>, policy: SoundPolicy, decoded: BooleanArray) = SoundNative.open(files, policy.usage, policy.content, decoded)
    override fun resume(handle: Long, files: Array<String>, policy: SoundPolicy, decoded: BooleanArray) = SoundNative.resume(handle, files, policy.usage, policy.content, decoded)
    override fun pause(handle: Long) = SoundNative.pause(handle)
    override fun op(handle: Long, op: JSONObject): Boolean {
        val kind = op.getInt("kind")
        val unchanged = if (kind == 4) Double.NaN else 1.0
        return SoundNative.op(handle, kind, op.optLong("id"), op.optInt("sound"), op.optDouble("at", 0.0),
            op.optDouble("gain", unchanged).toFloat(), op.optDouble("rate", unchanged).toFloat(),
            op.optDouble("pan", if (kind == 4) Double.NaN else 0.0).toFloat(), op.optDouble("ramp", 0.0))
    }
    override fun counts(handle: Long) = SoundNative.counts(handle)
    override fun close(handle: Long) = SoundNative.close(handle)
}

/** Main-thread owner of Contract effects. Native code runs on AAudio's callback thread; WAV decode and stream lifecycle run off-main on [executor]. */
internal class SoundOutput(
    private val context: Context,
    /** Where a sound's path is a file ([AssetRoot]), asked at each open. */
    private val root: () -> File,
    private val policy: SoundPolicy,
    private val backend: SoundBackend = NativeSoundBackend,
    private val report: ((String) -> Unit)? = null,
    private val now: () -> Double = { System.nanoTime() / 1e9 },
    private val executor: Executor = defaultExecutor,
) {
    private data class QueuedOp(val op: JSONObject, val submitted: Double)

    private val debug = context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0
    private var loggedAt = Double.NEGATIVE_INFINITY
    private val main = Handler(Looper.getMainLooper())
    private var handle = 0L
    private var running = false
    private var opening = false
    private var openingGen = 0
    private var destroyed = false
    private var files = emptyList<String>()
    private val reportedFailures = mutableSetOf<String>()
    private var decodedCount = 0
    private var claim: AudioClaim? = null
    private var foreground = true
    private var interrupted = false
    private var skipped = 0L
    private var status = "closed"
    private var error: String? = null
    private var failedAt = Double.NEGATIVE_INFINITY
    private var idleAt: Double? = null
    private var discardedBefore = Double.NEGATIVE_INFINITY
    private val pending = ArrayDeque<QueuedOp>()
    private var listening = false
    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY ||
                (intent.action == AudioManager.RINGER_MODE_CHANGED_ACTION && silent())) stop()
        }
    }
    private val poll = Runnable { drain() }

    /** The running generation's assets moved ([AssetRoot]): drop cached PCM so the next batch decodes the new generation's files (LLP 1156.008.001 B7.1). */
    fun reroot() {
        close(release = true, freeCache = true)
        files = emptyList()
        reportedFailures.clear()
        status = "closed"
        error = null
        failedAt = Double.NEGATIVE_INFINITY
    }

    fun apply(body: JSONObject) {
        val names = body.getJSONArray("files").let { a -> List(a.length()) { a.getString(it) } }
        if (body.optBoolean("reset") || names != files) {
            close(release = true, freeCache = true)
            files = names
            reportedFailures.clear()
            status = "closed"
            error = null
            failedAt = Double.NEGATIVE_INFINITY
        }
        val ops = body.getJSONArray("ops")
        val submitted = body.optDouble("submitted", now())
        val before = skipped
        if (debug && ops.length() != 0) Log.d("exact", "sound batch: now=${now()} submitted=${body.optDouble("submitted")} cutoff=$discardedBefore foreground=$foreground interrupted=$interrupted ops=$ops")
        for (i in 0 until ops.length()) {
            val op = ops.getJSONObject(i)
            if (op.getInt("kind") == 1) {
                if (!foreground || interrupted || silent() || submitted < discardedBefore || op.getDouble("at") < discardedBefore) { skipped++; continue }
                if (!ensureOpeningOrRunning()) { skipped++; continue }
                idleAt = null
            }
            if (running || opening) pending.addLast(QueuedOp(op, submitted))
        }
        if (debug && skipped != before) Log.d("exact", "sound skipped: batch=${skipped - before} total=$skipped silent=${silent()}")
        drain()
    }

    private fun silent() = policy.obeyRinger && context.getSystemService(AudioManager::class.java).ringerMode != AudioManager.RINGER_MODE_NORMAL

    private fun ensureOpeningOrRunning(): Boolean {
        if (running || opening) return true
        if (destroyed || now() - failedAt < 1) return false
        try {
            AudioFocus.application = context.applicationContext
            if (policy.focus != null && claim == null) {
                claim = AudioFocus.claim(policy.focus, policy.attributes()) { event ->
                    if (debug) Log.d("exact", "sound focus: now=${now()} event=$event handle=$handle foreground=$foreground")
                    when (event) {
                        "focus-gained" -> {
                            interrupted = false
                            // Interrupted effects were discarded; nothing is left to hold focus for.
                            if (!running && !opening) { claim?.release(); claim = null }
                        }
                        "focus-transient" -> { interrupted = true; stop(release = false) }
                        "focus" -> { interrupted = true; stop() }
                    }
                } ?: throw IllegalStateException("audio focus refused")
            }
            val root = root().canonicalFile
            val names = files.toList()
            val paths = names.map { name ->
                File(root, name).canonicalFile.also { require(it.path.startsWith(root.path + File.separator)) { "sound path leaves app assets" } }.path
            }.toTypedArray()
            val existing = handle
            val gen = ++openingGen
            opening = true
            status = "opening"
            error = null
            executor.execute {
                val decoded = BooleanArray(paths.size)
                var opened = existing
                var failure: String? = null
                try {
                    if (existing != 0L) {
                        check(backend.resume(existing, paths, policy, decoded)) { "sound output did not resume" }
                    } else {
                        opened = backend.open(paths, policy, decoded)
                        check(opened != 0L) { "sound output did not open" }
                    }
                } catch (e: Exception) {
                    failure = e.message ?: e.javaClass.simpleName
                    if (existing == 0L) opened = 0L
                } catch (e: LinkageError) {
                    failure = e.message ?: "sound library unavailable"
                    if (existing == 0L) opened = 0L
                }
                val finish = Runnable { finishOpen(gen, existing, opened, names, decoded, failure) }
                if (Looper.myLooper() == Looper.getMainLooper()) finish.run() else main.post(finish)
            }
            return true
        } catch (e: Exception) {
            fail(e.message ?: e.javaClass.simpleName)
        } catch (e: LinkageError) {
            fail(e.message ?: "sound library unavailable")
        }
        return false
    }

    private fun finishOpen(gen: Int, existing: Long, opened: Long, names: List<String>, decoded: BooleanArray, failure: String?) {
        if (gen != openingGen || destroyed) {
            if (failure == null && opened != 0L) {
                if (opened != handle) executor.execute { backend.close(opened) }
                else executor.execute { backend.pause(opened) }
            }
            return
        }
        opening = false
        if (failure != null) {
            fail(failure)
            publish()
            return
        }
        handle = opened
        running = true
        decodedCount = decoded.count { it }
        if (debug) Log.d("exact", "sound opened: now=${now()} handle=$handle resumed=${existing != 0L} decoded=$decodedCount/${names.size} focus=${claim != null}")
        names.forEachIndexed { i, name ->
            if (!decoded[i] && reportedFailures.add(name)) Log.w("exact", "sound: $name could not be decoded; its cues are dropped")
        }
        if (!listening) {
            val filter = IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY).apply { addAction(AudioManager.RINGER_MODE_CHANGED_ACTION) }
            if (Build.VERSION.SDK_INT >= 33) context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
            else context.registerReceiver(receiver, filter)
            listening = true
        }
        // Drop queued plays invalidated by a cutoff (e.g., transient pause) while open was in flight.
        val kept = ArrayDeque<QueuedOp>(pending.size)
        for (item in pending) {
            if (item.op.getInt("kind") == 1 && (item.submitted < discardedBefore || item.op.getDouble("at") < discardedBefore)) {
                skipped++
            } else kept.addLast(item)
        }
        pending.clear()
        pending.addAll(kept)
        status = "running"
        error = null
        drain()
    }

    private fun fail(reason: String) {
        close(release = true, freeCache = true)
        failedAt = now()
        error = reason
        status = "failed"
        Log.w("exact", "sound: $reason; a play after one second retries")
    }

    private fun drain() {
        main.removeCallbacks(poll)
        var counts: DoubleArray? = null
        if (running && handle != 0L) {
            while (pending.isNotEmpty() && backend.op(handle, pending.first().op)) pending.removeFirst()
            counts = backend.counts(handle)
            if (debug && now() - loggedAt >= 1) {
                loggedAt = now()
                Log.d("exact", "sound output: now=$loggedAt handle=$handle pending=${pending.size} live=${counts[0]} dropped=${counts[1]} late=${counts[2]} latency=${counts[3]} rate=${counts[4]} timestamp=${counts[5]} error=${counts[6]} queued=${counts[7]} underruns=${counts[8]}")
            }
            if (counts[6] != 0.0) { fail("AAudio stream error ${counts[6].toInt()}"); counts = null }
            else {
                val at = now()
                if (pending.isEmpty() && counts[0] == 0.0 && counts[7] == 0.0) {
                    val idle = idleAt ?: at.also { idleAt = it }
                    // Let the last rendered frames reach the speaker before pausing the stream and releasing focus (PCM cache stays warm).
                    if (at - idle > counts[3] + 0.05) { close(release = true, freeCache = false); status = "stopped"; counts = null }
                } else idleAt = null
            }
        }
        publish(counts)
        if (running && handle != 0L) main.postDelayed(poll, if (pending.isEmpty()) 50 else 5)
    }

    private fun publish(counts: DoubleArray? = null) {
        report?.invoke(snapshot(counts).toString())
    }

    fun state(): JSONObject = snapshot(if (running && handle != 0L) backend.counts(handle) else null)

    private fun snapshot(counts: DoubleArray?): JSONObject {
        val state = snapshot(status, skipped)
        error?.let { state.put("error", it) }
        counts?.let {
            state.put("live", it[0].toLong()).put("dropped", it[1].toLong()).put("late", it[2].toLong())
                .put("outputLatency", it[3]).put("sampleRate", it[4].toInt()).put("timestamp", it[5] == 1.0)
                .put("underruns", it[8].toInt()).put("decoded", "$decodedCount/${files.size}")
        }
        return state
    }

    companion object {
        private val defaultExecutor: Executor = Executors.newSingleThreadExecutor { r ->
            Thread(r, "exact-sound").apply { isDaemon = true }
        }

        fun closedState(): JSONObject = snapshot("closed", 0)

        private fun snapshot(status: String, skipped: Long): JSONObject =
            JSONObject().put("kind", "aaudio").put("state", status).put("skipped", skipped)
    }

    private fun close(release: Boolean = true, freeCache: Boolean = true) {
        main.removeCallbacks(poll)
        openingGen++
        opening = false
        pending.clear()
        val h = handle
        if (freeCache) {
            handle = 0L
            running = false
            if (h != 0L) {
                if (debug) Log.d("exact", "sound closed: now=${now()} handle=$h release=$release")
                executor.execute { backend.close(h) }
            }
        } else if (running && h != 0L) {
            running = false
            if (debug) Log.d("exact", "sound paused: now=${now()} handle=$h release=$release")
            executor.execute { backend.pause(h) }
        }
        if (release) { claim?.release(); claim = null }
        idleAt = null
    }

    private fun stop(release: Boolean = true) {
        discardedBefore = now()
        close(release, freeCache = false)
        status = "stopped"
        publish()
    }

    fun foreground(on: Boolean) {
        if (debug) Log.d("exact", "sound foreground: now=${now()} on=$on handle=$handle")
        foreground = on
        if (!on) stop()
        else { interrupted = false; discardedBefore = now() }
    }

    fun destroy() {
        destroyed = true
        discardedBefore = now()
        close(release = true, freeCache = true)
        status = "stopped"
        publish()
        if (listening) context.unregisterReceiver(receiver)
        listening = false
    }
}
