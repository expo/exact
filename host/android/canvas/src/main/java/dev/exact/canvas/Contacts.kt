package dev.exact.canvas

import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import kotlin.math.abs

/**
 * Touch arbitration for the reader (LLP 1133.008 D8, LLP 1133.021 §9). Hold events behind the down's hit reply,
 * including subsequent contacts, then replay through the same dispatch path. Hosted views and fields use Android's
 * scroll interception rule; a child can disallow interception through its parent chain.
 */
internal class Contacts(private val view: ExactView, private val scrolling: Scrolling) {
    private val module = ModuleContact { target, matrix -> view.natives?.transform(target, matrix) == true }
    private val pending = ArrayDeque<MotionEvent>()
    private val candidateEvents = ArrayList<MotionEvent>()
    private var candidate: View? = null
    private var originX = 0f
    private var originY = 0f
    private var downX = 0f
    private var downY = 0f
    private var tool = 0
    private var claims = 0
    private var longPresses = 0
    private var contextPending = false
    private var serial = 0
    private var waiting = false
    private var stopped = false
    private var engine = false
    private var field = false
    val fieldKeeps get() = field && view.fields?.keeps == true
    private val slop = ViewConfiguration.get(view.context).scaledTouchSlop
    /** When the contact that is down becomes a long press (uptime ms; 0: none armed). */
    private var heldAt = 0L
    private val held = Runnable {
        // Behind an unanswered hit the long press waits for its place among the held events ([drain]).
        if (waiting) return@Runnable
        heldAt = 0L
        if (!stopped && engine) {
            Exact.touched = android.os.SystemClock.uptimeMillis()
            if (module.owned || candidate != null) {
                val token = serial
                waiting = true
                // Let queued pointers preempt the menu before sending the long-press action to the owner.
                view.ask(downX, downY) {
                    if (token == serial && !stopped) {
                        if (pending.any { it.actionMasked == MotionEvent.ACTION_POINTER_DOWN }) {
                            finishHold(null)
                        } else {
                            view.forward(4, downX, downY, tool, -1L)
                            view.ask(downX, downY) { words ->
                                if (token == serial && !stopped) finishHold(words)
                            }
                        }
                    }
                }
            } else view.forward(4, downX, downY, tool, -1L)
        }
    }

    private fun finishHold(words: IntArray?) {
        waiting = false
        val at = 3 + 11 * (words?.getOrElse(1) { 0 } ?: 0)
        val additional = pending.any { it.actionMasked == MotionEvent.ACTION_POINTER_DOWN }
        if (!additional && words != null && words.getOrElse(at + 7) { longPresses } != longPresses) {
            dropCandidate(); module.cancel(); endEngine(); scrolling.yieldToHosted()
        } else {
            contextPending = false
            admit()
        }
        drain()
    }

    fun hold(e: MotionEvent?) {
        view.removeCallbacks(held)
        heldAt = 0L
        if (e == null) return
        heldAt = e.eventTime + ViewConfiguration.getLongPressTimeout()
        view.postDelayed(held, (heldAt - android.os.SystemClock.uptimeMillis()).coerceAtLeast(0))
    }

    fun resume() { stopped = false }

    fun started() { if (view.started) drain() }

    fun stop() {
        stopped = true; serial++; waiting = false
        hold(null)
        for (e in pending) e.recycle()
        pending.clear(); dropCandidate(); module.clear(); releaseInterception()
        if (field) view.fields?.cancel()
        if (engine) view.forward(3, downX, downY, tool, 0L)
        engine = false; field = false
    }

    fun retired(v: View) {
        if (module.retired(v)) { endEngine(); scrolling.yieldToHosted(); releaseInterception() }
        if (candidate === v) dropCandidate()
    }

    fun scrollClaimed(x: Float, y: Float, tool: Int) {
        dropCandidate(); module.cancel(); hold(null); releaseInterception()
        if (field || engine) view.forward(3, x, y, tool, 0L)
        field = false; engine = false
    }

    fun touch(e: MotionEvent): Boolean {
        if (stopped) return true
        if (e.actionMasked == MotionEvent.ACTION_POINTER_DOWN) hold(null)
        if (!view.started) {
            if (pending.isNotEmpty() || e.actionMasked == MotionEvent.ACTION_DOWN) pending.addLast(MotionEvent.obtain(e))
            if (pending.size > 240) {
                for (held in pending) held.recycle()
                pending.clear()
            }
            return true
        }
        if (waiting) {
            pending.addLast(MotionEvent.obtain(e))
            // Abandon an unanswered hit with a definite recipient, including the field's held down.
            if (pending.size > 240) {
                serial++; waiting = false; hold(null)
                if (field) view.fields?.let { it.under(it.serial, 0) }
                field = false; engine = true
                dropCandidate(); module.clear()
                scrolling.yieldToHosted()
                drain()
            }
            return true
        }
        val down = e.actionMasked == MotionEvent.ACTION_DOWN
        if (down) {
            module.clear(); dropCandidate(); hold(null); releaseInterception()
            downX = e.x; downY = e.y; tool = e.getToolType(0)
            engine = false; field = false
            view.fingers.began(e)
            val caughtFling = scrolling.touch(e)
            field = !caughtFling && view.fields?.touch(e) == true
            if (!field && !caughtFling) { engine = true; view.fingers.relay(e) }
            val token = ++serial
            val scrollToken = scrolling.serial
            val fieldToken = view.fields?.serial ?: 0
            val first = MotionEvent.obtain(e)
            waiting = true
            view.ask(e.x, e.y) { words ->
                try {
                    if (token != serial || stopped) return@ask
                    if (field) {
                        field = view.fields?.under(fieldToken, words.getOrElse(2) { 0 }) == true
                        engine = !field
                    }
                    if (engine) hold(first)
                    scrolling.under(scrollToken, words)
                    claims = words.getOrElse(0) { 0 }
                    val at = 3 + 11 * words.getOrElse(1) { 0 }
                    longPresses = words.getOrElse(at + 7) { 0 }
                    contextPending = words.getOrElse(at + 12) { 0 } != 0
                    if (!field && words.size >= at + 13) {
                        val session = (words[at + 8].toLong() and 0xffffffffL) or (words[at + 9].toLong() shl 32)
                        candidate = view.natives?.taker(words[at], words[at + 5], words[at + 6], session)
                        originX = Float.fromBits(words[at + 10]); originY = Float.fromBits(words[at + 11])
                        if (candidate != null) {
                            if (caughtFling) { engine = true; view.fingers.relay(first); hold(first) }
                            candidateEvents.add(MotionEvent.obtain(first))
                            if (!contextPending) admit()
                        }
                    }
                    waiting = false
                    drain()
                } finally { first.recycle() }
            }
            return true
        }
        if (candidate != null) {
            candidateEvents.add(MotionEvent.obtain(e))
            val dx = abs(e.x - downX); val dy = abs(e.y - downY)
            if (candidateEvents.size >= 240 || e.actionMasked == MotionEvent.ACTION_CANCEL) admit(false)
            else if (e.actionMasked == MotionEvent.ACTION_POINTER_DOWN) admit()
            else if (dx > slop || dy > slop) {
                // Offered its down, the view may yet ask to keep the drag from a sheet.
                admit(!claimed(dx, dy, true))
            } else if (e.actionMasked == MotionEvent.ACTION_UP) admit()
            return true
        }
        if (module.active && engine) {
            val dx = abs(e.x - downX); val dy = abs(e.y - downY)
            if (e.actionMasked == MotionEvent.ACTION_MOVE && (dx > slop || dy > slop)) {
                if (claimed(dx, dy, view.natives?.keeps == true)) {
                    module.clear(); releaseInterception()
                } else endEngine()
            } else if (e.actionMasked == MotionEvent.ACTION_POINTER_DOWN) endEngine()
        }
        if (module.active && e.actionMasked == MotionEvent.ACTION_POINTER_DOWN) scrolling.yieldToHosted()
        if (scrolling.touch(e, module.active && view.natives?.keeps == true)) { hold(null); return true }
        if (field && view.fields?.touch(e) == true) { hold(null); return true }
        if (module.owned) {
            if (e.actionMasked == MotionEvent.ACTION_UP || e.actionMasked == MotionEvent.ACTION_CANCEL) endEngine()
            module.deliver(e)
            if (e.actionMasked == MotionEvent.ACTION_UP || e.actionMasked == MotionEvent.ACTION_CANCEL) releaseInterception()
            return true
        }
        val action = view.fingers.touch(e)
        if (action < 0) return true
        if (action != 2 || abs(e.x - downX) > slop || abs(e.y - downY) > slop) hold(null)
        if (action == 1) Exact.touched = e.eventTime
        if (engine) view.fingers.forward(e, action)
        if (action == 1 || action == 3) engine = false
        return true
    }

    /** Whether a drag past the slop, longer along [dx] or [dy], is Exact's and not the hosted view's. A gesture's claim
     * on that axis always is (bits 0 and 1 of the hit reply's claims). Bit 2 says the up-and-down claim is a sheet's
     * drag alone: the sheet intercepts as a parent does (Material's `BottomSheetBehavior` takes its drag in its
     * `CoordinatorLayout`'s `onInterceptTouchEvent`), so not from a view that [kept] its touch. */
    private fun claimed(dx: Float, dy: Float, kept: Boolean): Boolean =
        if (dx > dy) claims and 1 != 0 else claims and 2 != 0 && !(kept && claims and 4 != 0)

    private fun admit(offer: Boolean = true) {
        val v = candidate ?: return
        val events = candidateEvents.toList()
        candidateEvents.clear(); candidate = null
        hold(null)
        if (offer && module.begin(v, events.first(), originX, originY)) {
            scrolling.hosted()
            // Android starts its long-click timer on down. Keep Exact's contact until movement chooses an axis,
            // so a native button can recognize a hold without taking the surrounding row's swipe.
            if (claims == 0) endEngine()
        } else {
            releaseInterception()
            if (scrolling.dragging) endEngine()
        }
        // A declined down leaves the scroller's tracker intact, including a down that stopped a fling.
        for (e in events.drop(1)) touch(e)
        for (e in events) e.recycle()
    }

    private fun releaseInterception() { view.natives?.requestDisallowInterceptTouchEvent(false) }

    private fun endEngine() {
        hold(null)
        if (engine) view.forward(3, downX, downY, tool, 0L)
        engine = false
    }

    private fun dropCandidate() {
        candidate = null
        for (e in candidateEvents) e.recycle()
        candidateEvents.clear()
    }

    /** The events held behind a hit reply, in order. A long press whose time came while they were held goes before
     * the first of them at or past that time, as it would have had the reply come at once. */
    private fun drain() {
        while (view.started && !waiting && !stopped && pending.isNotEmpty()) {
            if (heldAt > 0L && pending.first().eventTime >= heldAt) {
                view.removeCallbacks(held)
                held.run()
                continue
            }
            val e = pending.removeFirst()
            try { touch(e) } finally { e.recycle() }
        }
    }
}
