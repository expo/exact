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
    private var downTime = 0L
    private var tool = 0
    private var claims = 0
    private var longPresses = 0
    private var contextPending = false
    private var serial = 0
    private var waiting = false
    private var stopped = false
    private var engine = false
    private var field = false
    /** Whether the hosted view under the finger could scroll the way the finger went past the slop; null before. */
    private var scrollsAlong: Boolean? = null
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
                            DragOut.held(downTime); view.forward(4, downX, downY, tool, -1L, at = -1L)
                            view.ask(downX, downY) { words ->
                                if (token == serial && !stopped) finishHold(words)
                            }
                        }
                    }
                }
            } else if (contextPending) {
                longPress()
            } else {
                val token = serial
                waiting = true
                view.textSelection.begin(downX, downY) { took ->
                    if (token == serial && !stopped) {
                        if (took) {
                            endEngine(); scrolling.yieldToHosted()
                            waiting = false
                            drain()
                        } else longPress()
                    }
                }
            }
        }
    }

    /** The long press to exact2. One it took (its menu, or a drag that lifts as the finger moves on, LLP 1094.000 D7) keeps
     * the rest of the contact from a scroller, which would otherwise claim the finger's next move along its axis. */
    private fun longPress() {
        DragOut.held(downTime); view.forward(4, downX, downY, tool, -1L, at = -1L)
        val token = serial
        // The moves that come meanwhile wait for the answer ([drain]), so a scroller cannot claim one first.
        waiting = true
        view.ask(downX, downY) { words ->
            if (token == serial && !stopped) {
                if (longPressesIn(words) != longPresses) scrolling.yieldToHosted()
                waiting = false
                drain()
            }
        }
    }

    /** The count of long presses exact2 has taken, in a hit reply ([Native.under]). */
    private fun longPressesIn(words: IntArray): Int {
        val at = 3 + 11 * words.getOrElse(1) { 0 } + (if (words.getOrElse(0) { 0 } and 32 != 0) 5 else 0)
        return words.getOrElse(at + 7) { longPresses }
    }

    private fun finishHold(words: IntArray?) {
        waiting = false
        val additional = pending.any { it.actionMasked == MotionEvent.ACTION_POINTER_DOWN }
        if (!additional && words != null && longPressesIn(words) != longPresses) {
            dropCandidate(); module.cancel()
            if (words[0] and 64 == 0) endEngine() // A lifted reorder row still follows this contact.
            scrolling.yieldToHosted()
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
        view.textSelection.stop()
        hold(null)
        for (e in pending) e.recycle()
        pending.clear(); dropCandidate(); module.clear(); releaseInterception()
        if (field) view.fields?.cancel()
        if (engine) view.forward(3, downX, downY, tool, 0L, at = -1L)
        engine = false; field = false
    }

    fun retired(v: View) {
        if (module.retired(v)) { endEngine(); scrolling.yieldToHosted(); releaseInterception() }
        if (candidate === v) dropCandidate()
    }

    fun scrollClaimed(x: Float, y: Float, tool: Int) {
        dropCandidate(); module.cancel(); hold(null); releaseInterception()
        if (field || engine) view.forward(3, x, y, tool, 0L, at = -1L)
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
            downX = e.x; downY = e.y; downTime = e.downTime; tool = e.getToolType(0)
            engine = false; field = false; scrollsAlong = null
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
                    val flags = words.getOrElse(0) { 0 }
                    claims = flags and 31
                    val at = 3 + 11 * words.getOrElse(1) { 0 } + (if (flags and 32 != 0) 5 else 0)
                    longPresses = longPressesIn(words)
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
                admit(!claimed(dx, dy, true, e.y - downY))
            } else if (e.actionMasked == MotionEvent.ACTION_UP) admit()
            return true
        }
        if (module.active && engine) {
            val dx = abs(e.x - downX); val dy = abs(e.y - downY)
            if (e.actionMasked == MotionEvent.ACTION_MOVE && (dx > slop || dy > slop)) {
                if (claimed(dx, dy, kept(e), e.y - downY)) {
                    module.clear(); releaseInterception()
                } else endEngine()
            } else if (e.actionMasked == MotionEvent.ACTION_POINTER_DOWN) endEngine()
        }
        if (module.active && e.actionMasked == MotionEvent.ACTION_POINTER_DOWN) scrolling.yieldToHosted()
        if (scrolling.touch(e, module.active && kept(e))) { hold(null); return true }
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

    /** Whether the hosted view that has the contact keeps its drag from a scroller around it: it asked its parent not to
     * intercept, or it can scroll the way the finger went past the slop, as a `ViewPager` leaves a child the drag that
     * child can scroll (`canScroll`). The second is judged once, at the slop, so a frame scrolled to its edge during
     * the drag keeps it, as a browser latches a scroll to one scroller. */
    private fun kept(e: MotionEvent): Boolean {
        if (view.natives?.keeps == true) return true
        val dx = e.x - downX; val dy = e.y - downY
        if (scrollsAlong == null && (abs(dx) > slop || abs(dy) > slop)) scrollsAlong = module.scrolls(dx, dy)
        return scrollsAlong == true
    }

    /** Whether a drag past the slop, longer along [dx] or [dy], is Exact's and not the hosted view's. A gesture's claim
     * on that axis always is (bits 0 and 1 of the hit reply's claims). Bit 2 says the up-and-down claim is a sheet's
     * drag alone: the sheet intercepts as a parent does (Material's `BottomSheetBehavior` takes its drag in its
     * `CoordinatorLayout`'s `onInterceptTouchEvent`), so not from a view that [kept] its touch. Bits 3 and 4
     * are the sheet's upward and downward claims over a nested scroller. */
    private fun claimed(dx: Float, dy: Float, kept: Boolean, vertical: Float): Boolean =
        if (dx > dy) claims and 1 != 0
        else (claims and 2 != 0 && !(kept && claims and 4 != 0)) ||
            (!kept && claims and (if (vertical < 0f) 8 else 16) != 0)

    private fun admit(offer: Boolean = true) {
        val v = candidate ?: return
        val events = candidateEvents.toList()
        candidateEvents.clear(); candidate = null
        hold(null)
        if (offer && module.begin(v, events.first(), originX, originY, place = view.natives?.placesFrame(v) != true)) {
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
        if (engine) view.forward(3, downX, downY, tool, 0L, at = -1L)
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
