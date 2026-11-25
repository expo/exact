package dev.exact.canvas

import android.app.Activity
import android.content.ClipData
import android.content.ClipDescription
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Point
import android.graphics.Rect
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.view.MotionEvent
import android.view.PixelCopy
import android.view.View
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/**
 * A node's drag out of the app (LLP 1094.000 D7): a finger held on a node with `draggable` for the long-press time,
 * which the presenter answers with the node's box, the finger and what the node carries. The drag is the platform's
 * ([View.startDragAndDrop]; [View.DRAG_FLAG_GLOBAL] lets another app's window take the drop and
 * [View.DRAG_FLAG_GLOBAL_URI_READ] lets it read the files), and once it has begun, Android's to track and end. An
 * assistive service's drag, started by the node's `ACTION_DRAG_START` with no finger down, carries
 * [View.DRAG_FLAG_ACCESSIBILITY_ACTION] and begins on the accessibility tree's own view ([Access.drag], LLP 1094.000 D9).
 */
internal object DragOut {
    /** A drag exact2 asked for (request kind 23): the node, its box and the finger in the view's pixels, whether an
     * assistive service started it, its text and URL, and its files as the node names them. */
    data class Ask(val node: Int, val box: Rect, val finger: Point, val service: Boolean, val text: String, val url: String, val files: List<String>)

    /** The request's words from [at], as `presenter::drag_out::DragOut::encode` writes them. */
    fun read(w: IntArray, at: Int): Ask {
        val box = Rect(w[at + 1], w[at + 2], w[at + 1] + w[at + 3], w[at + 2] + w[at + 4])
        val (text, afterText) = Requests.string(w, at + 8)
        var (url, i) = Requests.string(w, afterText)
        val files = List(w[i++]) { Requests.string(w, i).also { i = it.second }.first }
        return Ask(w[at], box, Point(w[at + 5], w[at + 6]), w[at + 7] == 1, text, url, files)
    }

    /** The touch down in the activity's window now, by when its first finger went down, while that finger is down;
     * null with none ([touched]). */
    private var down: Long? = null
    private var finger = 0

    /** The touch the reader last held for the long-press time ([held]): a finger's drag is that touch's. */
    private var holding: Long? = null

    /** The activity's touches, handed over before any view takes them. A touch ends when its first finger lifts: a
     * drag that began then would follow another finger. */
    fun touched(event: MotionEvent) {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> { down = event.downTime; finger = event.getPointerId(0) }
            MotionEvent.ACTION_POINTER_UP -> if (event.getPointerId(event.actionIndex) == finger) down = null
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> down = null
        }
    }

    /** The reader held the touch that went down at [downTime] for the long-press time, which exact2 may answer with a
     * finger's drag ([start]). The reader may tell it late, behind a hit reply, once another touch is down. */
    fun held(downTime: Long) { holding = downTime }

    /**
     * Starts a finger's drag, with what the window shows within the node's box as its shadow. The view's own drawing is
     * its frame's work (`onDraw` takes what exact2 published), so the shadow is copied from the window ([PixelCopy]),
     * at most [MOST_PIXELS] pixels drawn at the node's size, and a plain one stands in where the copy fails. Android
     * draws the shadow as the drag begins and takes nothing after (`View.startDragAndDrop`), so the drag begins when
     * the copy arrives, and only while the touch held for it is the latest the activity was handed and its first finger
     * is down: Android drags the window's touch, whichever it is. A finger lifted before then has the long press's
     * feedback and no drag, and so has one while the window has yet to hear an earlier drag's end ([busy]). What
     * happened is journaled when no drag began.
     */
    fun start(activity: Activity, view: View, box: Rect, finger: Point, clip: ClipData, busy: () -> Boolean = { false }) {
        val touch = holding
        val shown = Rect(box).apply { if (!intersect(0, 0, view.width, view.height)) setEmpty() }
        val begin = { picture: Bitmap? ->
            when {
                touch == null || down != touch -> Exact.journal("drag: not begun: the finger held for it lifted before its shadow was drawn")
                busy() -> Exact.journal(UNHEARD)
                !view.startDragAndDrop(clip, Shadow(picture, shown, finger), OWN, flags(false, Build.VERSION.SDK_INT)) ->
                    Exact.journal("drag: the platform began no drag: another drag is under way, or the view left its window")
            }
        }
        if (shown.isEmpty || !view.isAttachedToWindow) return begin(null)
        val at = IntArray(2).also(view::getLocationInWindow)
        val source = Rect(shown).apply { offset(at[0], at[1]) }
        val size = copySize(shown.width(), shown.height())
        val picture = Bitmap.createBitmap(size.x, size.y, Bitmap.Config.ARGB_8888)
        try {
            PixelCopy.request(activity.window, source, picture, { result ->
                begin(picture.takeIf { result == PixelCopy.SUCCESS })
            }, Handler(Looper.getMainLooper()))
        } catch (e: IllegalArgumentException) {
            // A window with no surface to copy from: the plain shadow.
            begin(null)
        }
    }

    /** The size a node's copy is made at: its own, scaled down to at most [MOST_PIXELS] pixels. */
    fun copySize(width: Int, height: Int): Point {
        val scale = min(1.0, sqrt(MOST_PIXELS / (width.toDouble() * height)))
        return Point(max(1, (width * scale).toInt()), max(1, (height * scale).toInt()))
    }

    /** The one-pixel shadow a service's drag is given: Android draws none for it, and needs one should the service go
     * before the drag begins (`View.startDragAndDrop`). */
    val blank: View.DragShadowBuilder get() = Shadow(null, Rect(0, 0, 1, 1), Point(0, 0))

    /** The drag's flags on API level [sdk]: a service's drag says it is one where the platform has the flag (API 32). */
    fun flags(service: Boolean, sdk: Int): Int {
        val flags = View.DRAG_FLAG_GLOBAL or View.DRAG_FLAG_GLOBAL_URI_READ
        return if (service && sdk >= Build.VERSION_CODES.S_V2) flags or View.DRAG_FLAG_ACCESSIBILITY_ACTION else flags
    }

    /**
     * The clip: one item a file, as a content URI the app's [Files] provider serves, then the URL and the text, each
     * as text. A URL is no item's URI: a receiver opens an item's URI as a file's content, as Chrome's page does for
     * every item with one. A file is a shipped `assets/…` name or an `app:/` path ([Files.uri]); one the app does not
     * have is left out and journaled. Null when it carries nothing.
     */
    fun clip(context: Context, text: String, url: String, files: List<String>): ClipData? {
        val found = files.mapNotNull { src ->
            val uri = Files.uri(context, src)
            if (uri == null) Exact.journal("drag: $src could not be read: the app has no such file, so the drag leaves it out")
            uri
        }
        val words = listOf(url, text).filter { it.isNotEmpty() }
        val items = found.map { ClipData.Item(it) } + words.map { ClipData.Item(it) }
        if (items.isEmpty()) return null
        val types = found.map { Files.type(it.lastPathSegment ?: "") } + words.map { ClipDescription.MIMETYPE_TEXT_PLAIN }
        // Other windows see the label when the drag starts, before any drop: a file's name, else none, as a TextView's
        // drag of text has.
        val label = found.firstOrNull()?.lastPathSegment
        return ClipData(ClipDescription(label, types.distinct().toTypedArray()), items.first()).apply { items.drop(1).forEach(::addItem) }
    }

    /** The node's [picture] drawn at its box's size, held where the finger is; with none, a plain translucent card. */
    private class Shadow(private val picture: Bitmap?, private val box: Rect, private val finger: Point) : View.DragShadowBuilder() {
        private val size = Rect(0, 0, box.width().coerceAtLeast(1), box.height().coerceAtLeast(1))

        override fun onProvideShadowMetrics(size: Point, touch: Point) {
            size.set(this.size.right, this.size.bottom)
            touch.set((finger.x - box.left).coerceIn(0, size.x - 1), (finger.y - box.top).coerceIn(0, size.y - 1))
        }

        override fun onDrawShadow(canvas: Canvas) {
            if (picture != null) canvas.drawBitmap(picture, null, size, Paint(Paint.FILTER_BITMAP_FLAG)) else canvas.drawColor(0x40808080)
        }
    }

    private const val MOST_PIXELS = 1024.0 * 1024

    /** The local state of the app's own drags: Android gives it only to the app's own windows ([DragIn] reads it), so
     * a drop that carries it is the app's own, whose files the app may read without a grant. */
    val OWN = Any()

    /** What is journaled for a drag not begun while the window has yet to hear an earlier drag's end. */
    const val UNHEARD = "drag: not begun: the window has yet to hear the end of the drag before it"
}
