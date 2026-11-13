package dev.exact.canvas

import android.content.Context
import android.media.AudioAttributes
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.os.VibrationAttributes
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.provider.Settings
import android.util.Log
import org.json.JSONObject
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * A GPU surface's haptics (LLP 1009 D10), as `exact_gpu::haptics` writes them and exact2 hands them over
 * ([Requests], `SURFACE_HAPTIC`), played on the device's vibrator. Core Haptics' model, on Android's:
 *
 * - A **texture** is a repeating waveform at an amplitude of `intensity`; a change of amplitude plays it again, no
 *   more often than every [MIN_GAP_MS]. One vibrator plays one effect, so of the surfaces' textures the strongest
 *   plays. Its `sharpness` has no amplitude to go to and is not played. A vibrator without amplitude control cannot
 *   follow an intensity, and plays no texture.
 * - A **pattern** is a composition of primitives when every event is a transient and the device has the click and
 *   tick primitives and says how long each lasts (Android 12): sharp transients click, dull ones tick, each scaled by
 *   its intensity, at its time ([schedule]). Otherwise it is a waveform on a [GRID_MS] grid, each slot at the
 *   strongest event over it, an intensity curve multiplying. It interrupts the texture, which plays again when the
 *   pattern's last primitive or slot ends.
 * - Everything stops while the activity is paused ([suspended]), and a surface that goes stops its texture (exact2
 *   tells it so). The person's touch feedback setting applies: from Android 13 the system applies it to
 *   [VibrationAttributes.USAGE_TOUCH]; before, a setting that is off plays nothing.
 */
internal class SurfaceHaptics(private val context: Context, suspended: Boolean) {
    private val vibrator: Vibrator? = (if (Build.VERSION.SDK_INT >= 31) context.getSystemService(VibratorManager::class.java)?.defaultVibrator
        else @Suppress("DEPRECATION") context.getSystemService(Vibrator::class.java))?.takeIf { it.hasVibrator() }
    private val handler = Handler(Looper.getMainLooper())
    private val log = android.system.Os.getenv("EXACT_LOG_REQUESTS") == "1"
    /** Each surface's texture amplitude, 1–255. */
    private val textures = HashMap<Int, Int>()
    /** The amplitude playing now (0: none), when it began, and when a pattern ends (the texture waits for it). */
    private var playing = 0
    private var playedAt = 0L
    private var patternEnds = 0L
    private val replay = Runnable { texture() }

    /** While the activity is paused: nothing plays, and a texture message starts nothing. Made with the
     * activity's state, which may be paused already. */
    var suspended = suspended
        set(on) {
            field = on
            if (on) { textures.clear(); handler.removeCallbacks(replay); stop() }
        }

    /** A surface's message. */
    fun told(surface: Int, json: String) {
        val v = vibrator ?: return
        if (suspended || !enabled()) return
        val message = try { JSONObject(json) } catch (e: org.json.JSONException) { Log.w(TAG, "haptics: $e"); return }
        if (message.has("texture")) {
            val t = message.optJSONObject("texture")
            val amplitude = t?.let { (it.optDouble("intensity", 0.0) * 255).roundToInt().coerceIn(0, 255) } ?: 0
            if (amplitude == 0 || !v.hasAmplitudeControl()) textures.remove(surface) else textures[surface] = amplitude
            texture()
        } else message.optJSONObject("pattern")?.let { pattern(v, it) }
    }

    /** The strongest texture, played when it changed, once the last play is [MIN_GAP_MS] old and no pattern plays. */
    private fun texture() {
        val v = vibrator ?: return
        val want = textures.values.maxOrNull() ?: 0
        if (want == playing) return
        val now = SystemClock.uptimeMillis()
        val wait = max(patternEnds - now, playedAt + MIN_GAP_MS - now)
        handler.removeCallbacks(replay)
        if (wait > 0) { handler.postDelayed(replay, wait); return }
        playing = want
        playedAt = now
        if (want == 0) v.cancel() else vibrate(v, VibrationEffect.createWaveform(longArrayOf(TEXTURE_MS), intArrayOf(want), 0))
        if (log) Log.i(TAG, "haptics: texture at $want")
    }

    private fun pattern(v: Vibrator, p: JSONObject) {
        val events = p.optJSONArray("events") ?: return
        if (events.length() == 0) return
        val scale = curve(p.optJSONArray("curves"))
        val list = (0 until events.length()).map { events.getJSONObject(it) }
        val transients = list.all { it.optString("type") == "transient" }
        val click = VibrationEffect.Composition.PRIMITIVE_CLICK
        val tick = VibrationEffect.Composition.PRIMITIVE_TICK
        val effect = if (transients && Build.VERSION.SDK_INT >= 31 && v.areAllPrimitivesSupported(click, tick)) {
            val sorted = list.sortedBy { it.optDouble("time") }
            val ids = IntArray(sorted.size) { if (sorted[it].optDouble("sharpness") >= 0.5) click else tick }
            val (clickMs, tickMs) = v.getPrimitiveDurations(click, tick).let { it[0] to it[1] }
            val relative = Build.VERSION.SDK_INT >= 36
            val (delays, end) = schedule(IntArray(sorted.size) { (sorted[it].optDouble("time") * 1000).roundToInt() },
                IntArray(sorted.size) { if (ids[it] == click) clickMs else tickMs }, relative)
            val c = VibrationEffect.startComposition()
            for ((i, e) in sorted.withIndex()) {
                val scaled = (e.optDouble("intensity") * scale(e.optDouble("time"))).toFloat().coerceIn(0f, 1f)
                if (relative) c.addPrimitive(ids[i], scaled, delays[i], VibrationEffect.Composition.DELAY_TYPE_RELATIVE_START_OFFSET)
                else c.addPrimitive(ids[i], scaled, delays[i])
            }
            patternEnds = SystemClock.uptimeMillis() + end
            c.compose()
        } else {
            // Each GRID_MS slot at the strongest event over it.
            val end = list.maxOf { it.optDouble("time") + max(it.optDouble("duration", 0.0), TRANSIENT_MS / 1000.0) }
            val slots = IntArray(((end * 1000) / GRID_MS).toInt().coerceIn(1, 30_000 / GRID_MS.toInt()))
            for (e in list) {
                val from = e.optDouble("time")
                val to = from + if (e.optString("type") == "transient") TRANSIENT_MS / 1000.0 else e.optDouble("duration", 0.0)
                for (i in (from * 1000 / GRID_MS).toInt() until (to * 1000 / GRID_MS).toInt().coerceAtMost(slots.size)) {
                    val amp = (e.optDouble("intensity") * scale(i * GRID_MS / 1000.0) * 255).roundToInt().coerceIn(0, 255)
                    slots[i] = max(slots[i], amp)
                }
            }
            patternEnds = SystemClock.uptimeMillis() + slots.size * GRID_MS
            if (v.hasAmplitudeControl()) VibrationEffect.createWaveform(LongArray(slots.size) { GRID_MS }, slots, -1)
            else {
                // On and off: a waveform's timings alternate, from off.
                val runs = arrayListOf(0L)
                var on = false
                for (a in slots) {
                    if ((a > 0) != on) { runs.add(0L); on = !on }
                    runs[runs.lastIndex] = runs.last() + GRID_MS
                }
                VibrationEffect.createWaveform(runs.toLongArray(), -1)
            }
        }
        playing = -1
        vibrate(v, effect)
        handler.removeCallbacks(replay)
        handler.postDelayed(replay, max(0L, patternEnds - SystemClock.uptimeMillis()))
        if (log) Log.i(TAG, "haptics: a pattern of ${list.size} events")
    }

    /** The intensity curve's value at `t` seconds, linearly between its points: 1 without one. */
    private fun curve(curves: org.json.JSONArray?): (Double) -> Double {
        val c = (0 until (curves?.length() ?: 0)).map { curves!!.getJSONObject(it) }.firstOrNull { it.optString("parameter") == "intensity" }
            ?: return { 1.0 }
        val points = c.getJSONArray("points").let { a -> (0 until a.length()).map { a.getJSONArray(it).let { p -> p.getDouble(0) to p.getDouble(1) } } }
        if (points.isEmpty()) return { 1.0 }
        return { t ->
            val after = points.indexOfFirst { it.first > t }
            when {
                after == 0 -> points[0].second
                after < 0 -> points.last().second
                else -> points[after - 1].let { (t0, v0) -> points[after].let { (t1, v1) -> v0 + (v1 - v0) * (t - t0) / (t1 - t0) } }
            }
        }
    }

    private fun vibrate(v: Vibrator, effect: VibrationEffect) {
        if (Build.VERSION.SDK_INT >= 33) v.vibrate(effect, VibrationAttributes.createForUsage(VibrationAttributes.USAGE_TOUCH))
        else @Suppress("DEPRECATION") v.vibrate(effect, AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION).build())
    }

    /** Before Android 13 the system does not apply the touch feedback setting to a vibration: it is read here. */
    private fun enabled() = Build.VERSION.SDK_INT >= 33 ||
        Settings.System.getInt(context.contentResolver, Settings.System.HAPTIC_FEEDBACK_ENABLED, 1) != 0

    private fun stop() {
        if (playing != 0 || patternEnds > SystemClock.uptimeMillis()) vibrator?.cancel()
        playing = 0
        patternEnds = 0
    }

    /** The activity goes. */
    fun destroy() { suspended = true }

    internal companion object {
        /**
         * Each primitive's delay for a composition, and when the last ends, in ms: primitive `i` is due at `times[i]`
         * and lasts `durations[i]`. With `relative` (Android 16's `DELAY_TYPE_RELATIVE_START_OFFSET`) a delay counts
         * from the previous primitive's start, so each plays at its time. Before, a delay counts from the previous
         * primitive's end (`addPrimitive`'s pause), so durations are taken off it, and one due before the previous
         * ends starts when it ends.
         */
        fun schedule(times: IntArray, durations: IntArray, relative: Boolean): Pair<IntArray, Int> {
            val delays = IntArray(times.size)
            var end = 0
            var free = 0
            for (i in times.indices) {
                val start = if (relative) times[i] else max(times[i], free)
                delays[i] = if (relative) times[i] - (if (i == 0) 0 else times[i - 1]) else start - free
                free = start + durations[i]
                end = max(end, free)
            }
            return delays to end
        }

        const val TAG = "exact"
        /** A texture's waveform segment, repeated until it changes or stops. */
        const val TEXTURE_MS = 1000L
        /** The least time between two plays of a texture: a vibrator restarts its effect at each. */
        const val MIN_GAP_MS = 16L
        const val GRID_MS = 10L
        const val TRANSIENT_MS = 20L
    }
}
