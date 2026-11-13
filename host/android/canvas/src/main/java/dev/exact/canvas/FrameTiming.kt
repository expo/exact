package dev.exact.canvas

import android.content.Context
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.view.Choreographer
import android.view.Display
import android.view.FrameMetrics
import android.view.Window
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicLongArray

/**
 * Choreographer's turns for [Owner] (LLP 1079.000 §6.2). On Android 13 and later a vsync callback, whose frame data
 * gives the frame's deadline and the spacing of the vsyncs it is scheduled on; before it a frame callback, which gives
 * neither. Both run in Choreographer's animation phase, so a turn is scheduled the same either way.
 */
internal class Turns(private val turned: (vsync: Long, deadline: Long, period: Long) -> Unit) {
    private val frame = Choreographer.FrameCallback { turned(it, 0L, 0L) }
    private val vsync: Any? = if (Build.VERSION.SDK_INT >= 33) Vsyncs.callback(turned) else null

    fun post(c: Choreographer) { if (vsync != null) Vsyncs.post(c, vsync) else c.postFrameCallback(frame) }
    fun remove(c: Choreographer) { if (vsync != null) Vsyncs.remove(c, vsync) else c.removeFrameCallback(frame) }
}

@android.annotation.TargetApi(33)
private object Vsyncs {
    fun callback(turned: (Long, Long, Long) -> Unit): Any = Choreographer.VsyncCallback { data ->
        // The timelines are the vsyncs this frame could be presented at, one period apart.
        val lines = data.frameTimelines
        val period = if (lines.size >= 2) lines[1].expectedPresentationTimeNanos - lines[0].expectedPresentationTimeNanos else 0L
        turned(data.frameTimeNanos, data.preferredFrameTimeline.deadlineNanos, period)
    }
    fun post(c: Choreographer, callback: Any) = c.postVsyncCallback(callback as Choreographer.VsyncCallback)
    fun remove(c: Choreographer, callback: Any) = c.removeVsyncCallback(callback as Choreographer.VsyncCallback)
}

/**
 * The display's refresh period, told at once and whenever its mode changes ([tell], on [handler]'s thread): a GPU
 * canvas's `Frame::period_ms` (LLP 1009) and the period of a turn Choreographer gives none.
 */
internal class DisplayPeriod(context: Context, private val handler: Handler, private val tell: (Long) -> Unit) :
    DisplayManager.DisplayListener {
    private val displays = context.getSystemService(DisplayManager::class.java)
    private var told = 0L

    init {
        displays.registerDisplayListener(this, handler)
        handler.post { check() }
    }

    override fun onDisplayChanged(id: Int) { if (id == Display.DEFAULT_DISPLAY) check() }
    override fun onDisplayAdded(id: Int) {}
    override fun onDisplayRemoved(id: Int) {}

    private fun check() {
        val hz = displays.getDisplay(Display.DEFAULT_DISPLAY)?.refreshRate ?: return
        if (!(hz > 0f)) return
        val period = Math.round(1e9 / hz)
        if (period != told) { told = period; tell(period) }
    }
}

/**
 * The window's [FrameMetrics] for `perf frames` (LLP 1079.000 §6.2, §5.1): each drawn frame's intended vsync, vsync,
 * total duration, deadline and GPU duration (the last two from Android 12; 0 before), copied on the listener's thread
 * into a fixed ring, since the platform reuses its object, and drained on the exact thread ([drain]). Each slot has a
 * version, odd while it is written, so a frame the listener wrote over while it was copied is counted lost, never read
 * torn; every word is an atomic one, so the copy is ordered between the two reads of its version. The listener
 * runs only while a driver's launch shows the activity ([attach]); a production bake collects nothing.
 */
internal class WindowFrames(private val wake: () -> Unit) {
    private val thread by lazy { HandlerThread("exact frame metrics").apply { start() } }
    private val ring = AtomicLongArray(RING * WORDS)
    /** Each slot's version: odd while the listener writes frame k there, then 2 × (k + 1). */
    private val versions = AtomicLongArray(RING)
    /** Frames written (the listener's thread), published after each one's words. */
    private val written = AtomicLong(0)
    /** Reports the platform dropped before the listener ran. */
    private val dropped = AtomicLong(0)
    private val waking = AtomicBoolean(false)
    /** [read] as the listener's thread sees it. */
    private val readSeen = AtomicLong(0)
    /** Frames drained: the exact thread's. */
    private var read = 0L
    private var window: Window? = null

    private val listener = Window.OnFrameMetricsAvailableListener { _, m, drop ->
        record(
            m.getMetric(FrameMetrics.INTENDED_VSYNC_TIMESTAMP), m.getMetric(FrameMetrics.VSYNC_TIMESTAMP),
            m.getMetric(FrameMetrics.TOTAL_DURATION),
            if (Build.VERSION.SDK_INT >= 31) m.getMetric(FrameMetrics.DEADLINE) else 0L,
            if (Build.VERSION.SDK_INT >= 31) m.getMetric(FrameMetrics.GPU_DURATION) else 0L, drop,
        )
    }

    /** One frame's words, on the listener's thread, and the reports the platform dropped before it. */
    internal fun record(intended: Long, vsync: Long, total: Long, deadline: Long, gpu: Long, drop: Int) {
        val w = written.get()
        val slot = (w % RING).toInt()
        val at = slot * WORDS
        versions.set(slot, 2 * w + 1)
        ring.set(at, intended); ring.set(at + 1, vsync); ring.set(at + 2, total); ring.set(at + 3, deadline); ring.set(at + 4, gpu)
        versions.set(slot, 2 * (w + 1))
        if (drop > 0) dropped.addAndGet(drop.toLong())
        written.set(w + 1)
        // Half the ring waits: the exact thread drains it before it laps, even with no turn of its own.
        if (w + 1 - readSeen.get() >= RING / 2 && !waking.getAndSet(true)) wake()
    }

    fun attach(window: Window) {
        if (this.window === window) return
        detach()
        this.window = window
        window.addOnFrameMetricsAvailableListener(listener, Handler(thread.looper))
    }

    /** Stops measuring [from]'s frames, or any window's when it is null; a window since replaced keeps its successor's. */
    fun detach(from: Window? = null) {
        val w = window ?: return
        if (from != null && from !== w) return
        window = null
        try { w.removeOnFrameMetricsAvailableListener(listener) } catch (e: IllegalArgumentException) {}
    }

    /** The exact thread: the frames written since the last drain into [into] ([WORDS] each), their count, and in
     * [lost] the reports dropped by the platform or overwritten here before they were read. */
    fun drain(into: LongArray, lost: LongArray): Int {
        waking.set(false)
        val w = written.get()
        var from = read
        if (w - from > RING) { lost[0] += w - from - RING; from = w - RING }
        var n = 0
        for (k in from until w) {
            val slot = (k % RING).toInt()
            val version = versions.get(slot)
            for (word in 0 until WORDS) into[n * WORDS + word] = ring.get(slot * WORDS + word)
            // Written over before or while it was copied: lost, not read torn.
            if (version != 2 * (k + 1) || versions.get(slot) != version) lost[0]++ else n++
        }
        read = w
        readSeen.set(w)
        lost[0] += dropped.getAndSet(0)
        return n
    }

    companion object {
        const val RING = 64
        const val WORDS = 5
    }
}

/**
 * The frame rates exact2's GPU canvases ask for (`Surface::max_fps`), by view, as last told ([Native.frameRates], on the
 * main thread): kept for the process, as exact2 tells a rate only when it changes, so a canvas whose view or surface is
 * made after its rate was told (a reader made again, a canvas given a window) is asked at its rate then ([Natives]).
 */
internal class FrameRates {
    private val rates = HashMap<Int, Int>()

    /** View and rate pairs; a rate of 0 is the display's. */
    fun told(pairs: IntArray) {
        for (k in 0 until pairs.size / 2) {
            if (pairs[2 * k + 1] > 0) rates[pairs[2 * k]] = pairs[2 * k + 1] else rates.remove(pairs[2 * k])
        }
    }

    /** The rate [view] asks for; 0 for the display's. */
    fun of(view: Int): Int = rates[view] ?: 0
}
