package dev.exact.canvas

import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.view.MotionEvent
import android.view.Choreographer
import java.nio.ByteOrder
import java.nio.IntBuffer
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.FutureTask
import java.util.concurrent.atomic.AtomicBoolean
import org.json.JSONException
import org.json.JSONObject

/**
 * exact2 on a thread of its own (LLP 1072, on Android). Every call into the Rust host runs on the `exact` thread, which
 * owns it from boot on: its looper takes the executor's replies and decoded pictures, its Choreographer the turns an
 * animation wants. Each new op stream is copied and published; the main thread only ingests and replays them. A scroll
 * translates the feed's group on the main thread at once (as a compositor scrolls) and reaches exact2 after, so a long
 * collection pass delays rows by a frame or two but never holds a frame (owner=0: exact2 on the main thread).
 */
internal class Owner(private val hwbuf: Boolean, private val fast: Boolean, priority: Int, intent: (String) -> String?) {
    /** Called on the main thread when a stream was published: the view that reads this owner now. The owner outlives
     * an activity (its thread holds the host, which is the process's), so each activity's view takes it in turn. */
    @Volatile var published: () -> Unit = {}

    /** boost=0: input does not raise the clocks, to compare. Otherwise the exact and main threads are an ADPF hint
     * session (made at the first input), told of a workload increase as each press, release or key arrives: an idle
     * phone meets a tap at its lowest clocks (a Pixel 10 Pro XL's exact thread at 400-650 MHz: the release's commit and
     * paint took 5-10 ms and its response missed the frame after the tap; with the hint, 1.8-2 GHz and 2-3 ms). */
    private val boost = intent("boost") != "0"
    private var session: Boolean? = null

    /** A press, release or key arrived: the threads that answer it clocked up now. (A host library without these
     * calls, an older exact2, goes without.) */
    fun input() {
        if (!boost || session == false) return
        try {
            val made = session ?: Native.hintSession(intArrayOf(thread.threadId, android.os.Process.myPid()), 4_000_000L).also { session = it }
            if (made) Native.inputHint()
        } catch (e: UnsatisfiedLinkError) { session = false }
    }

    /** mainfast=0: the main thread wakes for a release where the scheduler puts it, to compare. Otherwise it is kept to
     * the fast cores while a press is held: on a phone with little cores (a Pixel 10a) it woke for the release on one,
     * so the exact thread was not colocated, started 1.3-2.9 ms late on an idle core and ran at 357 MHz until the hint
     * reached the governor; taps answered a frame late 14 of 66 times, 7 of 66 with this. (The hint again at each warm
     * poke did not help; a clock floor of the threads' own, uclamp.min, is refused to an app.) */
    private val mainFast = intent("mainfast") != "0"
    private var isHeld = false

    /** A press began ([on]) or ended (released, cancelled, or held past the warm hold). */
    fun held(on: Boolean) {
        if (on == isHeld) return
        isHeld = on
        if (mainFast) try { Native.fastThread(android.os.Process.myPid(), on) } catch (e: UnsatisfiedLinkError) {}
    }

    /** place=0: the exact thread wakes where the scheduler puts it, to compare. Otherwise, while the main thread waits for
     * an answer, the exact thread runs on the main thread's (awake) core: woken on an idle core it started ~1 ms after
     * the post, on the main thread's ~0.4 ms. It widens its mask again ([Native.fastCores]) as it starts. */
    private var colocate = intent("place") != "0"

    private fun colocated(waitMs: Long): Boolean {
        if (!colocate || !fast || waitMs <= 0) return false
        return try { Native.colocate(thread.threadId) } catch (e: UnsatisfiedLinkError) { colocate = false; false }
    }

    /** A published stream: [ops] (a copy, the reader's), whether it only moves the last paint, the last scroll it
     * includes, and the feed's group (0 before the first scroll). [scrollers]: what exact2 tells the reader of its
     * scrollers as of this stream ([Native.scrollers]). A statement can acknowledge a rejected move without [ops].
     * [fields]: what exact2 tells of the text fields the reader hosts as of this stream
     * ([Native.fields], LLP 1115.001 D8), null when that is what it last told. [bars]: what it tells of its bars as
     * of this stream ([Native.bars], D11), null likewise. [access]: what it tells of the accessibility tree
     * ([Native.access], D13), null likewise, and always while no accessibility service is on. */
    class Pub(
        val publication: Long,
        val ops: IntBuffer?, val move: Boolean, val seq: Int, val feed: Int, val scrollers: IntArray?, val fields: IntArray?,
        val bars: IntArray?, val access: IntArray?,
        /** The picture's token for its present ([Presents]), 0 for none. */
        val picture: Long = 0L,
    )

    @Volatile var module: ModuleSession? = null

    /** At the main thread's own priority, not above it ([priority]): its work is ahead of the frame the main thread
     * draws, never instead of it (THREAD_PRIORITY_DISPLAY let it preempt the main thread on a phone with few big cores:
     * heavy 12k/24k dropped to 113/108 fps). */
    @Volatile private var ended = false
    private val thread = object : HandlerThread("exact", priority) {
        override fun run() {
            try { super.run() } finally {
                ended = true
                if (booted) Native.endSession()
            }
        }
    }.apply { start() }
    private val handler = Handler(thread.looper)
    private val main = Handler(Looper.getMainLooper())
    private val mainNow = Handler.createAsync(Looper.getMainLooper())
    // The main thread chooses the boot path before attaching the reader.
    private var firstBoot = true
    private var readerRunning = true
    @Volatile private var coldChrome: ((IntArray) -> Unit)? = null
    private var windowSent = 0
    private var windowApplied = 0
    /** Every paint in order (each may define and free pictures and rows); moves are coalesced by the reader. */
    val queue = ConcurrentLinkedQueue<Pub>()
    private val notifying = AtomicBoolean(false)
    /** Pictures the exact thread made for streams it published (copying exact2's pixels into a GPU buffer), so the
     * main thread's ingest only takes them. */
    val made = java.util.concurrent.ConcurrentHashMap<Int, android.graphics.Bitmap>()
    /** Called on the main thread with what exact2 asks of the platform ([Native.requests], LLP 1115.001 D5): the
     * view's activity that reads this owner now acts on them ([Requests]). */
    @Volatile var asked: ((IntArray, ModuleSession?) -> Unit)? = null

    // The exact thread's own state.
    private var windowRead = 0
    private var windowPosted = 0
    private val timerReports = TimerReports()
    private val reader get() = timerReports.reader
    private val readerKnown get() = timerReports.known
    private val readerScrolling get() = timerReports.scrolling
    private val readerSampling get() = timerReports.sampling
    private var idlePosted = false

    private fun postMain(work: () -> Unit) = postMain(main, work)
    private fun postMain(to: Handler, work: () -> Unit) {
        timerReports.mainWork++
        to.post {
            try { work() } finally {
                handler.post {
                    timerReports.mainWork--
                    if (running && Native.timerObservation(readerScrolling, readerSampling, timerReports.applying, false)) produce() else schedule()
                }
            }
        }
    }
    fun timerReader(view: Any) {
        handler.post {
            timerReports.replace(view)
            if (running) Native.timerObservation(false, false, true, false)
        }
    }
    /** The reader's report: what it is doing, the last publication it ingested ([applied]) and, when it ingested any
     * since its last report, when it began and ended ([began], [ended]; 0 otherwise), which finishes the turns that
     * published them in `perf frames` (LLP 1079.000 §6.2). */
    fun timerReport(view: Any, scrolling: Boolean, sampling: Boolean, applied: Long, began: Long, ended: Long) {
        handler.post {
            if (ended != 0L && timing) try { Native.frameLanded(applied, began, ended) } catch (e: UnsatisfiedLinkError) { timing = false }
            if (!timerReports.report(view, scrolling, sampling, applied)) return@post
            if (running && Native.timerObservation(scrolling, sampling, timerReports.applying, false)) produce() else schedule()
        }
    }

    /** The reader drew picture [token] and it reached the screen at [at] (`System.nanoTime`, [Presents]). */
    fun presented(token: Long, at: Long) {
        handler.post { if (Native.picturePresented(token, at)) produce() else schedule() }
    }

    private var seq = 0
    private var steps = 0
    private var turnPosted = false
    /** The last turn took longer than a display frame: the next one is run at once, not at the next one. */
    private var late = false
    private var choreographer: Choreographer? = null
    /** Each turn is told to exact2 for `perf frames` (a host library without the calls, an older exact2, is not). */
    private var timing = true
    /** The window's frame metrics, drained on this thread, while a driver's launch shows an activity
     * ([ExactActivity]). */
    val windowFrames = WindowFrames { handler.post { drainWindow() } }
    private val windowWords = LongArray(WindowFrames.RING * WindowFrames.WORDS)
    private val windowLost = LongArray(1)
    private var displayPeriod: DisplayPeriod? = null

    /** The display's refresh period, told to exact2 now and whenever its mode changes: once for the process. */
    fun displays(context: android.content.Context) {
        if (displayPeriod != null) return
        displayPeriod = DisplayPeriod(context.applicationContext, handler) { period ->
            if (timing) try { Native.framePeriod(period) } catch (e: UnsatisfiedLinkError) { timing = false }
        }
    }

    /** No turn is asked for: the run of turns ends. */
    private fun idle() { if (timing) try { Native.frameIdle() } catch (e: UnsatisfiedLinkError) { timing = false } }

    /** The window's frames drained now: before a driver's request reads or changes what they are counted in. */
    fun drainWindow() {
        if (!timing) return
        windowLost[0] = 0
        val n = windowFrames.drain(windowWords, windowLost)
        if (n > 0 || windowLost[0] > 0) try { Native.frameWindow(windowWords, n, windowLost[0]) } catch (e: UnsatisfiedLinkError) { timing = false }
    }

    /** Called on the main thread with the frame rates exact2's GPU canvases ask for, as they change ([Natives]). */
    @Volatile var rates: (IntArray) -> Unit = {}
    /** Those rates as last told: the main thread's. */
    val frameRates = FrameRates()
    private val prefetch = true

    // Driver travel and gesture samples the exact thread has not taken.
    private val lock = Any()
    private var owedDy = 0f
    private var owedSeq = 0
    private var stepPosted = false
    private class Moved(val id: Int, val dx: Float, val dy: Float, val rev: Int)
    private var owedMoves = ArrayList<Moved>()
    /** lead=1: the velocity the window leads by (px/s, positive toward the end; otherwise none, a viewport each side).
     * Fast travel ([LEAD_ON] dp/s and on, until it slows below [LEAD_OFF]) leads by all exact2 allows (two viewports
     * more): a lead that followed the measured speed moved the window's far edge with each frame's jitter, mounting and
     * retiring a row every report (crypto at 1k: 3x the collection work). Off by default: on a Pixel 10 Pro XL the
     * longer window made every report dearer (xheavy 24k: 18-19% of the view blank against 12-14% without, CPU +4%;
     * crypto +12%), and rows near the view are what blanks (ExactView's near rows). */
    @Volatile private var velocity = 0f
    private var measured = 0f
    private var lastScrollAt = 0L
    private val lead = intent("lead") == "1"
    private val density = android.content.res.Resources.getSystem().displayMetrics.density

    init {
        // At once, while the activity goes on setting up: the thread's Choreographer, its cores, and exact2's library
        // (loaded by Native's first call), so the boot posted after starts with them done.
        handler.post {
            choreographer = Choreographer.getInstance()
            // Off the little cores (fast=0: wherever the scheduler puts it): the thread sleeps between bursts (a tap's
            // commit, a frame), so the scheduler wakes it where an idle thread goes, and a tap's work there took two
            // to three times as long. Neither a higher priority nor an ADPF session kept it off them.
            if (fast) Native.fastCores()
        }
    }

    /** Whether the host's wake descriptors are watched: once, however many readers boot it. */
    private var watching = false
    /** The host's wake descriptors, watched again after each turn ([schedule]): a dev loop's plan brings new ones. */
    private var wakes: Wakes? = null

    /** Whether an accessibility service is on, and whether the reader holds nothing of the tree yet (LLP 1115.001
     * D13): the exact thread's own. exact2 is asked for the tree with each stream only while one is on. */
    private var accessible = false
    private var accessAgain = false

    /** An accessibility service came on ([on]), or the last one went: the reader is told the tree whole, or exact2
     * forgets it. */
    fun access(on: Boolean) {
        handler.post {
            if (on) { accessible = true; accessAgain = true; if (watching) produce() }
            else if (accessible) { accessible = false; try { Native.accessOff() } catch (e: UnsatisfiedLinkError) {} }
        }
    }

    /** Boot at the size the view is expected to have; [done] on the main thread with whether it booted. An activity
     * made again in this process (LLP 1115.001 D15.1) calls this for its own view: [Native.start] then adopts the host
     * that runs on this thread, which paints everything again for the new reader, and starts no second one.
     * [started] runs on this thread once the host has booted, before its first turn and before its data module can
     * activate ([active]). Returns whether the new reader should hold its first draw. */
    fun boot(width: Int, height: Int, scale: Float, dir: String, store: String, live: Boolean, started: () -> Unit = {},
             done: (Boolean) -> Unit): Boolean {
        val cold = firstBoot && readerRunning
        firstBoot = false
        // Only a cold foreground launch saves a frame by passing a traversal barrier. On adoption, chrome,
        // paint notifications and platform requests must stay in queue order with completion.
        coldChrome = if (cold) chrome else null
        handler.post {
            android.os.Trace.beginSection("exact boot")
            try { Native.hardwarePictures(hwbuf) } catch (e: UnsatisfiedLinkError) { }
            val ok = Native.start(width, height, scale, dir, store, live)
            android.os.Trace.endSection()
            if (ok) {
                booted = true
                started()
                // What was published or scrolled for the reader before this one is not this one's.
                queue.clear(); made.clear(); seq = 0
                accessAgain = true
                synchronized(lock) { owedDy = 0f; owedSeq = 0 }
                if (!watching) {
                    watching = true
                    // A reply, a picture, or a request exact2 queued with no frame to follow it; or an acquired image.
                    wakes = Wakes(thread.looper.queue) { index ->
                        if (index == Wakes.ACQUIRED) Native.acquired(running) else if (Native.poll()) produce() else { requests(); schedule() }
                    }
                }
                wakes?.sync()
                Warm.stop = true
                produce()
            } else coldChrome = null
            postMain(if (cold) mainNow else main) { done(ok) }
        }
        return cold
    }

    /** The cold reader drew, even if it received no chrome, or left without drawing. */
    fun endColdDraw() { coldChrome = null }

    val awaitingColdDraw get() = coldChrome != null
    val waitingForColdWindow get() = awaitingColdDraw && windowApplied != windowSent

    /** Whether a [boot] has given this thread a host. */
    @Volatile var booted = false
        private set

    /** Who waits to hear that the host's data module is active ([active]): the exact thread's own. */
    private val activating = ArrayList<(Boolean) -> Unit>()

    /**
     * [done] hears, on the main thread, whether the host's data module is active, once it is or has failed to be;
     * false at once where no boot gave this thread a host. A host no reader takes a frame from (no activity, or a
     * stopped one) has no first pixel to wait for, so its module activates now (LLP 1133.017 D7); one a reader draws
     * activates it after its first pixel, as it does with nobody waiting. A module whose image is still loading is
     * asked again at each turn its wakes bring ([schedule]).
     */
    fun active(done: (Boolean) -> Unit) {
        handler.post {
            if (!booted) { postMain { done(false) }; return@post }
            activating += done
            schedule()
        }
    }

    private fun activated() {
        if (activating.isEmpty()) return
        val state = Native.dataActive(!running)
        if (state == 0) return
        val waiting = activating.toList()
        activating.clear()
        postMain { waiting.forEach { it(state > 0) } }
    }

    /** The device's settings ([Device]): told before [boot] so the host boots with them (the library loads here, not on
     * the main thread), and whenever they change, when the host takes them and publishes what they changed. */
    fun configure(device: Device) {
        handler.post { device.tell(); if (watching) produce() }
    }

    /** The device's colours ([Palette]): told before [boot] so the host's first frame is in them, and whenever they
     * change, when the host paints the page again. */
    fun colors(roles: IntArray) {
        handler.post { Native.colors(roles); if (watching) produce() }
    }

    /** What exact2 asks of the platform since it was last asked ([Native.requests]): handed to the main thread, which
     * acts on it. After every turn, and when exact2 wakes this thread for a request no frame follows (a haptic
     * changes no pixel); also while the activity is stopped, when no stream is made. */
    private fun requests(to: Handler = if (awaitingColdDraw) mainNow else main) {
        Native.requests()?.let { postRequests(it, to) }
    }

    private fun postRequests(words: IntArray, to: Handler) {
        val module = this.module
        postMain(to) { if (!ended) asked?.invoke(words, module) ?: Requests.detached(words, module) }
    }

    /** The window ([Native.window]): told before [boot] so the host is laid out in it from its first frame, and
     * whenever its size, the display's density or an inset changes, when the host lays the page out again. */
    fun window(width: Int, height: Int, scale: Float, insets: IntArray) {
        val number = ++windowSent
        handler.post {
            Native.window(width, height, scale, insets)
            windowRead = number
            if (watching) produce()
        }
    }

    /** Called on the main thread with what the host says to draw around the page ([Native.chrome]) when that
     * changed: asked after each stream, so it is drawn with the frame it belongs to. */
    @Volatile var chrome: (IntArray) -> Unit = {}

    /** Whether the activity is started. Stopped, it runs tasks and drains replies without recording frames
     * (LLP 1115.001 D15.2, LLP 1147 D2). Android still decides whether the process gets to execute. */
    private var running = true

    /** The activity stopped ([on] false) or started again. */
    fun running(on: Boolean) {
        readerRunning = on
        if (!on) endColdDraw()
        handler.post {
            if (on == running) return@post
            running = on
            if (on) { produce(); return@post }
            // Start adopts a fresh reader and reannounces its drawing from the running host.
            queue.clear()
            made.clear()
            timerReports.retire()
            choreographer?.let { turns.remove(it) }; turnPosted = false
            idle()
            handler.removeCallbacks(again)
            handler.removeCallbacks(timer); handler.removeCallbacks(refiner); handler.removeCallbacks(settle)
            schedule()
        }
    }

    /** A scroll of [dy] px, the main thread's [seq]th. */
    fun scroll(dy: Float, seq: Int) {
        val now = System.nanoTime()
        val dt = (now - lastScrollAt) / 1e9f
        lastScrollAt = now
        // Steps a frame apart: their mean over the last few frames (a pause starts it again).
        measured = if (dt > 0.1f || dt <= 0f) 0f else measured * 0.6f + dy / dt * 0.4f
        if (lead) {
            val dp = Math.abs(measured) / density
            val on = if (velocity != 0f) dp >= LEAD_OFF && Math.signum(measured) == Math.signum(velocity) else dp >= LEAD_ON
            velocity = if (on) Math.signum(measured) * LEAD_PX else 0f
        }
        synchronized(lock) {
            owedDy += dy; owedSeq = seq
            if (stepPosted) return
            stepPosted = true
        }
        handler.post(step)
    }

    /** The reader moved scroller [id] by ([dx], [dy]) px, knowing revision [rev] of it ([Scrolling]); the main
     * thread's [seq]th scroll. exact2 applies each sample in order, as the reader did. */
    fun scrolled(id: Int, dx: Float, dy: Float, rev: Int, seq: Int) {
        synchronized(lock) {
            owedMoves.add(Moved(id, dx, dy, rev))
            owedSeq = seq
            if (stepPosted) return
            stepPosted = true
        }
        handler.post(step)
    }

    /** What is under a point of the view ([Native.under]), asked on the exact thread behind the touch that landed
     * there. [done] on the main thread with the answer, after the statement of the scrollers it names is published,
     * so the reader knows where each is before it decides anything. */
    fun under(x: Float, y: Float, done: (IntArray) -> Unit) {
        handler.post { val u = Native.under(x, y); produce(); postMain { done(u) } }
    }

    /** [Native.snapsAround], on the exact thread; [done] on the main thread with the answer. */
    fun snapsAround(id: Int, vertical: Boolean, release: Float, projected: Float, done: (IntArray) -> Unit) {
        handler.post { val w = Native.snapsAround(id, vertical, release, projected); postMain { done(w) } }
    }

    private val step = Runnable {
        val dy: Float; val s: Int
        val moves: List<Moved>
        synchronized(lock) {
            dy = owedDy; s = owedSeq; owedDy = 0f; stepPosted = false
            moves = owedMoves
            owedMoves = ArrayList()
        }
        android.os.Trace.beginSection("exact scroll")
        for (m in moves) Native.scrolled(m.id, m.dx, m.dy, m.rev)
        if (moves.isEmpty() || dy != 0f) Native.scroll(dy)
        android.os.Trace.endSection()
        seq = s
        // A step of the reader's own moves whose frame would only move what it moved itself makes none, unless an
        // accessibility service is on (each frame tells it the scrollers' offsets). What follows a frame still runs.
        if (running && !accessible && moves.isNotEmpty() && dy == 0f && Native.moved()) produced() else produce()
        // Every third step: the window's lead mounts, off the main thread.
        if (prefetch && ++steps % 3 == 0) { handler.removeCallbacks(refiner); handler.post(refiner) }
        if (led) { handler.removeCallbacks(settle); handler.postDelayed(settle, SETTLE_MS) }
    }

    /** The last refine led (exact2 keeps that velocity for its passes until a refine sets another). */
    private var led = false

    private val refiner: Runnable = Runnable {
        android.os.Trace.beginSection("exact prefetch")
        val v = velocity
        led = v != 0f
        val r = Native.refine(-1, v)
        if (r and 1 != 0) produce()
        // A pass exact2 cut short (while the view moves fast it builds what fits a frame's share): the rest after
        // the steps queued meanwhile, each pass painting what it built.
        if (r and 2 != 0) { handler.removeCallbacks(refiner); handler.post(refiner) }
        android.os.Trace.endSection()
    }

    /** Scrolling stopped while the window led: it goes back to a viewport each side. */
    private val settle = Runnable {
        if (led) { led = false; if (Native.refine(-1, 0f) and 1 != 0) produce() }
    }

    /** A touch: answered within [waitMs] if exact2 is free (as LLP 1072's events are synchronous, so the response is
     * in this frame), else published when it is. Whether it was answered. */
    fun touch(action: Int, x: Float, y: Float, tool: Int, waitMs: Long, at: Long, history: MotionEvent?, released: ((IntArray) -> Unit)? = null): Boolean {
        // Android recycles the delivered event as soon as the reader returns.
        val batch = history?.let { MotionEvent.obtain(it) }
        return answer(waitMs) {
            try { batch?.forwardHistory(tool) } finally { batch?.recycle() }
            val list = Native.touch(action, x, y, tool, at)
            if (list != null && released != null) main.post { released(list) }
            if (action == 0 && ahead) { handler.removeCallbacks(aheadSlice); handler.post(aheadSlice) }
        }
    }

    /** A finger past the touch's first ([Native.finger]) on the exact thread, waiting as a touch does. */
    fun finger(action: Int, id: Int, x: Float, y: Float, tool: Int, at: Long, primaryX: Float, primaryY: Float, waitMs: Long): Boolean = answer(waitMs) { Native.finger(action, id, x, y, tool, at, primaryX, primaryY) }

    /** [Native.touching] on the exact thread, in order with the touch's own events and waiting for nothing: a
     * replacement the finger held applies in the frame this then produces. */
    fun touching(on: Boolean) { handler.post { if (Native.touching(on)) produce() } }

    /** ahead=0: off. What the press of the touch that is down would open is laid out ahead of its release (exact2's
     * `ahead`: its text shaped, in a scratch tree; nothing shown or committed), a slice of at most [AHEAD_US] of
     * shaping at a time. Each slice is posted behind whatever this thread has queued, so the touch's own frame and
     * anything that came meanwhile go first; exact2 forgets the rest when the touch ends or anything commits. The
     * first mail the daily benchmark opens was ready 9 ms after its release, 3 ms of that its layout; 6 ms with this. */
    private val ahead = intent("ahead") != "0"
    private val aheadSlice: Runnable = object : Runnable {
        override fun run() {
            val more = try { Native.ahead(AHEAD_US) } catch (e: UnsatisfiedLinkError) { false }
            if (more) handler.post(this)
        }
    }

    /** Run [f] on the exact thread and publish; wait up to [waitMs] for it. Whether it finished in time. */
    fun answer(waitMs: Long, f: () -> Unit): Boolean {
        val pinned = colocated(waitMs)
        // The waiter stays on this core for the wait (waitpin=0: wherever the scheduler wakes it, to compare): the
        // exact thread is answering here, and the main thread woken for the answer on another, idle core began its
        // ingest ~1.1 ms after the answer was ready (every tap; an answer ready at 4.6 ms was drawn from 7.2 ms).
        val main = android.os.Process.myPid()
        val stays = pinned && waitPin && try { Native.colocate(main) } catch (e: UnsatisfiedLinkError) { false }
        return try { waitForOwner(waitMs) { if (pinned) Native.fastCores(); f(); produce() }; true } catch (e: java.util.concurrent.TimeoutException) { false } finally {
            if (stays) try { Native.fastThread(main, isHeld && mainFast) } catch (e: UnsatisfiedLinkError) {}
        }
    }
    private val waitPin = intent("waitpin") != "0"

    /** Run [f] on the exact thread, publishing nothing: before the host boots too. */
    fun tell(f: () -> Unit) {
        handler.post(f)
    }

    /** A canvas has samples for its next display callback. */
    fun sensorFrame() = tell {
        if (running && !turnPosted) choreographer?.let { turnPosted = true; turns.post(it) }
    }

    /** Run [f] on the exact thread, then publish what it changed. */
    fun call(f: () -> Unit) {
        handler.post { f(); produce() }
    }

    private val replies = OwnerReplies(::call, OwnerReplies::deliver)

    fun hand(handed: OwnerReplies.Handed) = replies.hand(handed)

    /** Called from Native.agent on this thread. Return data, never re-enter the borrowed Rust host: in the order they
     * arrived, each answer as `[id, answer]`, each topic as itself, each picture as `{"picture", "revoked"}` and each
     * journal line as `{"journal"}` (`presenter::Handed`). */
    fun agentTurn(requests: IntArray?): String {
        check(Looper.myLooper() == thread.looper)
        if (requests != null) postRequests(requests, main)
        return org.json.JSONArray().apply {
            for (h in replies.take()) put(when (h) {
                is OwnerReplies.Handed.After -> JSONObject().put("timer", h.timer).put("due", h.due)
                is OwnerReplies.Handed.Answer -> org.json.JSONArray().put(h.id).put(h.value ?: org.json.JSONObject.NULL)
                is OwnerReplies.Handed.Refused -> JSONObject().put("id", h.id).put("error", h.error).put("message", h.message)
                is OwnerReplies.Handed.Changed -> h.topic
                is OwnerReplies.Handed.Picture -> org.json.JSONObject().put("picture", h.source).put("revoked", h.revoked)
                is OwnerReplies.Handed.Journal -> org.json.JSONObject().put("journal", h.line)
            })
        }.toString()
    }

    /** The store's writes on disk ([Native.storeDrain]), behind what was sent to the exact thread before it: at most
     * [ms] in all, the main thread waiting (the activity going to the background). Whether they were. */
    fun drainStore(ms: Long): Boolean {
        val until = System.nanoTime() + ms * 1_000_000
        return try {
            waitForOwner(ms) { Native.storeDrain(((until - System.nanoTime()) / 1_000_000).coerceAtLeast(0).toInt()) }
        } catch (e: java.util.concurrent.TimeoutException) { false }
    }

    /** Run [f] on the exact thread and wait for it (Back, a surface's window): rare, and the answer is needed now. */
    fun <T> sync(publish: Boolean = true, f: () -> T): T {
        if (Looper.myLooper() == thread.looper) return f()
        return waitForOwner(null) { f().also { if (publish) produce() } }
    }

    private fun <T> waitForOwner(waitMs: Long?, work: () -> T): T {
        if (Looper.myLooper() == main.looper) Exact.backHandoff?.let { return it.awaitOwner(handler, waitMs, work) }
        val task = FutureTask(work)
        handler.post(task)
        return if (waitMs == null) task.get() else task.get(waitMs, java.util.concurrent.TimeUnit.MILLISECONDS)
    }

    /** Run [f] on the exact thread and wait for it, publishing nothing: a question that changes nothing
     * ([ExactForma]). */
    fun <T> ask(f: () -> T): T = sync(publish = false, f = f)

    /** exact2's next stream, if it has one: copied, queued, and the main thread told. */
    private fun produce(vsync: Boolean = false) {
        // This frame does what a queued `again` was for.
        handler.removeCallbacks(again)
        if (!running) { schedule(); return }
        drainWindow()
        android.os.Trace.beginSection("exact native frame")
        val started = System.nanoTime()
        val fresh = Native.frame(vsync)
        val picture = Native.presentToken()
        late = System.nanoTime() - started > frameNanos
        android.os.Trace.endSection()
        val told = Native.scrollers()
        val fields = Native.fields()
        val bars = Native.bars()
        // A host library without the call (an older exact2) is not asked again.
        val access = if (accessible) try { Native.access(accessAgain).also { accessAgain = false } } catch (e: UnsatisfiedLinkError) { accessible = false; null } else null
        // The bindings ask again first, so a frame's colours read the new answers; every turn, as a token only
        // native code reads moves no paint (one inputs call per binding).
        Formas.turned()
        if (fresh == null && (told != null || fields != null || bars != null || access != null)) {
            queue.add(Pub(timerReports.publish(), null, true, seq, 0, told, fields, bars, access))
            Native.timerObservation(readerScrolling, readerSampling, true, false)
            if (!notifying.getAndSet(true)) postMain { notifying.set(false); published() }
        }
        if (fresh != null) {
            // A heap copy: no native allocation or Cleaner per stream, at up to a stream a frame.
            val words = IntArray(fresh.capacity() / 4)
            fresh.order(ByteOrder.LITTLE_ENDIAN).asIntBuffer().get(words)
            val ints = IntBuffer.wrap(words)
            // The pictures this stream announces, made here before the main thread needs them.
            val pics = Native.pendingPictures()
            if (pics.isNotEmpty()) {
                android.os.Trace.beginSection("exact pictures")
                for (k in pics.indices step 3) if (!made.containsKey(pics[k])) made[pics[k]] = makePicture(pics[k], pics[k + 1], pics[k + 2], hwbuf)
                android.os.Trace.endSection()
            }
            queue.add(Pub(timerReports.publish(), ints, ints.limit() > 0 && ints.get(0) == 22, seq, Native.feed(), told, fields, bars, access, picture))
            Native.timerObservation(readerScrolling, readerSampling, true, false)
            if (!notifying.getAndSet(true)) postMain { notifying.set(false); published() }
        }
        produced()
    }

    /** After a turn, with or without a stream: the chrome, the requests, and the next turn and timer. */
    private fun produced() {
        val c = Native.chrome()
        val cold = coldChrome
        val number = windowRead
        // A window change can leave chrome identical. It still counts once its turn has run.
        if (cold != null && (c != null || number != windowPosted)) {
            windowPosted = number
            postMain(mainNow) {
                if (chrome === cold) {
                    if (c != null) cold(c)
                    windowApplied = number
                }
            }
        } else if (c != null) postMain { chrome(c) }
        requests(if (cold != null) mainNow else main)
        schedule()
    }

    /** A turn at vsync [t]: the frame's [deadline] and its vsyncs' [period] where Choreographer gives them ([Turns]),
     * else 0. Told to exact2 with when it ran and the publication it made, if any, whose ingest finishes its work. */
    private fun turned(t: Long, deadline: Long, period: Long, arrival: Long) {
        // The display's frame: the shortest gap between consecutive vsyncs seen.
        if (vsync > 0 && t - vsync > 2_000_000) frameNanos = minOf(frameNanos, t - vsync)
        vsync = t
        turnPosted = false
        // The next vsync is asked for while this one is current. Android 10's SurfaceFlinger turns the app's vsync
        // off between requests, and a request made within a few milliseconds of the next vsync waits for the one
        // after: asked for after the frame, a Note 9 frame that ended 2.8 ms before the vsync lost it.
        if (running && Native.animating()) { turnPosted = true; choreographer?.let { turns.post(it) } }
        val before = timerReports.publication
        HostFacts.turned(t, arrival, period); produce(vsync = true)
        if (!timing) return
        val published = if (timerReports.publication != before) timerReports.publication else 0L
        try {
            Native.frameTurn(arrival, t, deadline, period, System.nanoTime(), published)
            // No turn asked for: the run ends, so the idle time to the next is no frame time.
            if (!turnPosted) Native.frameIdle()
            Native.frameRates()?.let { r -> main.post { frameRates.told(r); rates(r) } }
        } catch (e: UnsatisfiedLinkError) { timing = false }
    }
    private val turns = Turns(::turned)
    private var vsync = 0L
    private var frameNanos = 1_000_000_000L / 60
    /** A frame that overran its display frame (a GPU canvas whose world ran two ticks in it, 256,000 cubes on a
     * Pixel 5) with no turn asked for waits for no vsync: Choreographer would skip the one it missed, and every
     * frame after would take two. The canvas's buffers hold at most three frames on the GPU, and while they do a
     * turn only runs its ticks. */
    private val again = Runnable { produce() }
    /** Timers, and the paint a scroll owes once it pauses (only then the collection pass it left behind). */
    private val timer = Runnable {
        if (running && prefetch && Native.owed() && Native.refine(-1, if (led) velocity else 0f) and 2 != 0) handler.post(refiner)
        produce()
    }

    private fun schedule() {
        activated()
        wakes?.sync()
        if (!running) {
            val due = Native.background(timerReports.mainWork != 0)
            BackgroundAudio.reconcile(::postMain)
            requests()
            handler.removeCallbacks(timer)
            if (due >= 0) handler.postDelayed(timer, kotlin.math.ceil(due).toLong())
            return
        }
        BackgroundAudio.reconcile(::postMain)
        // One producer at a time. A turn asked for when an overrunning turn began is for the vsync that passed
        // during it, and runs as soon as this returns: that is the retry, and `again` only stands in where no turn
        // is asked for. With nothing animating, none is left asked for.
        if (!idlePosted && Native.timerHeld() and 16 != 0 && readerKnown && !timerReports.applying) {
            idlePosted = true
            val captured = reader
            thread.looper.queue.addIdleHandler {
                idlePosted = false
                if (reader === captured && readerKnown) {
                    if (Native.timerObservation(readerScrolling, readerSampling, timerReports.applying, true)) produce()
                }
                false
            }
        }
        val animating = Native.animating()
        // Contract frame tasks and live GPU frames both need their display callback after an overrun.
        if (animating && late && !turnPosted && !Native.animating(vsyncOnly = true)) {
            late = false
            handler.removeCallbacks(again); handler.post(again)
        } else if (animating && !turnPosted) { turnPosted = true; choreographer?.let { turns.post(it) } }
        else if (!animating && turnPosted) {
            choreographer?.let { turns.remove(it) }; turnPosted = false
            idle()
        }
        var due = Native.nextDue()
        // A moved paint is owed a paint once scrolling pauses: each step pushes this back.
        if (Native.owed() && (due < 0 || due > 16)) due = 16.0
        handler.removeCallbacks(timer)
        if (due >= 0) handler.postDelayed(timer, kotlin.math.ceil(due).toLong())
    }
}

/** dp/s from which a scroll leads, and below which it stops leading; the velocity a lead passes (px/s: past any
 * viewport's cap, two viewports at a quarter second of travel). */
private const val LEAD_ON = 8000f
private const val LEAD_OFF = 4000f
private const val LEAD_PX = 1e6f
/** ms without a scroll step after which a leading window stops leading. */
private const val SETTLE_MS = 100L
/** A slice of laying a press's screen out ahead: the microseconds it may shape for. */
private const val AHEAD_US = 1500

/** Calls the reader makes outside [ExactView]'s own (keys, Back, GPU surfaces): to the [Owner] when there is one. */
internal object Exact {
    /** The process's owner, from the first activity that made one on: an activity made again takes it. */
    var owner: Owner? = null
    /** Whether an activity or [Headless] has set this process up for a host (its environment is written, its host
     * may run). */
    var live = false
    /** Whether [Headless] set the process up and no activity has been made in it since: the first one's launching
     * intent is the launch's ([launch], [launchShare]). */
    var unshown = false
    /** The activity whose view reads the host now. */
    var reader: java.lang.ref.WeakReference<ExactActivity>? = null
        set(value) { field = value; BackgroundAudio.updateMainThreadWakes() }

    /** Main thread, when an answer was published in time (the reader draws it this frame). */
    var answered: () -> Unit = {}

    fun key(code: Int, backspace: Boolean) {
        val o = owner
        if (o != null) { o.input(); if (o.answer(KEY_WAIT_MS) { Native.key(code, backspace) }) answered() } else Native.key(code, backspace)
    }

    /** A gamepad's whole state ([Pads]): 17 buttons, then 4 axes. `at` is the event's `eventTime`, 0 for none. */
    fun pad(index: Int, connected: Boolean, values: FloatArray, at: Long) {
        val o = owner
        if (o != null) { o.input(); if (o.answer(KEY_WAIT_MS) { Native.pad(index, connected, values, at) }) answered() } else Native.pad(index, connected, values, at)
    }

    fun namedKey(code: String, key: String, down: Boolean, repeat: Boolean, shift: Boolean) {
        val o = owner
        if (o != null) { o.input(); if (o.answer(KEY_WAIT_MS) { Native.namedKey(code, key, down, repeat, shift) }) answered() }
        else Native.namedKey(code, key, down, repeat, shift)
    }

    /** How long a press or release waits for exact2's answer before the frame goes without it: what is left of a
     * 120 Hz frame after input (a longer wait holds the frame; 16 ms cost 9-15 janky frames per daily scenario). */
    const val WAIT_MS = 5L
    /** A key's wait: typing sends them in bursts. */
    const val KEY_WAIT_MS = 2L

    @Volatile var backHandoff: BackHandoff? = null

    fun hostedBack(on: Boolean): Boolean {
        val o = owner ?: return Native.hostedBack(on)
        return o.sync(publish = false) { Native.hostedBack(on) }
    }

    fun back(kind: Int, id: Int): Int = owner?.sync { Native.back(kind, id) } ?: Native.back(kind, id)
    /** Predictive Back's start (0), progress (1) or cancel (2): drawn in the frame it arrives for, as a key is. */
    fun backProgress(kind: Int, id: Int, phase: Int, progress: Float, edge: Int) {
        val o = owner
        if (o != null) { o.input(); if (o.answer(WAIT_MS) { Native.backProgress(kind, id, phase, progress, edge) }) answered() }
        else Native.backProgress(kind, id, phase, progress, edge)
    }
    /** A URL given to the running app ([Native.open], LLP 1133.023 D15 item 3): on exact2's thread, in order. */
    fun open(href: String) { val o = owner; if (o != null) { o.input(); o.call { Native.open(href) } } else Native.open(href) }
    fun share(request: String) { val o = owner; if (o != null) { o.input(); o.call { Native.share(request) } } else Native.share(request) }
    /** The launch location of the first activity over a host [Headless] booted ([Native.launch], [Native.launchShare]):
     * on exact2's thread, behind that boot and ahead of the first frame the activity's reader takes. */
    fun launch(href: String) { owner?.call { Native.launch(href) } }
    fun launchShare(request: String) { owner?.call { Native.launchShare(request) } }

    /** A driver's request ([Native.agent], [AgentDrive]): answered on exact2's thread, which then publishes what it
     * painted; waited for, since the driver's next request follows its answer. Carried only where exact2 has a
     * thread of its own (not with owner=0, the benchmark's compare switch). */
    fun agent(line: String): String {
        val o = owner ?: return "{\"error\":\"the reader carries a driver only with exact2 on its own thread (owner=0 is set)\"}"
        val result = o.sync { o.drainWindow(); String(Native.agent(line.toByteArray(Charsets.UTF_8)), Charsets.UTF_8) }
        // The main thread takes the turn's sound requests before reporting their output.
        val refresh = FutureTask {
            Extensions.clock()
            if (android.system.Os.getenv("EXACT_AGENT") == "live") withSoundState(line, result) else result
        }
        Handler(Looper.getMainLooper()).post(refresh)
        return refresh.get()
    }

    private fun withSoundState(line: String, result: String): String {
        try {
            if (JSONObject(line).optString("op") != "state") return result
            val state = JSONObject(result)
            val sounds = state.optJSONObject("sounds") ?: return result
            sounds.put("output", reader?.get()?.soundState() ?: SoundOutput.closedState())
            return state.toString()
        } catch (_: JSONException) {
            return result
        }
    }

    /** The waiting agent's request/answer turn, called by JNI on the exact thread. */
    @JvmStatic fun agentTurn(requests: IntArray?): String = checkNotNull(owner).agentTurn(requests)

    /** What an [ExactForma] asks ([Native.forma], LLP 1121 D12): on exact2's thread, waited for, publishing
     * nothing. An error's JSON where no host runs or the library predates the call. */
    fun forma(request: String): String {
        val f = { try { Native.forma(request) } catch (e: UnsatisfiedLinkError) { "{\"error\":\"no forma in this library\"}" } }
        val o = owner
        return if (o != null) o.ask(f) else if (live) f() else "{\"error\":\"no host\"}"
    }

    /** Main thread, where exact2 runs there too (owner=0) and a hosted field told it something: the reader draws
     * what that changed. */
    var turned: () -> Unit = {}

    private fun mainThreadTurned() {
        turned()
        BackgroundAudio.reconcileOnMain()
    }

    /** What a hosted text field tells exact2 ([Fields], LLP 1115.001 D8), on exact2's thread and in order. */
    private fun field(f: () -> Unit) {
        val o = owner
        if (o != null) { o.input(); o.call(f) } else { f(); mainThreadTurned() }
    }

    fun moduleEvent(id: Int, slot: Int, generation: Int, session: Long, kind: String, payload: String) =
        field { Native.moduleEvent(id, slot, generation, session, kind, payload) }

    fun moduleSizes(reports: Collection<() -> org.json.JSONArray?>) = field {
        val sizes = org.json.JSONArray()
        for (report in reports) report()?.let(sizes::put)
        if (sizes.length() > 0) Native.moduleSizes(sizes.toString())
    }

    fun frameLoaded(id: Int, slot: Int, generation: Int, session: Long) =
        field { Native.frameLoaded(id, slot, generation, session) }

    fun frameFocused(id: Int, slot: Int, generation: Int, session: Long, focused: Boolean) =
        field { Native.frameFocused(id, slot, generation, session, focused) }

    fun frameEvent(id: Int, slot: Int, generation: Int, session: Long, event: String, record: String) =
        field { Native.frameEvent(id, slot, generation, session, event, record) }

    /** A media element's event (by HTML's name, [payload] its value) or, with no [event], its observed [state]
     * (LLP 1042.001 D7), checked against the allocation and session its painted element carries. */
    fun media(id: Int, slot: Int, generation: Int, session: Long, event: String, payload: String, state: String) =
        field { Native.media(id, slot, generation, session, event, payload, state) }

    fun fieldFocused(id: Int, generation: Int, focused: Boolean) = field { Native.fieldFocused(id, generation, focused) }
    fun fieldEdited(id: Int, generation: Int, seq: Int, value: String, start: Int, end: Int) =
        field { Native.fieldEdited(id, generation, seq, value, start, end) }
    fun fieldComposing(id: Int, generation: Int, composing: Boolean, value: String) =
        field { Native.fieldComposing(id, generation, composing, value) }
    fun fieldScrolled(id: Int, generation: Int, x: Int, y: Int) = field { Native.fieldScrolled(id, generation, x, y) }
    fun fieldSelected(id: Int, generation: Int, start: Int, end: Int) = field { Native.fieldSelected(id, generation, start, end) }
    fun fieldAction(id: Int, generation: Int, action: Int) = field { Native.fieldAction(id, generation, action) }
    fun fieldInput(id: Int, generation: Int, kind: Int) = field { Native.fieldInput(id, generation, kind) }

    /** What happened in the window of surface [serial] of the top layer ([Surfaces], [Native.surface], LLP 1115.001
     * D10): told on exact2's thread, in order. exact2 answers with the surface painted again or closed (a request,
     * [Requests]) and with whatever a chosen row's handler changed in the page (a stream). */
    fun surface(serial: Int, what: Int, x: Float, y: Float) {
        val o = owner
        if (o != null) {
            if (what != 2) o.input()
            val deliver = { Native.surface(serial, what, x, y) }
            if (what == Surfaces.DISMISSED && backHandoff != null) o.sync(f = deliver) else o.call(deliver)
        }
        else { Native.surface(serial, what, x, y); mainThreadTurned() }
    }

    /** What happened in bar [which] ([Bars], [Native.bar], LLP 1115.001 D11): told on exact2's thread, in order.
     * exact2 answers with the bar painted again (a target lit) and with whatever the press changed in the page. */
    fun bar(which: Int, what: Int, x: Float, y: Float) {
        val o = owner
        if (o != null) { if (what != 2) o.input(); o.call { Native.bar(which, what, x, y) } }
        else { Native.bar(which, what, x, y); mainThreadTurned() }
    }

    /** What an accessibility service asks of node [id] of the tree exact2 last told ([Access], [Native.accessAct],
     * LLP 1115.001 D13): on exact2's thread, and waited for, since the service is answered with whether it was
     * done. exact2 then publishes what it changed. */
    fun accessAct(id: Int, action: Int, value: Float, target: Int = 0): Boolean {
        val o = owner
        return if (o != null) { o.input(); o.sync { Native.accessAct(id, action, value, target) } } else Native.accessAct(id, action, value, target).also { mainThreadTurned() }
    }

    /** A drag is at ([x], [y]) px, left, or a field took its drop ([DragIn], LLP 1094.001 D5): on exact2's thread,
     * which sends the nodes under it their enters and leaves and publishes what they changed. */
    fun dragOver(x: Float, y: Float) = dragged { Native.dragOver(x, y) }
    fun dragLeft() = dragged { Native.dragLeft() }
    fun dragDropped(x: Float, y: Float) = dragged { Native.dragDropped(x, y) }

    /** The drag was released at ([x], [y]) px: the drop's number, 0 when nothing there takes it. Waited for, since
     * Android is answered with whether the window took the drop, and published after the answer. */
    fun dropReleased(x: Float, y: Float): Int {
        val o = owner ?: return Native.dropReleased(x, y).also { mainThreadTurned() }
        o.input()
        return o.sync(publish = false) { Native.dropReleased(x, y) }.also { o.call {} }
    }

    /** Who takes a drop Android gives field [id]'s view at ([x], [y]) px ([Native.fieldDrop]), asked once as it lands. */
    fun fieldDrop(id: Int, x: Float, y: Float): Int = owner?.ask { Native.fieldDrop(id, x, y) } ?: Native.fieldDrop(id, x, y)

    /** Drop [id]'s items arrived ([DropItems.json]). */
    fun dropLanded(id: Int, items: String) = dragged { Native.dropLanded(id, items) }

    private fun dragged(f: () -> Unit) { val o = owner; if (o != null) o.call(f) else { f(); mainThreadTurned() } }

    /** What a hatch's handle asks of node [node] ([ExactElement]): on exact2's thread, waited for, as an
     * accessibility service's act is; exact2 runs it between its turns and publishes what it changed. */
    fun hatchAct(node: Int, kind: Int, text: String?): Long {
        val o = owner
        return if (o != null) { o.input(); o.sync { Native.hatchAct(node, kind, text) } } else Native.hatchAct(node, kind, text).also { mainThreadTurned() }
    }

    /** When the last touch lifted, or was held long enough to be a long press ([android.os.SystemClock.uptimeMillis]):
     * what a surface's first frame is timed from (`x_log_surfaces=1`). */
    var touched = 0L

    /** The answer to a request exact2 made ([Native.answer]), or its cancellation (null): on exact2's thread. */
    fun answer(id: Int, answer: String?) {
        val o = owner
        if (o != null) o.hand(OwnerReplies.Handed.Answer(id, answer)) else Native.answer(id, answer)
    }

    fun refuseModule(id: Int, error: String, message: String?) {
        val o = owner
        if (o != null) o.hand(OwnerReplies.Handed.Refused(id, error, message)) else Native.refuseModule(id, error, message)
    }

    /** A package calls an app handler on the host's thread and publishes what it changed. */
    fun invoke(body: String): Int = owner?.sync {
        try { Native.invoke(body) } catch (e: UnsatisfiedLinkError) { -1 }
    } ?: Native.invoke(body).also { mainThreadTurned() }

    /** What the extensions tell exact2 ([Native.changed], [Native.bindExtensions]): on exact2's thread, in order. */
    fun changed(topic: String) { val o = owner; if (o != null) o.hand(OwnerReplies.Handed.Changed(topic)) else Native.changed(topic) }
    /** A person's act on what an extension posted ([Activations]): its event raised with a person's origin, as an
     * input, on exact2's thread; [action] is the declared action's record a notification's action carries. */
    fun activate(extension: String, event: String, args: String, action: String?) {
        val o = owner
        if (o != null) { o.input(); o.call { Native.activate(extension, event, args, action) } } else Native.activate(extension, event, args, action)
    }
    /** A control's press ([Controls]), as an input, on exact2's thread: its call's id, or null for one refused
     * outright. */
    fun press(kind: String, digest: String, foreground: Boolean, cold: Boolean, hidden: Boolean): Long? {
        val o = owner ?: return null
        o.input()
        val reply = o.sync { Native.press(kind, digest, foreground, cold, hidden) }
        return runCatching { JSONObject(reply) }.getOrNull()?.takeIf { it.has("call") }?.getLong("call")
    }
    fun cancelPress(id: Long) { owner?.call { Native.cancelPress(id) } }
    /** Whether press [id] still runs: asked on exact2's thread and waited for. */
    fun pressLive(id: Long): Boolean = owner?.sync(publish = false) { Native.pressLive(id) } ?: false
    /** A line for the session's journal, on exact2's thread, in order with what the extensions tell ([changed]). */
    fun journal(line: String) { val o = owner; if (o != null) o.hand(OwnerReplies.Handed.Journal(line)) else Native.journal(line) }
    private val sensorMain = Handler(Looper.getMainLooper())

    fun sensorFrame() { owner?.sensorFrame() ?: sensorMain.post { mainThreadTurned() } }
    /** A turn on exact2's thread for an asynchronous result. */
    fun wake() { owner?.call {} }
    fun presence(answer: String) { val o = owner; if (o != null) o.call { Native.presence(answer) } else Native.presence(answer) }
    /** A live activity's answer or list ([Activities]), on exact2's thread. */
    fun activities(report: String) { val o = owner; if (o != null) o.call { Native.activities(report) } else Native.activities(report) }
    // Until the activity or Headless says otherwise, the host boots with a visible page.
    private var pageHidden: Boolean? = null
    /** Execution sources: a started Activity, or a successfully promoted microphone, call or screen-capture
     * foreground service with a live claim, or an active media playback service. Agent claims grant no device execution. */
    internal fun executionEligible(): Boolean = pageHidden != true || Claims.keepsExecution
    /** The page freeze drops gated streams before returning; socket close handshakes finish asynchronously. */
    fun hidden(hidden: Boolean) {
        val first = pageHidden == null
        pageHidden = hidden
        // The first visible notification would wait for the boot. Later, the agent may have changed the page.
        if (first && !hidden) return
        publishExecution()
    }
    /** The page of a host [Headless] boots is hidden from the start. What tells the host so, for its thread to run as
     * the host boots ([Owner.boot]): told later, the first turn and the data module's activation would run in a
     * visible page. What an activity or a claim says afterwards reaches that thread behind the boot. */
    fun hiddenAtBoot(): () -> Unit {
        pageHidden = true
        val eligible = executionEligible()
        return { Native.hidden(true, eligible) }
    }
    internal fun executionChanged() { if (pageHidden == true) publishExecution() }
    private fun publishExecution() {
        val hidden = pageHidden == true
        val eligible = executionEligible()
        val o = owner
        if (o != null) o.sync { Native.hidden(hidden, eligible) } else { Native.hidden(hidden, eligible); mainThreadTurned() }
    }
    /** The store's writes on disk before the activity goes to the background, so a kill after its stop loses none
     * committed before (exact2 writes them on a thread of its own): at most [ms]. Whether every commit was written
     * ([Native.storeDrain]). */
    fun drainStore(ms: Long = 2000): Boolean = try {
        val o = owner
        if (o != null) o.drainStore(ms) else Native.storeDrain(ms.toInt())
    } catch (e: UnsatisfiedLinkError) { true }
    fun orientationApplied(applied: String, conflict: Boolean, line: String?) {
        val o = owner
        if (o != null) o.call { Native.orientationApplied(applied, conflict, line) } else Native.orientationApplied(applied, conflict, line)
    }
    fun bindExtensions(names: String) { val o = owner; if (o != null) o.call { Native.bindExtensions(names) } else Native.bindExtensions(names) }
    /** A package's picture for request [id] ([Native.picture]), or why there is none: on exact2's thread. */
    fun picture(id: Int, bitmap: android.graphics.Bitmap?, error: String?) {
        val o = owner
        if (o != null) o.call { Native.picture(id, bitmap, error) } else Native.picture(id, bitmap, error)
    }
    /** Picture [source] changed or was revoked ([Native.pictureChanged]): on exact2's thread, in order with what the
     * extensions tell ([changed]). */
    fun pictureChanged(source: String, revoked: Boolean) {
        val o = owner
        if (o != null) o.hand(OwnerReplies.Handed.Picture(source, revoked)) else Native.pictureChanged(source, revoked)
    }

    /** Give a GPU canvas its window: on the exact thread, without waiting for it; [done] there with the handle. */
    fun attachSurface(view: Int, surface: android.view.Surface, width: Int, height: Int, done: (Long) -> Unit) {
        val o = owner
        if (o != null) o.call { done(Native.attachSurface(view, surface, width, height)) } else done(Native.attachSurface(view, surface, width, height))
    }

    /** A GPU canvas that renders into buffers the reader draws: on the exact thread, not waited for. */
    fun attachBuffers(view: Int, width: Int, height: Int) {
        val o = owner
        if (o != null) o.call { Native.attachBuffers(view, width, height) } else Native.attachBuffers(view, width, height)
    }

    /** A buffered canvas goes, its buffers with it: on the exact thread, in order with the canvases attached after it,
     * and not waited for (detachwait=1: waited for, to compare). The main thread waited inside its ingest for whatever
     * the exact thread was in the middle of: 10-24 ms behind a collection pass in an xheavy fling, and the next frame
     * began that late (most of that list's janky frames). The buffers themselves are counted references: the reader's
     * bitmaps of them go when it drops them, exact2's when the canvas goes there. */
    var detachWait = false
    fun detachBuffers(view: Int) {
        val o = owner
        val f = { Native.detachSurface(view, 0L) }
        if (o == null) f() else if (detachWait) o.sync(f = f) else o.call(f)
    }

    /** Take a GPU canvas's window back, waiting for it; [window] (read on the exact thread) is the handle, 0 for none. */
    fun detachSurface(view: Int, window: () -> Long) {
        val o = owner
        val f = { val w = window(); if (w != 0L) Native.detachSurface(view, w) }
        if (o != null) o.sync(publish = false, f = f) else f()
    }
}
