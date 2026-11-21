package dev.exact.canvas

import android.app.Activity
import android.content.ClipData
import android.content.ClipDescription
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Point
import android.graphics.Rect
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.view.PixelCopy
import android.view.View
import java.io.File

/**
 * A node's drag out of the app (LLP 1094.000 D7): a finger held on a node with `draggable` for the long-press time,
 * which the presenter answers with the node's box, the finger and what the node carries. The drag is the platform's
 * ([View.startDragAndDrop]; [View.DRAG_FLAG_GLOBAL] lets another app's window take the drop and
 * [View.DRAG_FLAG_GLOBAL_URI_READ] lets it read the files). An assistive service's drag, started by the node's
 * `ACTION_DRAG_START` with no finger down, carries [View.DRAG_FLAG_ACCESSIBILITY_ACTION] (LLP 1094.000 D9).
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

    /**
     * Starts the drag, with what the window shows within the node's box as its shadow: the view's own drawing is its
     * frame's work (`onDraw` takes what exact2 published), so the shadow is copied from the window ([PixelCopy]), and a
     * plain one stands in where the copy fails. Whether there was anything to drag; what the platform then did is
     * journaled.
     */
    fun start(activity: Activity, view: View, box: Rect, finger: Point, service: Boolean, text: String, url: String, files: List<String>): Boolean {
        val appFile = { src: String -> Native.appFile(src)?.let(::File)?.takeIf(Files::regular) }
        val clip = clip(activity, text, url, files, AssetRoot.of(activity), appFile) ?: return false
        val flags = flags(service, Build.VERSION.SDK_INT)
        val shown = Rect(box).apply { if (!intersect(0, 0, view.width, view.height)) setEmpty() }
        val begin = { picture: Bitmap? ->
            if (!view.startDragAndDrop(clip, Shadow(picture, shown, finger), null, flags)) {
                Exact.journal("drag: the platform began no drag: the finger had lifted, or the view left its window")
            }
        }
        if (shown.isEmpty || !view.isAttachedToWindow) return true.also { begin(null) }
        val at = IntArray(2).also(view::getLocationInWindow)
        val source = Rect(shown).apply { offset(at[0], at[1]) }
        val picture = Bitmap.createBitmap(shown.width(), shown.height(), Bitmap.Config.ARGB_8888)
        try {
            PixelCopy.request(activity.window, source, picture, { result ->
                begin(picture.takeIf { result == PixelCopy.SUCCESS })
            }, Handler(Looper.getMainLooper()))
        } catch (e: IllegalArgumentException) {
            // A window with no surface to copy from: the plain shadow.
            begin(null)
        }
        return true
    }

    /** The drag's flags on API level [sdk]: a service's drag says it is one where the platform has the flag (API 32). */
    fun flags(service: Boolean, sdk: Int): Int {
        val flags = View.DRAG_FLAG_GLOBAL or View.DRAG_FLAG_GLOBAL_URI_READ
        return if (service && sdk >= Build.VERSION_CODES.S_V2) flags or View.DRAG_FLAG_ACCESSIBILITY_ACTION else flags
    }

    /**
     * The clip: one item a file, as a content URI the app's [Files] provider serves, then the URL and the text, each
     * as text. A URL is no item's URI: a receiver opens an item's URI as a file's content, as Chrome's page does for
     * every item with one. A shipped file is found under [assets], the running generation's, and an `app:/` one by
     * [appFile] ([Sources.file]); one that is neither is left out and journaled. Null when it carries nothing.
     */
    fun clip(context: Context, text: String, url: String, files: List<String>, assets: File, appFile: (String) -> File?): ClipData? {
        val found = files.mapNotNull { src ->
            val file = Sources.file(src, assets, appFile)
            val uri = file?.let { Files.uri(context, it) }
            if (uri == null) Exact.journal("drag: $src could not be read: the app has no such file, so the drag leaves it out")
            uri?.let { Triple(it, Files.type(file), file.name) }
        }
        val words = listOf(url, text).filter { it.isNotEmpty() }
        val items = found.map { ClipData.Item(it.first) } + words.map { ClipData.Item(it) }
        if (items.isEmpty()) return null
        val types = found.map { it.second } + words.map { ClipDescription.MIMETYPE_TEXT_PLAIN }
        // Other windows see the label when the drag starts, before any drop: a file's name, else none, as a TextView's
        // drag of text has.
        val label = found.firstOrNull()?.third
        return ClipData(ClipDescription(label, types.distinct().toTypedArray()), items.first()).apply { items.drop(1).forEach(::addItem) }
    }

    /** The node's [picture] at its box's size, held where the finger is; with none, a plain translucent card. */
    private class Shadow(private val picture: Bitmap?, private val box: Rect, private val finger: Point) : View.DragShadowBuilder() {
        override fun onProvideShadowMetrics(size: Point, touch: Point) {
            size.set(box.width().coerceAtLeast(1), box.height().coerceAtLeast(1))
            touch.set((finger.x - box.left).coerceIn(0, size.x - 1), (finger.y - box.top).coerceIn(0, size.y - 1))
        }

        override fun onDrawShadow(canvas: Canvas) {
            if (picture != null) canvas.drawBitmap(picture, 0f, 0f, null) else canvas.drawColor(0x40808080)
        }
    }
}
