package dev.exact.canvas

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.view.ActionMode
import android.view.HapticFeedbackConstants
import android.view.Menu
import android.view.MenuItem
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.widget.Magnifier
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject
import kotlin.math.abs
import kotlin.math.roundToInt

/** Android supplies the handle drawables, highlight colour, magnifier and floating toolbar.
 * The painted paragraph supplies their positions and the text that Copy and Share receive. */
internal class TextSelection(private val view: ExactView) : ActionMode.Callback2() {
    private val appearance by lazy { TextView(view.context) }
    private val ink by lazy { Paint().apply { color = appearance.highlightColor } }
    private class Handle(private val text: TextView) {
        private val left by lazy { copy(text.textSelectHandleLeft) }
        private val right by lazy { copy(text.textSelectHandleRight) }
        fun drawable(pointsLeft: Boolean) = if (pointsLeft) right else left
        private fun copy(d: Drawable?): Drawable? =
            d?.constantState?.newDrawable(text.resources, text.context.theme)?.mutate() ?: d?.mutate()
    }
    private val firstHandle by lazy { Handle(appearance) }
    private val lastHandle by lazy { Handle(appearance) }
    private var first: Drawable? = null
    private var last: Drawable? = null
    private val slop = ViewConfiguration.get(view.context).scaledTouchSlop
    private var mode: ActionMode? = null
    private var marks = emptyList<RectF>()
    private val anchor = Rect()
    private var drag = 0
    private var dx = 0f
    private var dy = 0f
    private var start = RectF()
    private var end = RectF()
    private var tap = false
    private var downX = 0f
    private var downY = 0f
    private var live = false
    private var sequence = 0
    private var refreshing = false
    private var again = false
    private var magnifier: Magnifier? = null

    private fun request(action: Int, x: Float = 0f, y: Float = 0f, answer: (String?) -> Unit = {}) {
        val owner = view.owner
        val read = action == 4 || action == 5
        if (owner == null) {
            answer(Native.readerSelection(action, x, y))
            if (!read) view.turned()
            return
        }
        val run: () -> Unit = {
            val result = Native.readerSelection(action, x, y)
            view.post { answer(result) }; Unit
        }
        if (read) owner.tell(run) else owner.call(run)
    }

    fun begin(x: Float, y: Float, done: (Boolean) -> Unit) {
        val token = ++sequence
        live = true
        request(0, x, y) { reply ->
            if (reply == null || token != sequence || !view.isAttachedToWindow) {
                if (token == sequence && mode == null) live = false
                done(false)
                return@request
            }
            tap = false
            geometry(reply)
            if (mode == null) mode = view.startActionMode(this, ActionMode.TYPE_FLOATING)
            if (mode == null) stop() else view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
            done(mode != null)
        }
    }

    fun refresh() {
        if (mode == null) return
        if (refreshing) { again = true; return }
        refreshing = true
        val token = sequence
        request(4) { reply ->
            refreshing = false
            if (token == sequence && mode != null) {
                if (reply == null) stop() else geometry(reply)
            }
            if (again) { again = false; refresh() }
        }
    }

    private fun bounds(a: JSONArray) = RectF(a.getDouble(0).toFloat(), a.getDouble(1).toFloat(),
        a.getDouble(2).toFloat(), a.getDouble(3).toFloat())

    private fun geometry(reply: String) {
        val data = JSONObject(reply)
        val rectangles = data.getJSONArray("marks")
        marks = List(rectangles.length()) { bounds(rectangles.getJSONArray(it)) }
        val clip = bounds(data.getJSONArray("clip"))
        start = bounds(data.getJSONArray("start")); end = bounds(data.getJSONArray("end"))
        val startRtl = data.getBoolean("startRtl")
        val endRtl = data.getBoolean("endRtl")
        first = firstHandle.drawable(startRtl)
        last = lastHandle.drawable(!endRtl)
        place(first, start, clip, !startRtl); place(last, end, clip, endRtl)
        val area = RectF()
        for (rect in marks) area.union(rect)
        area.roundOut(anchor)
        mode?.invalidateContentRect()
        view.invalidate()
    }

    private fun place(handle: Drawable?, caret: RectF, clip: RectF, leading: Boolean) {
        if (handle == null) return
        val x = caret.left.roundToInt()
        val y = caret.bottom.roundToInt()
        if (x < clip.left || x > clip.right || y < clip.top || y > clip.bottom) { handle.setBounds(0,0,0,0); return }
        val width = handle.intrinsicWidth
        val l = x - (width * if (leading) 0.75f else 0.25f).roundToInt()
        handle.setBounds(l, y, l + width, y + handle.intrinsicHeight)
    }

    fun draw(canvas: Canvas) {
        if (mode == null) return
        for (rect in marks) canvas.drawRect(rect, ink)
        first?.draw(canvas); last?.draw(canvas)
    }

    private fun near(d: Drawable?, x: Float, y: Float): Boolean {
        val r = d?.bounds ?: return false
        return !r.isEmpty && x >= r.left - slop && x <= r.right + slop &&
            y >= r.top - slop && y <= r.bottom + slop
    }

    fun touch(e: MotionEvent): Boolean {
        if (mode == null) return false
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                downX = e.x; downY = e.y
                val onStart = near(first, e.x, e.y)
                val onEnd = near(last, e.x, e.y)
                drag = when {
                    onStart && onEnd -> if (abs(e.x-start.left) <= abs(e.x-end.left)) 1 else 2
                    onStart -> 1
                    onEnd -> 2
                    else -> 0
                }
                tap = drag == 0
                if (drag != 0) {
                    val caret = if (drag == 1) start else end
                    dx = caret.left - e.x; dy = caret.centerY() - e.y
                    view.parent?.requestDisallowInterceptTouchEvent(true)
                    mode?.hide(ActionMode.DEFAULT_HIDE_DURATION.toLong())
                }
            }
            MotionEvent.ACTION_MOVE -> {
                if (abs(e.x-downX) > slop || abs(e.y-downY) > slop) tap = false
                if (drag != 0) {
                    val x = e.x + dx; val y = e.y + dy
                    change(drag, x, y)
                    val glass = magnifier ?: Magnifier.Builder(view).build().also { magnifier = it }
                    glass.show(x, y)
                }
            }
            MotionEvent.ACTION_POINTER_DOWN -> tap = false
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                val owned = drag != 0
                if (owned) {
                    magnifier?.dismiss(); view.parent?.requestDisallowInterceptTouchEvent(false)
                    mode?.hide(0); mode?.invalidateContentRect()
                } else if (tap && e.actionMasked == MotionEvent.ACTION_UP) stop()
                drag = 0; tap = false
                return owned
            }
        }
        return drag != 0
    }

    private fun change(action: Int, x: Float = 0f, y: Float = 0f) {
        val token = ++sequence
        request(action, x, y) { reply ->
            if (token == sequence && mode != null) {
                if (reply == null) stop() else geometry(reply)
            }
        }
    }

    override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
        menu.add(0, android.R.id.copy, 0, android.R.string.copy).setShowAsAction(MenuItem.SHOW_AS_ACTION_ALWAYS)
        menu.add(0, android.R.id.shareText, 1, R.string.exact_share).setShowAsAction(MenuItem.SHOW_AS_ACTION_IF_ROOM)
        menu.add(0, android.R.id.selectAll, 2, android.R.string.selectAll).setShowAsAction(MenuItem.SHOW_AS_ACTION_IF_ROOM)
        return true
    }

    override fun onPrepareActionMode(mode: ActionMode, menu: Menu) = false
    override fun onGetContentRect(mode: ActionMode, source: View, outRect: Rect) = outRect.set(anchor)

    override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
        if (item.itemId == android.R.id.selectAll) { change(3); return true }
        if (item.itemId != android.R.id.copy && item.itemId != android.R.id.shareText) return false
        val token = sequence
        request(5) { text ->
            if (token != sequence || this.mode !== mode) return@request
            if (text != null) {
                if (item.itemId == android.R.id.copy) {
                    view.context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(null, text))
                } else {
                    val share = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)
                    view.context.startActivity(Intent.createChooser(share, null))
                }
            }
            mode.finish()
        }
        return true
    }

    override fun onDestroyActionMode(mode: ActionMode) {
        this.mode = null
        clear()
    }

    fun stop() {
        val active = mode
        if (active != null) active.finish() else clear()
    }

    private fun clear() {
        sequence++; tap = false; drag = 0
        magnifier?.dismiss(); magnifier = null
        if (marks.isNotEmpty()) { marks = emptyList(); view.invalidate() }
        if (live && view.started) request(6)
        live = false
    }
}
