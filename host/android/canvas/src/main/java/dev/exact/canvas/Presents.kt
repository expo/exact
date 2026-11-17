package dev.exact.canvas

import android.view.View
import android.view.ViewTreeObserver

/** Tells exact2 when each picture it asked about reaches the screen (LLP 1003.001 D9): what the picture carried, a
 * commit's new animations, starts there, and the page's first one sets `exactPage().presented`. A picture's token
 * comes with its stream ([Native.presentToken]). A frame commit callback (API 29) runs once a frame is handed to the
 * compositor, and the frame shows at the first vsync at or after that. `ViewRootImpl.performDraw` takes the frame's
 * commit callbacks before it draws, so the callback is registered before the draw, at pre-draw, for each frame that
 * draws a picture ingested before it or may ingest one while drawing ([mayIngest]); a picture ingested while drawing
 * with no callback registered asks for one more frame and is reported with that one, a frame late, never early. A
 * window's first picture on Android 12 and later shows no earlier than the starting window's removal
 * ([StartingWindow]). [report] gets the token and the present (`System.nanoTime`). All on the main thread. */
internal class Presents(
    private val view: View,
    private val mayIngest: () -> Boolean,
    private val report: (Long, Long) -> Unit,
) {
    /** A frame being drawn, and the newest picture it draws (0: none). */
    private class Frame(var token: Long = 0L)

    /** The newest ingested picture no frame has taken. */
    private var latest = 0L
    private var current: Frame? = null

    fun ingested(picture: Long) { if (picture != 0L) latest = picture }

    private fun take(frame: Frame) { if (latest != 0L) { frame.token = latest; latest = 0L } }

    private val preDraw = ViewTreeObserver.OnPreDrawListener {
        if (latest != 0L || mayIngest()) {
            val frame = Frame()
            take(frame)
            current = frame
            view.viewTreeObserver.registerFrameCommitCallback { committed(frame) }
        }
        true
    }

    /** The view drew: the pictures it ingested are in this frame, or in the next where no callback waits. */
    fun drew() {
        val frame = current
        if (frame != null) take(frame) else if (latest != 0L) view.invalidate()
    }

    fun attach() { view.viewTreeObserver.addOnPreDrawListener(preDraw) }
    fun detach() { view.viewTreeObserver.removeOnPreDrawListener(preDraw); latest = 0L; current = null }

    private fun committed(frame: Frame) {
        if (current === frame) current = null
        val t = frame.token
        if (t == 0L) return
        if (StartingWindow.showing) { StartingWindow.after { report(t, onGrid(System.nanoTime())) }; return }
        report(t, onGrid(System.nanoTime()))
    }

    /** The first vsync at or after [at] on the grid through the latest frame's (`View.getDrawingTime`: Choreographer's
     * frame time, in uptime ms, which is `CLOCK_MONOTONIC` as `System.nanoTime` is). */
    private fun onGrid(at: Long): Long {
        val vsync = view.drawingTime * 1_000_000
        val rate = view.display?.refreshRate ?: 60f
        val period = (1e9 / (if (rate > 0f) rate else 60f)).toLong()
        return vsync + presentsAt(at - vsync, period)
    }
}

/** The offset from a vsync of the first vsync at or after [since] ns past it, on a grid of [period] ns. */
internal fun presentsAt(since: Long, period: Long): Long =
    if (since <= 0L || period <= 0L) 0L else (since + period - 1) / period * period

/** Android 12's starting window, which covers the app's window until the reader removes it at its first frame
 * ([ExactActivity]'s exit listener). A present heard while it shows is when it goes. A launch the system gives no
 * starting window never calls the listener, so a report waits at most [WAIT_MS] for it, short of exact2's own
 * deadline (2 s, which would start the motion at its commit). All on the main thread. */
internal object StartingWindow {
    var showing = false
        private set
    private val waiting = ArrayList<() -> Unit>()
    private val main by lazy { android.os.Handler(android.os.Looper.getMainLooper()) }
    private val giveUp = Runnable { removed() }
    private const val WAIT_MS = 1500L

    fun shown() { showing = true }

    fun removed() {
        showing = false
        main.removeCallbacks(giveUp)
        val ready = ArrayList(waiting)
        waiting.clear()
        ready.forEach { it() }
    }

    fun after(work: () -> Unit) {
        if (waiting.isEmpty()) main.postDelayed(giveUp, WAIT_MS)
        waiting.add(work)
    }
}
