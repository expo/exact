package dev.exact.canvas

import android.content.Context
import android.graphics.RectF
import android.graphics.RenderNode
import android.os.SystemClock
import android.view.Choreographer
import android.view.MotionEvent
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewGroup
import android.widget.EdgeEffect
import android.widget.FrameLayout
import android.widget.OverScroller
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt
import kotlin.math.sign

/**
 * A finger scrolls (LLP 1115.001 D7.1 to D7.3). The gesture and its physics are the reader's, with the platform's own
 * classes and nothing imitated: the system's touch slop and fling velocities ([ViewConfiguration]), [VelocityTracker],
 * [OverScroller] for the fling, the stretch [EdgeEffect] at the ends, and the scrollbar a [View] draws and fades.
 *
 * The reader moves a scroller at once and exact2 catches up: a scroller whose rows the stream keeps in a node of their
 * own (a group) is translated here, on the UI thread, in the frame the finger moved; exact2 is told how far
 * ([Native.scrolled]) and fills in rows behind it. A scroller with no such node (one inside another's row, one that
 * scrolls sideways, the page itself) moves when exact2's paint of it arrives, which the frame waits a moment for.
 *
 * What crosses to exact2 and back is `host/linux/src/presenter/scrollers.rs`:
 * - at a touch's down the reader asks what is under it ([Native.under]): the scrollers, innermost first, and the axes
 *   exact2's own gestures claim there (a swipe row, a reorder grip, a `pan`);
 * - past the slop, a drag along an axis exact2 claims stays its own; along one a scroller under the finger scrolls
 *   on, the scroll wins: exact2's press and drag recognition for the touch is cancelled ([ExactView.forward] with a
 *   cancel) and no more of the touch reaches it;
 * - a move goes to the innermost scroller first and what it cannot take (it is at its end) to the next one out, as
 *   nested scrolling hands off on Android; what none can take stretches the innermost one's edge;
 * - with each stream exact2 says where the scrollers the reader knows are, where their rows were painted and how far
 *   they go ([Native.scrollers]), with a revision that moves on when the app scrolled one itself: the reader then
 *   takes exact2's offset and ends the fling that was moving it. A scroller that statement leaves out is gone.
 */
internal class Scrolling(private val view: ExactView) {
    /** One scroller as exact2 told it, in the view's pixels. */
    class Scroller(val id: Int) {
        /** Bit 0: it scrolls sideways; bit 1: up and down; bit 2: its rows are a node the reader moves; bit 3: no indicator;
         * bit 4: it hears `refresh`; bit 5: its `refreshing` is true; bits 6 and 7: what a drag of it does to the
         * keyboard ([dismiss]). */
        var flags = 0
            set(value) {
                field = value
                if (barless) { bar?.let { (it.parent as? ViewGroup)?.removeView(it) }; bar = null }
            }
        /** What exact2 calls its revision: a move sent under an older one is dropped there. */
        var rev = 0
        /** Whether a statement has said where it is. */
        var told = false
        /** Where exact2 has it, where its rows were last painted, and its farthest offset. */
        var nowX = 0f; var nowY = 0f; var drawnX = 0f; var drawnY = 0f; var maxX = 0f; var maxY = 0f
        /** What the reader moved it by that no stream it has taken includes yet. */
        var pendX = 0f; var pendY = 0f
        /** The reader has moved it: its rows are placed here, not by exact2's moves. */
        var own = false
        /** The box it shows in. */
        val port = RectF()
        /** What the view draws for its rows once an edge has stretched: see [record]. */
        var holder: RenderNode? = null
        /** Its edges' effects: top, bottom, left, right. */
        val edges = arrayOfNulls<EdgeEffect>(4)
        var bar: Indicator? = null
        /** Its snap, as the last touch's answer told it ([Native.under]): whether a rest must be a snap position, and
         * sideways ([0]) and up and down ([1]) the snapport's length (0: it does not snap that way), each snap area's
         * least and greatest offsets, and whether a snap may not pass over it (`scroll-snap-stop: always`). */
        var snapMandatory = false
        val snapPort = FloatArray(2)
        val snaps = arrayOf(FloatArray(0), FloatArray(0))
        val stops = arrayOf(BooleanArray(0), BooleanArray(0))
        /** It is a virtualized list: its areas past those told are asked for at a release ([Native.snapsAround]). */
        var snapList = false
        val barless get() = flags and 8 != 0
        val node get() = flags and 4 != 0
        val refreshes get() = flags and 16 != 0
        val refreshing get() = flags and 32 != 0
        /** Its pull to refresh, while that shows ([Refresh]). */
        var pull: Refresh? = null
        /** Its `refreshing` was seen true since its pull sent `refresh`. */
        var seen = false
        /** Its `keyboardDismissMode`: 0 nothing, [Dismissal.ON_DRAG] or [Dismissal.INTERACTIVE]. */
        val dismiss get() = (flags shr 6) and 3
        fun at(sideways: Boolean) = if (sideways) nowX + pendX else nowY + pendY
        fun end(sideways: Boolean) = if (sideways) maxX else maxY
    }

    /** A scroller under the touch, and the axes it takes part on for a drag from there. */
    private class Link(val s: Scroller, val x: Boolean, val y: Boolean)
    /** A move sent to exact2's thread: its number, and how far it moved which scroller. */
    private class Sent(val seq: Int, val s: Scroller, val dx: Float, val dy: Float)

    private val scrollers = HashMap<Int, Scroller>()
    private val sent = ArrayDeque<Sent>()
    private val config = ViewConfiguration.get(view.context)
    private val slop = config.scaledTouchSlop
    private val minFling = config.scaledMinimumFlingVelocity
    private val maxFling = config.scaledMaximumFlingVelocity
    private val frames = Choreographer.getInstance()
    /** Its settle onto a snap position ([OverScroller.startScroll]) eases out as ViewPager's does. */
    private val fling = OverScroller(view.context) { t -> val u = t - 1f; u * u * u * u * u + 1f }
    private val dismissal = Dismissal(view)
    /** The scroller a pull to refresh is pulling, and how far the finger has pulled it, px. */
    private var pulling: Scroller? = null
    private var pulled = 0f
    private var tracker: VelocityTracker? = null

    /** The touch that is down: counted, so an answer for one that ended is not taken for the next. */
    var serial = 0
        private set
    private var mode = IDLE
        set(value) {
            // A fling that ends, however it ends (at rest, caught by a finger, stopped by the app), is told.
            if (field == FLINGING && value != FLINGING) rested()
            field = value
        }
    val timerActive: Boolean get() = mode != IDLE
    /** What exact2 said is under the touch, once it has. */
    private var chain: List<Link> = emptyList()
    private var claims = 0
    private var answered = false
    /** The drag's or the fling's axis, and the scrollers it moves, innermost first. */
    private var sideways = false
    private var active: List<Scroller> = emptyList()
    private var pointer = -1
    private var tool = 0
    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f
    private var lastY = 0f
    private var flingAt = 0
    /** Where the drag's innermost scroller was when the drag began, along its axis ([snapRest]'s start). */
    private var dragFrom = 0f
    /** What came of the touch that is down before exact2's answer for it ([touch]), and its down. */
    private val early = ArrayList<MotionEvent>()
    private var down: MotionEvent? = null
    /** A touch that ended before exact2's answer for it came, while a later one began: its serial, its down and what
     * followed. Answers come in the order the downs were asked about, so these are taken in order ([owed]). */
    private class Owed(val serial: Int, val events: List<MotionEvent>)
    private val owing = ArrayDeque<Owed>()
    /** An earlier touch is being taken now ([owed]): exact2 heard all of it already, so nothing is cancelled there. */
    private var replaying = false
    /** The scrollers the drag has moved, each told to exact2 as dragged from its first move ([move]) and held so
     * until the lift, or until the fling the lift threw stops ([lifted], [rested]). */
    private val held = LinkedHashSet<Scroller>()
    /** The scroller that first moved in the last [move], and whether this drag has said what it does to the keyboard. */
    private var mover: Scroller? = null
    private var keyed = false

    /**
     * A touch event: whether it was taken here (a scroll's), so that exact2 does not hear it. A down, and every move
     * until the touch is known to be a scroll, goes on to exact2: it may be a tap, a press, or a gesture of its own.
     *
     * Nothing of a touch is decided before exact2's answer for its down ([under]) says what is under it. What comes
     * before the answer is kept ([early]) and taken when it comes, event by event and in order, as it would have been
     * had the answer come at once ([next], [caughtUp]): a frame that answers late loses none of the finger's way, its
     * turns at a scroller's end, a finger that lifted, or the lift itself.
     */
    fun touch(e: MotionEvent): Boolean {
        if (e.actionMasked == MotionEvent.ACTION_DOWN) {
            val ended = early.any { it.actionMasked == MotionEvent.ACTION_UP || it.actionMasked == MotionEvent.ACTION_CANCEL }
            val first = down
            if (mode == WATCHING && !answered && ended && first != null) {
                // The touch before this one is over and still waits for its answer: it is taken when that comes.
                owing.addLast(Owed(serial, listOf(first) + early))
                early.clear(); down = null
            } else forget()
            down = MotionEvent.obtain(e)
            tracker?.recycle()
            tracker = VelocityTracker.obtain().also { it.addMovement(e) }
            pointer = e.getPointerId(0); tool = e.getToolType(0)
            downX = e.x; downY = e.y; lastX = e.x; lastY = e.y
            serial++
            if (mode == FLINGING) {
                // A finger on a list that is flinging stops it and has it, as the platform's lists do: no press.
                halt()
                // The drag that catches it begins where it was caught ([snapRest]'s start).
                if (active.any { it.port.contains(e.x, e.y) }) {
                    mode = DRAGGING
                    dragFrom = active.first().at(sideways)
                    keyed = false
                    return true
                }
            }
            mode = WATCHING; answered = false; chain = emptyList(); claims = 0
            return false
        }
        if (mode == WATCHING && !answered) { early.add(MotionEvent.obtain(e)); return false }
        return take(e)
    }

    /** [e], a touch event after the down, once exact2 has answered for the touch: whether the scroll took it. */
    private fun take(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_MOVE -> {
                tracker?.addMovement(e)
                val i = e.findPointerIndex(pointer)
                if (i < 0) return mode == DRAGGING
                val x = e.getX(i); val y = e.getY(i)
                if (mode == WATCHING) decide(x, y)
                if (mode != DRAGGING) return false
                val d = if (sideways) lastX - x else lastY - y
                lastX = x; lastY = y
                if (d != 0f) drag(d, x, y)
                dismissal.moved(y)
                return true
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                if (mode != DRAGGING) return false
                // Another finger takes the drag, as ScrollView's does.
                val i = e.actionIndex
                pointer = e.getPointerId(i); lastX = e.getX(i); lastY = e.getY(i)
                return true
            }
            MotionEvent.ACTION_POINTER_UP -> {
                val i = e.actionIndex
                if (e.getPointerId(i) == pointer) {
                    // The finger followed lifts and one still down is followed, as ScrollView's onSecondaryPointerUp
                    // has it. Not yet a scroll, the slop is measured from where that finger is now.
                    val other = if (i == 0) 1 else 0
                    pointer = e.getPointerId(other); lastX = e.getX(other); lastY = e.getY(other)
                    if (mode == WATCHING) { downX = lastX; downY = lastY }
                    tracker?.clear()
                }
                return mode == DRAGGING
            }
            else -> {
                val lifted = e.actionMasked == MotionEvent.ACTION_UP
                if (lifted) tracker?.addMovement(e)
                val was = mode
                if (was == DRAGGING) release(lifted) else mode = IDLE
                tracker?.recycle(); tracker = null
                return was == DRAGGING
            }
        }
    }

    /**
     * The next event of the touch that came before exact2's answer for it ([early]), taken now that the answer has
     * come: whether the scroll has the touch from it on. A touch a hosted field held for itself is let go of one event
     * at a time through this ([Fields.under]), each taken here before the field has it, as a live one is: once the
     * scroll has it, the field had the touch only up to there and is cancelled. False for an answer that is not
     * touch [serial]'s.
     */
    fun next(serial: Int): Boolean {
        if (serial != this.serial || !answered) return false
        val e = early.removeFirstOrNull() ?: return mode == DRAGGING
        try { return take(e) } finally { e.recycle() }
    }

    /** exact2's answer for touch [serial] was given, and anything that held the touch for it has had it: what is
     * left of what came before it is taken, in order. */
    fun caughtUp(serial: Int) {
        if (serial != this.serial || !answered) return
        while (early.isNotEmpty()) next(serial)
    }

    /**
     * The answer [w] for touch [o], which ended before it came while a later touch began: [o] is taken now, from its
     * down to its end, as it would have been had its answer come at once, and a fling it threw stops where the later
     * touch came down. The later touch, which has done nothing yet but wait for its own answer, is set aside meanwhile.
     */
    private fun owed(o: Owed, w: IntArray) {
        val held = ArrayList(early); early.clear()
        val heldDown = down; down = null
        val heldTracker = tracker
        val at = floatArrayOf(downX, downY, lastX, lastY)
        val heldPointer = pointer; val heldTool = tool; val heldMode = mode
        replaying = true
        try {
            val first = o.events[0]
            tracker = VelocityTracker.obtain().also { it.addMovement(first) }
            pointer = first.getPointerId(0); tool = first.getToolType(0)
            downX = first.x; downY = first.y; lastX = first.x; lastY = first.y
            mode = WATCHING
            if (w.size >= 3) {
                answered(w)
                for (e in o.events.drop(1)) take(e)
            }
            if (mode == FLINGING) { halt(); mode = IDLE }
        } finally {
            replaying = false
            for (e in o.events) e.recycle()
            tracker?.recycle()
            tracker = heldTracker; early.addAll(held); down = heldDown
            downX = at[0]; downY = at[1]; lastX = at[2]; lastY = at[3]
            pointer = heldPointer; tool = heldTool
            mode = heldMode; answered = false; chain = emptyList(); claims = 0
        }
    }

    /** What came of the touch before exact2's answer, let go of. */
    private fun forget() {
        for (e in early) e.recycle()
        early.clear()
        down?.recycle(); down = null
    }

    /**
     * The activity stopped, or the view left its window: a fling ends where it is, and a touch that is down, or
     * that exact2 has not answered for yet, is over with what came of it. An answer that comes after moves nothing,
     * so neither does a lift that came before it: nothing scrolls or flings in a window that is not shown.
     */
    fun stopped() {
        // A pull to refresh goes, and stops turning ([Refresh.onDetachedFromWindow]).
        for (s in scrollers.values) s.pull?.let { (it.parent as? ViewGroup)?.removeView(it); s.pull = null }
        for (o in owing) for (e in o.events) e.recycle()
        owing.clear()
        if (mode == DRAGGING) lifted(0f, false)
        pulling?.let { pulled(it, false) }
        dismissal.lifted(0f)
        forget()
        tracker?.recycle(); tracker = null
        released()
        mode = IDLE
    }

    /**
     * exact2's answer for touch [serial] ([Native.under]): the claims, a count, and eleven words a scroller. The
     * statement that tells each scroller's place is published before this arrives ([Owner.under]). It decides
     * nothing itself: what came of the touch meanwhile is taken after it ([next], [caughtUp]).
     */
    fun under(serial: Int, w: IntArray) {
        if (serial != this.serial) { owing.firstOrNull { it.serial == serial }?.let { owing.remove(it); owed(it, w) }; return }
        if (mode != WATCHING || w.size < 3) return
        answered(w)
    }

    /** The claims and the scrollers of exact2's answer [w], for the touch being taken. */
    private fun answered(w: IntArray) {
        claims = w[0]
        val links = ArrayList<Link>(w[1])
        // After the claims and the count, the hosted field under the touch ([Fields.under]); then the scrollers.
        var i = 3
        // The words after the scrollers are the module button's ([Native.under]).
        val end = minOf(w.size, 3 + 11 * w[1])
        while (i + 11 <= end) {
            val s = scrollers.getOrPut(w[i]) { Scroller(w[i]) }
            if (!s.told) { s.rev = w[i + 2]; s.nowX = f(w[i + 3]); s.nowY = f(w[i + 4]); s.maxX = f(w[i + 5]); s.maxY = f(w[i + 6]) }
            // A queued hit reply can predate the latest statement: its policy is only a fallback.
            val policy = (if (s.told) s.flags else w[i + 1]) and SAID
            s.flags = (s.flags and 3) or (w[i + 1] and 4) or policy
            s.port.set(f(w[i + 7]), f(w[i + 8]), f(w[i + 7]) + f(w[i + 9]), f(w[i + 8]) + f(w[i + 10]))
            links.add(Link(s, w[i + 1] and 1 != 0, w[i + 1] and 2 != 0))
            i += 11
        }
        // After the module's eight words, each scroller's snap.
        var at = 3 + 11 * w[1] + 8
        for (link in links) {
            if (at >= w.size) break
            val s = link.s
            s.snapMandatory = w[at] and 4 != 0
            s.snapList = w[at] and 8 != 0
            at++
            for (axis in 0..1) {
                if (at + 2 > w.size) break
                val n = w[at + 1]
                s.snapPort[axis] = f(w[at])
                s.snaps[axis] = FloatArray(2 * n) { f(w[at + 2 + 3 * (it / 2) + it % 2]) }
                s.stops[axis] = BooleanArray(n) { w[at + 4 + 3 * it] != 0 }
                at += 2 + 3 * n
            }
        }
        // The indicators of scrollers this touch is not over go: they have faded, or are about to.
        for (s in scrollers.values) if (links.none { it.s === s }) s.bar?.let { (it.parent as? ViewGroup)?.removeView(it); s.bar = null }
        chain = links
        answered = true
    }

    /** Past the slop: whose the touch is. Along an axis exact2 claims, its own; along one a scroller under it scrolls
     * on, the scroll's, and exact2 is told to let go of the touch. Otherwise nobody's yet. */
    private fun decide(x: Float, y: Float) {
        val ax = abs(x - downX); val ay = abs(y - downY)
        if (max(ax, ay) <= slop) return
        // A hosted text field that drags its own selection or caret asked to keep its touch, as it asks a ScrollView.
        if (view.fieldKeeps()) return
        val across = ax > ay
        if (claims and (if (across) 1 else 2) != 0) { mode = THEIRS; return }
        val takes = { side: Boolean -> chain.any { if (side) it.x else it.y } }
        val other = (if (across) ay else ax) > slop && claims and (if (across) 2 else 1) == 0
        sideways = when {
            takes(across) -> across
            other && takes(!across) -> !across
            else -> return
        }
        active = chain.filter { if (sideways) it.x else it.y }.map { it.s }
        dragFrom = active.firstOrNull()?.at(sideways) ?: 0f
        mode = DRAGGING
        // The drag takes the way the finger went past the slop, as RecyclerView's does: all of it when the move that
        // crossed the slop went far past it (a quick swipe, or moves held back until a late answer).
        lastX = if (sideways) downX + sign(x - downX) * slop else x
        lastY = if (sideways) y else downY + sign(y - downY) * slop
        if (!replaying) view.forward(3, x, y, tool, 0L)
        keyed = false
    }

    /** The finger moved [delta] px along the drag's axis (toward the end positive), at ([x], [y]). */
    private fun drag(delta: Float, x: Float, y: Float) {
        var d = delta
        // A pull to refresh, and a stretched edge, take the finger's way back before anything scrolls.
        pulling?.let { s ->
            if (d <= 0f) return@let
            val back = minOf(d, pulled); pulled -= back; d -= back; s.pull?.pull(pulled)
            // Taken all the way back and on: the pull is over, and the drag scrolls, and flings, as any other.
            if (pulled <= 0f && d > 0f) { pulling = null; unpull(s) }
        }
        val edged = edged()
        if (edged != null) d -= relax(edged, d, x, y)
        mover = null
        val rest = if (d != 0f) move(d) else 0f
        if (!keyed) {
            // What the drag does to the keyboard is the first scroller's it moves, or, while none can, the one whose
            // edge it stretches.
            val s = mover ?: (if (rest != 0f) edged ?: active.firstOrNull() else null)
            if (s != null) {
                keyed = true
                // The keyboard follows only a drag up and down.
                dismissal.began(if (sideways && s.dismiss == Dismissal.INTERACTIVE) 0 else s.dismiss)
            }
        }
        if (rest < 0f && refresh(rest)) return
        if (rest != 0f && edged != null) pull(edged, rest, x, y)
    }

    /**
     * [rest] px toward the start that no scroller of the drag could take, so each is at its start: the innermost that
     * hears `refresh` and is not refreshing is pulled, as Material's pull to refresh takes what the list leaves before
     * its edge would stretch. Whether one was.
     */
    private fun refresh(rest: Float): Boolean {
        if (sideways) return false
        val s = pulling ?: active.firstOrNull { it.refreshes }?.takeIf { !it.refreshing && it.pull == null } ?: return false
        pulling = s
        pulled -= rest
        val r = s.pull ?: Refresh(view.context).also { r ->
            s.pull = r
            (view.parent as? ViewGroup)?.addView(r, FrameLayout.LayoutParams(s.port.width().roundToInt(), s.port.height().roundToInt()))
        }
        r.translationX = view.left + s.port.left
        r.translationY = view.top + s.port.top
        r.pull(pulled)
        return true
    }

    /** The pull of [s] was let go of ([lifted]) or taken by the system: past the threshold, `refresh` is sent and the
     * indicator turns until the app's `refreshing` ends ([told]); short of it, or taken, it goes. */
    private fun pulled(s: Scroller, lifted: Boolean) {
        pulling = null; pulled = 0f
        val r = s.pull ?: return
        if (!lifted || r.fraction < 1f) { unpull(s); return }
        s.seen = false
        r.refreshing()
        tell { Native.refresh(s.id) }
        view.postDelayed({ if (s.pull === r && !s.seen && !s.refreshing) unpull(s) }, UNSTARTED_MS)
    }

    /** [s]'s pull to refresh goes back up, and away. */
    private fun unpull(s: Scroller) {
        val r = s.pull ?: return
        r.hide { (r.parent as? ViewGroup)?.removeView(r); if (s.pull === r) s.pull = null }
    }

    /**
     * Move the drag's scrollers by [d] px along its axis: the innermost as far as it goes, the rest to the next one
     * out (the hand-off). Each one's rows move now, where it has a node, and exact2 is told. What none could take.
     */
    private fun move(d: Float): Float {
        var rest = d
        var waits = false
        for ((k, s) in active.withIndex()) {
            val at = s.at(sideways)
            val took = (at + rest).coerceIn(0f, s.end(sideways)) - at
            if (abs(took) < 0.01f) continue
            s.own = true
            if (mover == null) mover = s
            // The finger drags it from its first move (`scrolldragstart`), as a nested scroller the hand-off reaches.
            if (mode == DRAGGING && held.add(s)) tell { Native.scrollDrag(s.id, false, 0f, 0f, false, 0f, 0f, false) }
            val dx = if (sideways) took else 0f; val dy = if (sideways) 0f else took
            val seq = view.scrolled(s.id, dx, dy, s.rev)
            // Told to exact2's own thread: it is ahead of exact2 by this much until a stream includes the move.
            if (seq != 0) { s.pendX += dx; s.pendY += dy; sent.addLast(Sent(seq, s, dx, dy)) }
            waits = waits || !s.node
            // The scrollers inside it went with it.
            for (j in 0 until k) { val inner = active[j]; inner.port.offset(-dx, -dy); inner.bar?.let { b -> place(b, inner) } }
            indicate(s)
            rest -= took
            if (abs(rest) < 0.01f) { rest = 0f; break }
        }
        place()
        view.fields?.place()
        view.tick()
        if (waits) view.awaitScrolled()
        return rest
    }

    /** Each scroller the reader has moved, drawn where the reader has it: its rows' node as far from where they were
     * painted as the offset is. Called after every stream too, which draws them where exact2 had them. */
    fun place() {
        for (s in scrollers.values) {
            if (!s.own || !s.node || !s.told) continue
            val g = view.group(s.id) ?: continue
            g.translationX = s.drawnX - s.nowX - s.pendX
            g.translationY = s.drawnY - s.nowY - s.pendY
        }
    }

    /** How far scroller [id]'s rows are drawn from where they were painted, down, when the reader moves it. */
    fun shift(id: Int): Float? = scrollers[id]?.takeIf { it.own && it.node && it.told }?.let { it.drawnY - it.nowY - it.pendY }

    /** A stream that includes the reader's moves up to number [seq] was taken. */
    fun acked(seq: Int) {
        while (sent.isNotEmpty() && sent.first().seq <= seq) { val m = sent.removeFirst(); m.s.pendX -= m.dx; m.s.pendY -= m.dy }
        if (sent.isEmpty()) for (s in scrollers.values) { s.pendX = 0f; s.pendY = 0f }
    }

    /** exact2's statement with a stream ([Native.scrollers]): nine words a scroller. */
    fun told(w: IntArray) {
        var i = 0
        while (i + 9 <= w.size) {
            val s = scrollers.getOrPut(w[i]) { Scroller(w[i]) }
            s.flags = w[i + 1]
            s.nowX = f(w[i + 3]); s.nowY = f(w[i + 4]); s.drawnX = f(w[i + 5]); s.drawnY = f(w[i + 6])
            s.maxX = f(w[i + 7]); s.maxY = f(w[i + 8])
            if (s.told && w[i + 2] != s.rev) {
                // The app moved it: what the reader sent under the old revision is dropped there and here, and a
                // fling that was moving it ends. A finger that is dragging it goes on from where the app put it.
                sent.removeAll { it.s === s }
                s.pendX = 0f; s.pendY = 0f
                if (mode == FLINGING && s in active) { halt(); mode = IDLE }
            }
            s.rev = w[i + 2]
            s.told = true
            // A refresh this reader's pull began ends when the app's `refreshing` that followed it does.
            s.pull?.takeIf { it.spinning && !it.leaving }?.let { if (s.refreshing) s.seen = true else if (s.seen) unpull(s) }
            i += 9
        }
        // A scroller the statement no longer has is gone (its node went, or a driver took it over): what the reader
        // kept for it goes with it.
        val count = w.size / 9
        if (scrollers.size <= count) return
        for (s in scrollers.values.filter { s -> s.told && (0 until count).none { w[it * 9] == s.id } }) {
            scrollers.remove(s.id)
            sent.removeAll { it.s === s }
            s.bar?.let { (it.parent as? ViewGroup)?.removeView(it) }
            s.pull?.let { (it.parent as? ViewGroup)?.removeView(it) }; s.pull = null
            if (pulling === s) { pulling = null; pulled = 0f }
            if (mode == FLINGING && s in active) { halt(); mode = IDLE }
        }
    }

    /** A driver from outside scrolls ([ExactView.scrollBy]): the scrollers are exact2's to place again. */
    fun released() {
        if (mode == FLINGING) { halt(); mode = IDLE }
        for (s in scrollers.values) s.own = false
    }

    /** The activity stopped, or a touch took a fling: the fling ends where it is. */
    fun halt() {
        fling.forceFinished(true)
        frames.removeFrameCallback(flinger)
    }

    /** The finger lifted ([lifted]) or the system took the touch: a fling at the finger's velocity, or rest. */
    private fun release(lifted: Boolean) {
        var v = 0f
        val t = tracker
        if (lifted && t != null) {
            t.computeCurrentVelocity(1000, maxFling.toFloat())
            v = -(if (sideways) t.getXVelocity(pointer) else t.getYVelocity(pointer))
        }
        dismissal.lifted(if (sideways) 0f else -v)
        pulling?.let { s -> lifted(v, false); mode = IDLE; pulled(s, lifted); return }
        val edged = edged()
        val stretched = edged != null && STRETCH && edged.edges.any { it != null && it.distance != 0f }
        if (edged != null) { for (e in edged.edges) e?.onRelease(); if (stretched) stretching() }
        if (!stretched && snap(v)) { lifted(v, mode == FLINGING); return }
        if (stretched || abs(v) < minFling || active.isEmpty()) { lifted(v, false); mode = IDLE; return }
        mode = FLINGING
        flingAt = 0
        // Unbounded, as RecyclerView flings: each frame's travel is clamped by the scrollers themselves, whose ends
        // may move while it runs.
        if (sideways) fling.fling(0, 0, v.toInt(), 0, Int.MIN_VALUE, Int.MAX_VALUE, 0, 0)
        else fling.fling(0, 0, 0, v.toInt(), 0, 0, Int.MIN_VALUE, Int.MAX_VALUE)
        lifted(v, true)
        frames.postFrameCallback(flinger)
    }

    /** How a list's areas around a release's projection are asked for ([ExactView.snapsAround]). */
    internal var snapsAround: (Int, Boolean, Float, Float, (IntArray) -> Unit) -> Unit = view::snapsAround

    /**
     * The drag's innermost scroller snaps along the drag's axis (LLP 1008 §9): it comes to rest where [snapRest] says,
     * from where the platform's fling at [v] px/s would have left it, and settles there in ViewPager's time. At its end
     * and flung on past it, it does not: the fling goes on to the scrollers outside. Whether it took the release; a
     * list's `proximity` snap that the areas the touch told leave free does not, and may still take it once the
     * areas around the projection arrive ([aimAgain]).
     */
    private fun snap(v: Float): Boolean {
        val s = active.firstOrNull() ?: return false
        val axis = if (sideways) 0 else 1
        if (s.snapPort[axis] <= 0f || s.snaps[axis].isEmpty()) return false
        val at = s.at(sideways)
        val end = s.end(sideways)
        if ((v < 0f && at <= 0f) || (v > 0f && at >= end)) return false
        val flung = abs(v) >= minFling
        var projected = at
        if (flung) {
            if (sideways) fling.fling(0, 0, v.toInt(), 0, Int.MIN_VALUE, Int.MAX_VALUE, 0, 0)
            else fling.fling(0, 0, 0, v.toInt(), 0, 0, Int.MIN_VALUE, Int.MAX_VALUE)
            projected += if (sideways) fling.finalX else fling.finalY
            fling.forceFinished(true)
        }
        val way = if (flung) (if (v > 0f) 1 else -1) else 0
        val target = snapRest(s.snaps[axis], s.stops[axis], projected, at, dragFrom, way, s.snapMandatory, s.snapPort[axis])
        if (target == null && !s.snapList) return false
        if (target != null) {
            val d = (target.coerceIn(0f, end) - at).roundToInt()
            // ViewPager's settle: four times as long as the finger's speed would take to cover it, within its bound;
            // a release too slow to fling takes the platform's default scroll duration.
            settle(d, if (flung) settleMs(d, v) else SNAP_MS)
        }
        // A list told only the areas around where the touch landed: those around the projection, and an `always` one
        // on the way, are asked for now, and the settle (or the fling) is aimed again with them.
        if (s.snapList) {
            val aim = Aim(serial, s.rev, axis, projected, at, dragFrom, way, target != null)
            snapsAround(s.id, !sideways, at, projected) { w -> aimAgain(s, aim, w) }
        }
        return target != null
    }

    /** ViewPager's settle time over [d] px for a finger, or a fling, moving at [v] px/s. */
    private fun settleMs(d: Int, v: Float) = (4000f * abs(d) / abs(v)).roundToInt().coerceIn(100, SNAP_MAX_MS)

    /** A release's snap, as [snap] decided it: for touch [touch], the scroller at revision [rev], and whether it
     * [settles] (else it flings, or rests, unsnapped). */
    private class Aim(
        val touch: Int, val rev: Int, val axis: Int, val projected: Float, val release: Float, val start: Float, val way: Int,
        val settles: Boolean,
    )

    /** When the snap's settle of [d] px is to end. */
    private var settleEnds = 0L

    /** Settle the drag's scrollers [d] px along its axis in [ms]. */
    private fun settle(d: Int, ms: Int) {
        fling.forceFinished(true)
        frames.removeFrameCallback(flinger)
        if (d == 0) { mode = IDLE; return }
        fling.startScroll(0, 0, if (sideways) d else 0, if (sideways) 0 else d, ms)
        settleEnds = SystemClock.uptimeMillis() + ms
        mode = FLINGING
        flingAt = 0
        frames.postFrameCallback(flinger)
    }

    /**
     * A list's areas around its release's projection arrived ([Native.snapsAround]): merged with those the touch told,
     * and the settle aimed again where [snapRest] now says, in the time it had left (a fling that was not snapping, in
     * ViewPager's time at its speed), unless another touch began since or the app scrolled the list (a new revision).
     * A rest past the areas the touch told is otherwise the last of them, behind the projection.
     */
    private fun aimAgain(s: Scroller, aim: Aim, w: IntArray) {
        if (aim.touch != serial || aim.rev != s.rev || active.firstOrNull() !== s || (mode != FLINGING && mode != IDLE)) return
        mergeSnaps(s, aim.axis, w)
        val target = snapRest(s.snaps[aim.axis], s.stops[aim.axis], aim.projected, aim.release, aim.start, aim.way, s.snapMandatory,
            s.snapPort[aim.axis]) ?: return
        val d = (target.coerceIn(0f, s.end(sideways)) - s.at(sideways)).roundToInt()
        if (d == 0 && mode == IDLE) return
        val ms = when {
            aim.settles -> (settleEnds - SystemClock.uptimeMillis()).toInt().coerceIn(100, SNAP_MAX_MS)
            mode == FLINGING && fling.currVelocity > 0f -> settleMs(d, fling.currVelocity)
            else -> SNAP_MS
        }
        settle(d, ms)
    }

    /** The finger let go of the drag at [v] px/s along its axis, positive as the offset grows (`VelocityTracker`'s,
     * negated): exact2 is told of each scroller it moved, with where that will rest, which is where [fling] ends
     * (exact2 clamps it to the scroller's range) when it [flings] and where it is otherwise; before the fling's first
     * frame, so its `scrolldragend` precedes the fling's `scroll`s. With nothing thrown the scroll is over, and its
     * `scrollend` comes now; a fling's comes when it stops ([rested]). */
    private fun lifted(v: Float, flings: Boolean) {
        val travel = if (!flings) 0f else (if (sideways) fling.finalX else fling.finalY).toFloat()
        val vx = if (sideways) v else 0f; val vy = if (sideways) 0f else v
        for (s in held) {
            val restX = s.at(true) + (if (sideways) travel else 0f)
            val restY = s.at(false) + (if (sideways) 0f else travel)
            tell { Native.scrollDrag(s.id, true, vx, vy, true, restX, restY, flings) }
        }
        if (!flings) held.clear()
    }

    /** The fling the drag threw stopped: exact2 is told ([Native.flingEnded]; each scroller's `scrollend`). */
    private fun rested() {
        for (s in held) tell { Native.flingEnded(s.id) }
        held.clear()
    }

    /** [f], a call that tells exact2 of a drag, on its thread behind the moves sent before, or here. */
    private fun tell(f: () -> Unit) {
        if (!view.started) return
        val o = view.owner
        if (o != null) { o.call(f); return }
        f()
        view.pump()
    }

    private val flinger: Choreographer.FrameCallback = Choreographer.FrameCallback {
        if (mode != FLINGING) return@FrameCallback
        if (!fling.computeScrollOffset()) { mode = IDLE; view.timerReport(); return@FrameCallback }
        val at = if (sideways) fling.currX else fling.currY
        val d = (at - flingAt).toFloat()
        flingAt = at
        var done = fling.isFinished
        if (d != 0f) {
            val rest = move(d)
            if (rest != 0f) {
                // Every scroller is at its end: what is left of the fling goes into the edge.
                if (STRETCH) edged()?.let { s -> edge(s, toward(rest > 0f)).onAbsorb(fling.currVelocity.toInt()); stretching() }
                fling.forceFinished(true)
                done = true
            }
        }
        if (done) mode = IDLE else frames.postFrameCallback(flinger)
        view.timerReport()
    }

    // The ends: the platform's stretch. Android 12's: before it a scroller's end does not move.

    /** The scroller whose edge stretches: the innermost of the drag that has a node to stretch. */
    private fun edged(): Scroller? = active.firstOrNull { it.node }

    private fun toward(end: Boolean) = if (sideways) (if (end) RIGHT else LEFT) else (if (end) BOTTOM else TOP)

    private fun edge(s: Scroller, which: Int): EdgeEffect = s.edges[which] ?: EdgeEffect(view.context).also { s.edges[which] = it }

    /** Where across [s]'s port the finger is, 0 to 1, as the edge at [which] counts it (ScrollView's and
     * HorizontalScrollView's displacement). */
    private fun across(s: Scroller, which: Int, x: Float, y: Float): Float {
        val a = if (sideways) (y - s.port.top) / max(1f, s.port.height()) else (x - s.port.left) / max(1f, s.port.width())
        return (if (which == TOP || which == RIGHT) a else 1f - a).coerceIn(0f, 1f)
    }

    private fun length(s: Scroller) = max(1f, if (sideways) s.port.width() else s.port.height())

    /** How much of [d] a stretched edge of [s] takes back as the finger returns toward it. */
    private fun relax(s: Scroller, d: Float, x: Float, y: Float): Float {
        if (!STRETCH) return 0f
        val size = length(s)
        val start = s.edges[toward(false)]; val end = s.edges[toward(true)]
        val took = when {
            d < 0f && end != null && end.distance != 0f -> size * end.onPullDistance(d / size, across(s, toward(true), x, y))
            d > 0f && start != null && start.distance != 0f -> -size * start.onPullDistance(-d / size, across(s, toward(false), x, y))
            else -> 0f
        }
        if (took != 0f) stretching()
        return took
    }

    /** [rest] px the finger moved with every scroller at its end: [s]'s edge that way stretches. */
    private fun pull(s: Scroller, rest: Float, x: Float, y: Float) {
        if (!STRETCH) return
        val which = toward(rest > 0f)
        edge(s, which).onPullDistance(abs(rest) / length(s), across(s, which, x, y))
        s.edges[toward(rest < 0f)]?.takeIf { !it.isFinished }?.onRelease()
        stretching()
    }

    private var edging = false
    /** An edge moves: each frame until it rests, the scrollers that have one are recorded again. */
    private fun stretching() {
        for (s in scrollers.values) if (s.holder == null && s.node && s.edges.any { it != null }) {
            // The view's drawing names the scroller's rows directly until now: it is recorded once more, naming the
            // holder ([holder]).
            s.holder = RenderNode("exact scroller").apply { setClipToBounds(false) }
            view.again()
        }
        if (edging) return
        edging = true
        frames.postFrameCallback(edger)
    }
    private val edger = Choreographer.FrameCallback {
        edging = false
        var more = false
        for (s in scrollers.values) if (s.holder != null) more = record(s) || more
        view.tick()
        if (more) stretching()
    }

    /** What the view draws for scroller [id]'s rows in place of their node, once an edge of it has stretched. */
    fun holder(id: Int): RenderNode? = scrollers[id]?.holder?.also { if (!it.hasDisplayList()) scrollers[id]?.let { s -> record(s) } }

    /**
     * [s]'s holder, recorded: its rows' node, and while an edge is stretched that node inside one the size of the
     * scroller's port with the edges drawn into it. [EdgeEffect.draw] is the platform's own stretch: it stretches the
     * node it is recorded into, as it does a ScrollView's. It adds to the stretch that node has, and only the view
     * system can clear one, so each frame of a stretch records a new node. Whether an edge was still moving.
     */
    private fun record(s: Scroller): Boolean {
        val holder = s.holder ?: return false
        val g = view.group(s.id) ?: return false
        val live = s.edges.any { it != null && !it.isFinished }
        holder.setPosition(0, 0, view.width, view.height)
        val c = holder.beginRecording()
        if (!live) c.drawRenderNode(g) else {
            val l = s.port.left.roundToInt(); val t = s.port.top.roundToInt()
            val w = max(1, s.port.width().roundToInt()); val h = max(1, s.port.height().roundToInt())
            val port = RenderNode("exact port")
            port.setPosition(l, t, l + w, t + h)
            val p = port.beginRecording()
            p.translate(-l.toFloat(), -t.toFloat())
            p.drawRenderNode(g)
            p.translate(l.toFloat(), t.toFloat())
            // As ScrollView.draw and HorizontalScrollView.draw place them.
            s.edges[TOP]?.takeIf { !it.isFinished }?.let { it.setSize(w, h); it.draw(p) }
            s.edges[BOTTOM]?.takeIf { !it.isFinished }?.let {
                val n = p.save(); p.translate(-w.toFloat(), h.toFloat()); p.rotate(180f, w.toFloat(), 0f)
                it.setSize(w, h); it.draw(p); p.restoreToCount(n)
            }
            s.edges[LEFT]?.takeIf { !it.isFinished }?.let {
                val n = p.save(); p.rotate(270f); p.translate(-h.toFloat(), 0f)
                it.setSize(h, w); it.draw(p); p.restoreToCount(n)
            }
            s.edges[RIGHT]?.takeIf { !it.isFinished }?.let {
                val n = p.save(); p.rotate(90f); p.translate(0f, -w.toFloat())
                it.setSize(h, w); it.draw(p); p.restoreToCount(n)
            }
            port.endRecording()
            c.drawRenderNode(port)
        }
        holder.endRecording()
        return live
    }

    // The indicators: the platform's scrollbar.

    /**
     * The scroll indicator of one scroller: a view over its port that draws nothing of its own. What shows is the
     * scrollbar [View] itself draws for a view that scrolls, from the theme's thumb, at the theme's size, and it fades
     * when and as [View] fades it ([awakenScrollBars]). The style gives it its scrollbars, as `android:scrollbars`
     * gives a ScrollView its.
     */
    class Indicator(context: Context) : View(context, null, 0, R.style.ExactScrollbars) {
        var rangeX = 0; var rangeY = 0; var atX = 0; var atY = 0
        init { importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO }
        override fun computeVerticalScrollRange() = rangeY
        override fun computeVerticalScrollOffset() = atY
        override fun computeVerticalScrollExtent() = height
        override fun computeHorizontalScrollRange() = rangeX
        override fun computeHorizontalScrollOffset() = atX
        override fun computeHorizontalScrollExtent() = width
        fun wake() { awakenScrollBars() }
    }

    /** [s] moved: its indicator shows where it is. */
    private fun indicate(s: Scroller) {
        if (s.barless) return
        val parent = view.parent as? ViewGroup ?: return
        val w = s.port.width().roundToInt(); val h = s.port.height().roundToInt()
        if (w <= 0 || h <= 0) return
        val b = s.bar ?: Indicator(view.context).also { s.bar = it; parent.addView(it, FrameLayout.LayoutParams(w, h)) }
        if (b.layoutParams.width != w || b.layoutParams.height != h) b.layoutParams = FrameLayout.LayoutParams(w, h)
        place(b, s)
        b.rangeX = if (s.flags and 1 != 0) (s.maxX + w).roundToInt() else 0
        b.rangeY = if (s.flags and 2 != 0) (s.maxY + h).roundToInt() else 0
        b.atX = s.at(true).roundToInt(); b.atY = s.at(false).roundToInt()
        b.wake()
    }

    private fun place(b: Indicator, s: Scroller) {
        b.translationX = view.left + s.port.left
        b.translationY = view.top + s.port.top
    }

    private fun f(bits: Int) = java.lang.Float.intBitsToFloat(bits)

    private companion object {
        /** The bits of a scroller's flags its statement is the authority on, and a hit answer only before one. */
        const val SAID = 8 or 16 or 32 or 192
        /** How long after a pull sent `refresh` an app that set no `refreshing` keeps the indicator turning: the iOS
         * host's wait for UIKit's control (`ScrollViewIOS.swift`). */
        const val UNSTARTED_MS = 350L
        const val IDLE = 0
        /** A touch is down and exact2 has it; whether it becomes a scroll is not known yet. */
        const val WATCHING = 1
        /** The touch is a gesture of exact2's own: nothing of it scrolls. */
        const val THEIRS = 2
        const val DRAGGING = 3
        const val FLINGING = 4
        const val TOP = 0
        const val BOTTOM = 1
        const val LEFT = 2
        const val RIGHT = 3
        /** `Scroller`'s default duration, and ViewPager's longest settle. */
        const val SNAP_MS = 250
        const val SNAP_MAX_MS = 600
        /** An edge's stretch, and the distance it is read by, are Android 12's (`EdgeEffect.getDistance`). */
        val STRETCH = android.os.Build.VERSION.SDK_INT >= 31
    }
}

/**
 * Where a scroll released at [release] and projected to rest at [projected] comes to rest among one axis's snap areas
 * ([snaps]: each one's least and greatest offset, which differ for one longer than the snapport; [stops]:
 * `scroll-snap-stop: always`), or null to rest where it would. LLP 1008 §9's rule, as
 * `host/linux/src/presenter/snap.rs`'s `rest`: the position nearest the projection (the smaller of two as near); for a
 * fling [way] (1 as the offset grows, -1 back, 0 for a release too slow to fling), at least the first position past
 * [start], where the drag began, that way; then short of the first `always` position past the release in the way it
 * goes (CSS Scroll Snap 1 §6.2); and for `proximity` ([mandatory] false), only within a third of the snapport ([port])
 * of the projection.
 */
internal fun snapRest(
    snaps: FloatArray, stops: BooleanArray, projected: Float, release: Float, start: Float, way: Int, mandatory: Boolean, port: Float,
): Float? {
    val n = stops.size
    if (n == 0) return null
    var rest = 0f
    var best = Float.MAX_VALUE
    for (k in 0 until n) {
        val p = projected.coerceIn(snaps[2 * k], snaps[2 * k + 1])
        val d = abs(p - projected)
        if (d < best || (d == best && p < rest)) { best = d; rest = p }
    }
    // A move under half a pixel is none: positions from layout are fractional.
    val e = 0.5f
    // A fling that would rest back where its drag began, or behind, goes on to the next position its way.
    if (way != 0 && way * (rest - start) <= e) {
        var next = Float.NaN
        for (k in 0 until n) {
            val p = (start + way * 2 * e).coerceIn(snaps[2 * k], snaps[2 * k + 1])
            if (way * (p - start) > e && (next.isNaN() || way * p < way * next)) next = p
        }
        if (!next.isNaN()) rest = next
    }
    if (rest > release + e) {
        for (k in 0 until n) if (stops[k]) {
            val from = snaps[2 * k]; val to = snaps[2 * k + 1]
            val stop = if (from > release + e) from else if (to > release + e) to else continue
            rest = minOf(rest, stop)
        }
    } else if (rest < release - e) {
        for (k in 0 until n) if (stops[k]) {
            val from = snaps[2 * k]; val to = snaps[2 * k + 1]
            val stop = if (to < release - e) to else if (from < release - e) from else continue
            rest = maxOf(rest, stop)
        }
    }
    return if (mandatory || abs(rest - projected) <= port / 3f) rest else null
}

/** Add [w]'s snap areas ([Native.snapsAround]: a count, then each one's least and greatest offsets as float bits and 1
 * for `always`) to [s]'s along [axis]. */
internal fun mergeSnaps(s: Scrolling.Scroller, axis: Int, w: IntArray) {
    val n = w.getOrElse(0) { 0 }.coerceAtMost((w.size - 1) / 3)
    if (n <= 0) return
    s.snaps[axis] = s.snaps[axis] + FloatArray(2 * n) { java.lang.Float.intBitsToFloat(w[1 + 3 * (it / 2) + it % 2]) }
    s.stops[axis] = s.stops[axis] + BooleanArray(n) { w[3 + 3 * it] != 0 }
}
