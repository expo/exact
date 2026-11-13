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
    @JvmStatic external fun op(handle: Long, kind: Int, id: Long, sound: Int, at: Double, gain: Float, rate: Float, pan: Float, ramp: Double): Boolean
    @JvmStatic external fun counts(handle: Long): DoubleArray
    @JvmStatic external fun close(handle: Long)
}

internal interface SoundBackend {
    fun open(files: Array<String>, policy: SoundPolicy, decoded: BooleanArray): Long
    fun op(handle: Long, op: JSONObject): Boolean
    fun counts(handle: Long): DoubleArray
    fun close(handle: Long)
}

private object NativeSoundBackend : SoundBackend {
    override fun open(files: Array<String>, policy: SoundPolicy, decoded: BooleanArray) = SoundNative.open(files, policy.usage, policy.content, decoded)
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

/** Main-thread owner of Contract effects. Native code alone runs on AAudio's callback thread. */
internal class SoundOutput(
    private val context: Context,
    private val policy: SoundPolicy,
    private val backend: SoundBackend = NativeSoundBackend,
    private val report: ((String) -> Unit)? = null,
    private val now: () -> Double = { System.nanoTime() / 1e9 },
) {
    private val debug = context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0
    private var loggedAt = Double.NEGATIVE_INFINITY
    private val main = Handler(Looper.getMainLooper())
    private var handle = 0L
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
    private val pending = ArrayDeque<JSONObject>()
    private var listening = false
    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY ||
                (intent.action == AudioManager.RINGER_MODE_CHANGED_ACTION && silent())) stop()
        }
    }
    private val poll = Runnable { drain() }

    fun apply(body: JSONObject) {
        val names = body.getJSONArray("files").let { a -> List(a.length()) { a.getString(it) } }
        if (body.optBoolean("reset") || names != files) { close(); files = names; reportedFailures.clear(); status = "closed"; error = null; failedAt = Double.NEGATIVE_INFINITY }
        val ops = body.getJSONArray("ops")
        val before = skipped
        if (debug && ops.length() != 0) Log.d("exact", "sound batch: now=${now()} submitted=${body.optDouble("submitted")} cutoff=$discardedBefore foreground=$foreground interrupted=$interrupted ops=$ops")
        for (i in 0 until ops.length()) {
            val op = ops.getJSONObject(i)
            if (op.getInt("kind") == 1) {
                if (!foreground || interrupted || silent() || body.optDouble("submitted", now()) < discardedBefore || op.getDouble("at") < discardedBefore) { skipped++; continue }
                if (!ready()) { skipped++; continue }
                idleAt = null
            }
            if (handle != 0L) pending.addLast(op)
        }
        if (debug && skipped != before) Log.d("exact", "sound skipped: batch=${skipped - before} total=$skipped silent=${silent()}")
        drain()
    }

    private fun silent() = policy.obeyRinger && context.getSystemService(AudioManager::class.java).ringerMode != AudioManager.RINGER_MODE_NORMAL

    private fun ready(): Boolean {
        if (handle != 0L) return true
        if (now() - failedAt < 1) return false
        try {
            AudioFocus.application = context.applicationContext
            if (policy.focus != null && claim == null) {
                claim = AudioFocus.claim(policy.focus, policy.attributes()) { event ->
                    if (debug) Log.d("exact", "sound focus: now=${now()} event=$event handle=$handle foreground=$foreground")
                    when (event) {
                        "focus-gained" -> {
                            interrupted = false
                            // Interrupted effects were discarded; nothing is left to hold focus for.
                            if (handle == 0L) { claim?.release(); claim = null }
                        }
                        "focus-transient" -> { interrupted = true; stop(release = false) }
                        "focus" -> { interrupted = true; stop() }
                    }
                } ?: throw IllegalStateException("audio focus refused")
            }
            val root = File(context.filesDir, "app").canonicalFile
            val paths = files.map { name ->
                File(root, name).canonicalFile.also { require(it.path.startsWith(root.path + File.separator)) { "sound path leaves app assets" } }.path
            }.toTypedArray()
            val decoded = BooleanArray(paths.size)
            handle = backend.open(paths, policy, decoded)
            check(handle != 0L) { "sound output did not open" }
            decodedCount = decoded.count { it }
            if (debug) Log.d("exact", "sound opened: now=${now()} handle=$handle decoded=$decodedCount/${files.size} focus=${claim != null}")
            files.forEachIndexed { i, name ->
                if (!decoded[i] && reportedFailures.add(name)) Log.w("exact", "sound: $name could not be decoded; its cues are dropped")
            }
            if (!listening) {
                val filter = IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY).apply { addAction(AudioManager.RINGER_MODE_CHANGED_ACTION) }
                if (Build.VERSION.SDK_INT >= 33) context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
                else context.registerReceiver(receiver, filter)
                listening = true
            }
            status = "running"
            error = null
            return true
        } catch (e: Exception) {
            fail(e.message ?: e.javaClass.simpleName)
        } catch (e: LinkageError) {
            fail(e.message ?: "sound library unavailable")
        }
        return false
    }

    private fun fail(reason: String) {
        close()
        failedAt = now()
        error = reason
        status = "failed"
        Log.w("exact", "sound: $reason; a play after one second retries")
    }

    private fun drain() {
        main.removeCallbacks(poll)
        var counts: DoubleArray? = null
        if (handle != 0L) {
            while (pending.isNotEmpty() && backend.op(handle, pending.first())) pending.removeFirst()
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
                    // Let the last rendered frames reach the speaker before releasing focus.
                    if (at - idle > counts[3] + 0.05) { close(); status = "stopped" }
                } else idleAt = null
            }
        }
        publish(counts)
        if (handle != 0L) main.postDelayed(poll, if (pending.isEmpty()) 50 else 5)
    }

    private fun publish(counts: DoubleArray? = null) {
        val report = report ?: return
        val state = JSONObject().put("kind", "aaudio").put("state", status).put("skipped", skipped)
        error?.let { state.put("error", it) }
        counts?.let {
            state.put("live", it[0].toLong()).put("dropped", it[1].toLong()).put("late", it[2].toLong())
                .put("outputLatency", it[3]).put("sampleRate", it[4].toInt()).put("timestamp", it[5] == 1.0)
                .put("underruns", it[8].toInt()).put("decoded", "$decodedCount/${files.size}")
        }
        report(state.toString())
    }

    private fun close(release: Boolean = true) {
        main.removeCallbacks(poll)
        pending.clear()
        if (handle != 0L) {
            if (debug) Log.d("exact", "sound closed: now=${now()} handle=$handle release=$release")
            backend.close(handle); handle = 0
        }
        if (release) { claim?.release(); claim = null }
        idleAt = null
    }

    private fun stop(release: Boolean = true) {
        discardedBefore = now()
        close(release)
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
        stop()
        if (listening) context.unregisterReceiver(receiver)
        listening = false
    }
}
