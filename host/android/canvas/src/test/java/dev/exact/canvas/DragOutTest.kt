package dev.exact.canvas

import android.content.ClipDescription
import android.content.Intent
import android.graphics.Point
import android.graphics.Rect
import android.net.Uri
import android.view.View
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.io.File

/** A node's drag out of the app (LLP 1094.000 D7) and the share sheet's files: the request as exact2 writes it, the
 * clip's items and what it leaves out, the touch a finger's drag begins under, the drag's flags by API level, and the
 * sheet's intent. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, shadows = [DragOutTest.Journal::class])
class DragOutTest {
    private val context = RuntimeEnvironment.getApplication()
    private val assets = File(context.filesDir, "app").apply { File(this, "assets").mkdirs() }

    @Before fun begin() {
        Journal.lines.clear()
        Journal.saved = File(context.filesDir, "saved.txt")
    }

    /** exact2's journal, as the reader writes to it with no owner, and its confined read of `app:/data/saved.txt`. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Journal {
        companion object {
            val lines = ArrayList<String>()
            lateinit var saved: File
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun journal(line: String) { lines += line }
            @JvmStatic @Implementation fun assetRoot(): String? = null
            @JvmStatic @Implementation fun openApp(path: String): android.os.ParcelFileDescriptor? =
                if (path == "app:/data/saved.txt" && saved.isFile) android.os.ParcelFileDescriptor.open(saved, android.os.ParcelFileDescriptor.MODE_READ_ONLY) else null
        }
    }

    /** `presenter::drag_out_tests::the_request_writes_the_words_the_reader_reads` writes these words: keep the two in
     * step. */
    @Test fun theRequestReadsAsThePresenterWritesIt() {
        val words = intArrayOf(7, 10, 20, 30, 40, 15, 25, 1, 4, 1768584518, 21, 1886680168, 791624307, 1835104357, 778398832,
            795701091, 49, 2, 12, 1702064993, 1630499700, 1735290926, 15, 980447329, 1952539695, 778186593, 6710384)
        val ask = DragOut.read(intArrayOf(99) + words, 1)
        assertEquals(DragOut.Ask(7, Rect(10, 20, 40, 60), Point(15, 25), true, "Fuji", "https://example.com/1",
            listOf("assets/a.png", "app:/data/b.pdf")), ask)
    }

    @Test fun aShippedFileAnAppFileTheUrlAndTheTextAreItsItems() {
        shadowOf(android.webkit.MimeTypeMap.getSingleton()).addExtensionMimeTypeMapping("png", "image/png")
        File(assets, "assets/fuji.png").writeText("png")
        Journal.saved.writeText("saved")

        val clip = DragOut.clip(context, "Mount Fuji", "https://example.com/1016", listOf("assets/fuji.png", "app:/data/saved.txt"))!!

        assertEquals(4, clip.itemCount)
        val authority = "${context.packageName}.exact.files"
        assertEquals("content", clip.getItemAt(0).uri.scheme)
        assertEquals(authority, clip.getItemAt(0).uri.authority)
        assertEquals(listOf("shipped", "assets", "fuji.png"), clip.getItemAt(0).uri.pathSegments)
        assertEquals(listOf("app", "data", "saved.txt"), clip.getItemAt(1).uri.pathSegments)
        assertEquals("https://example.com/1016", clip.getItemAt(2).text)
        assertNull("a URL is no content to open", clip.getItemAt(2).uri)
        assertEquals("Mount Fuji", clip.getItemAt(3).text)
        assertEquals("fuji.png", clip.description.label)
        assertTrue(clip.description.hasMimeType("image/png"))
        assertFalse(clip.description.hasMimeType(ClipDescription.MIMETYPE_TEXT_URILIST))
        assertTrue(clip.description.hasMimeType(ClipDescription.MIMETYPE_TEXT_PLAIN))
        assertEquals(listOf<String>(), Journal.lines)
    }

    /** A missing file, one outside the assets that exists, and an unknown scheme are left out and journaled; with
     * nothing else, there is no drag, and with no file there is no label. */
    @Test fun aFileTheAppDoesNotHaveIsLeftOut() {
        File(context.filesDir, "outside.txt").writeText("not shipped")
        val refused = listOf("assets/missing.png", "../outside.txt", "file:///etc/hosts", "app:/data/gone")
        assertNull(DragOut.clip(context, "", "", refused))
        assertEquals(refused.map { "drag: $it could not be read: the app has no such file, so the drag leaves it out" }, Journal.lines)
        val clip = DragOut.clip(context, "kept", "", listOf("assets/missing.png"))!!
        assertEquals("kept", clip.getItemAt(0).text.toString())
        assertNull("other windows see the label before any drop", clip.description.label)
    }

    /** A finger's drag begins only under the touch the reader held for the long-press time, while that touch's first
     * finger is down: Android drags the window's touch, whichever it is. A hold told late, once another touch is down,
     * begins none. A view in no window lets the drag reach the platform at once, which then begins none. */
    @Test fun aFingersDragBeginsOnlyUnderTheTouchHeldForIt() {
        val view = View(context)
        val clip = DragOut.clip(context, "Mount Fuji", "", emptyList())!!
        val activity = org.robolectric.Robolectric.buildActivity(android.app.Activity::class.java).get()
        val start = { DragOut.start(activity, view, Rect(0, 0, 10, 10), Point(5, 5), clip) }
        val notBegun = "drag: not begun: the finger held for it lifted before its shadow was drawn"
        val tried = "drag: the platform began no drag: another drag is under way, or the view left its window"
        fun touch(action: Int, at: Long, pointers: Int = 1) {
            val ids = Array(pointers) { android.view.MotionEvent.PointerProperties().apply { id = it } }
            val coords = Array(pointers) { android.view.MotionEvent.PointerCoords() }
            DragOut.touched(android.view.MotionEvent.obtain(at, at, action, pointers, ids, coords, 0, 0, 1f, 1f, 0, 0, 0, 0))
        }
        val down = android.view.MotionEvent.ACTION_DOWN
        val up = android.view.MotionEvent.ACTION_UP

        touch(down, 1); DragOut.held(1); start()
        touch(up, 1); start()
        touch(down, 2); start()
        DragOut.held(1); start()
        DragOut.held(2); start()
        // The first finger lifting while a second stays down ends the touch; the second finger's lift ends nothing more.
        touch(android.view.MotionEvent.ACTION_POINTER_DOWN or (1 shl android.view.MotionEvent.ACTION_POINTER_INDEX_SHIFT), 2, 2)
        touch(android.view.MotionEvent.ACTION_POINTER_UP, 2, 2); start()
        touch(up, 2)
        assertEquals(listOf(tried, notBegun, notBegun, notBegun, tried, notBegun), Journal.lines)
    }

    /** The window's copy, held until the test lets it arrive: each request's size, and its answer to give. */
    @Implements(android.view.PixelCopy::class)
    class HeldCopy {
        companion object {
            val held = ArrayList<() -> Unit>()
            val sizes = ArrayList<Point>()
            @JvmStatic @Implementation fun request(window: android.view.Window, from: Rect?, to: android.graphics.Bitmap, done: android.view.PixelCopy.OnPixelCopyFinishedListener, handler: android.os.Handler) {
                sizes += Point(to.width, to.height)
                held += { done.onPixelCopyFinished(android.view.PixelCopy.SUCCESS) }
            }
        }
    }

    /** A copy that arrives after the held finger lifted and another went down begins no drag; one that arrives while
     * its touch is down begins it. The copy is made at most about a million pixels. */
    @Test @Config(shadows = [Journal::class, HeldCopy::class]) fun aCopyArrivingAfterItsTouchBeginsNoDrag() {
        HeldCopy.held.clear(); HeldCopy.sizes.clear()
        val activity = org.robolectric.Robolectric.buildActivity(android.app.Activity::class.java).setup().get()
        val view = View(activity).also { activity.setContentView(it) }
        shadowOf(android.os.Looper.getMainLooper()).idle()
        val clip = DragOut.clip(context, "Mount Fuji", "", emptyList())!!
        fun touch(action: Int, at: Long) = DragOut.touched(android.view.MotionEvent.obtain(at, at, action, 0f, 0f, 0))

        touch(android.view.MotionEvent.ACTION_DOWN, 1); DragOut.held(1)
        DragOut.start(activity, view, Rect(0, 0, view.width, view.height), Point(5, 5), clip)
        touch(android.view.MotionEvent.ACTION_UP, 1); touch(android.view.MotionEvent.ACTION_DOWN, 2)
        HeldCopy.held.removeAt(0)()
        assertEquals(listOf("drag: not begun: the finger held for it lifted before its shadow was drawn"), Journal.lines)

        DragOut.held(2)
        DragOut.start(activity, view, Rect(0, 0, 2560, 1600), Point(5, 5), clip)
        assertTrue(HeldCopy.sizes.last().let { it.x * it.y <= 1024 * 1024 })
        HeldCopy.held.removeAt(0)()
        assertEquals("one more line would be a drag not begun", 1, Journal.lines.size)

        // While the window has yet to hear an earlier drag's end, none begins.
        DragOut.start(activity, view, Rect(0, 0, 10, 10), Point(5, 5), clip) { true }
        HeldCopy.held.removeAt(0)()
        assertEquals(DragOut.UNHEARD, Journal.lines.last())
        touch(android.view.MotionEvent.ACTION_UP, 2)
    }

    /** A node's copy is its own size up to about a million pixels, and scaled down to that past it. */
    @Test fun aCopyIsBounded() {
        assertEquals(Point(300, 200), DragOut.copySize(300, 200))
        val big = DragOut.copySize(2560, 1600)
        assertTrue(big.x * big.y <= 1024 * 1024)
        assertEquals(2560.0 / 1600, big.x.toDouble() / big.y, 0.01)
    }

    /** A service's drag says it is one where the platform has the flag, API 32 and later. */
    @Test fun aServicesDragIsMarkedFromApi32() {
        val base = View.DRAG_FLAG_GLOBAL or View.DRAG_FLAG_GLOBAL_URI_READ
        assertEquals(base, DragOut.flags(false, 36))
        assertEquals(base, DragOut.flags(true, 31))
        assertEquals(base or View.DRAG_FLAG_ACCESSIBILITY_ACTION, DragOut.flags(true, 32))
    }

    /** The sheet's intent: text alone; one file as its stream with its type; files of one family as several streams
     * typed by the family; files of two families as any type; the text one `EXTRA_TEXT` whatever the files, and nowhere
     * else; each with the clip of the streams and their types, and the read grant the chooser passes on. */
    @Test fun theSheetIsGivenTheFilesAsStreamsWithAReadGrant() {
        val text = Requests.sendIntent("A post", "Read this", emptyList())
        assertEquals(listOf(Intent.ACTION_SEND, "text/plain", "Read this", "A post"),
            listOf(text.action, text.type, text.getStringExtra(Intent.EXTRA_TEXT), text.getStringExtra(Intent.EXTRA_SUBJECT)))
        assertNull(text.clipData)
        val a = Uri.parse("content://app.exact.files/a.png") to "image/png"
        val b = Uri.parse("content://app.exact.files/b.jpg") to "image/jpeg"
        val c = Uri.parse("content://app.exact.files/c.pdf") to "application/pdf"
        val one = Requests.sendIntent(null, "", listOf(a))
        val clipOf = { send: Intent -> (0 until send.clipData!!.description.mimeTypeCount).map { send.clipData!!.description.getMimeType(it) } }
        val captioned = Requests.sendIntent(null, "A photo", listOf(a))
        assertEquals(listOf("A photo", null), listOf(captioned.getStringExtra(Intent.EXTRA_TEXT), captioned.clipData!!.getItemAt(0).text))
        assertEquals(listOf("image/png"), clipOf(captioned))
        @Suppress("DEPRECATION")
        assertEquals(listOf(Intent.ACTION_SEND, "image/png", a.first), listOf(one.action, one.type, one.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)))
        assertFalse("no empty text beside a file", one.hasExtra(Intent.EXTRA_TEXT))
        val two = Requests.sendIntent(null, "Two photos", listOf(a, b))
        @Suppress("DEPRECATION")
        assertEquals(listOf(Intent.ACTION_SEND_MULTIPLE, "image/*", listOf(a.first, b.first), "Two photos"),
            listOf(two.action, two.type, two.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM), two.getStringExtra(Intent.EXTRA_TEXT)))
        assertEquals(listOf("image/png", "image/jpeg"), clipOf(two))
        assertEquals(listOf(null, null), (0 until two.clipData!!.itemCount).map { two.clipData!!.getItemAt(it).text })
        assertEquals(listOf("image/png"), clipOf(one))
        assertEquals("*/*", Requests.sendIntent(null, "", listOf(a, c)).type)
        for (send in listOf(one, captioned, two)) {
            assertTrue(send.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION != 0)
            assertEquals(send.getStreams(), (0 until send.clipData!!.itemCount).map { send.clipData!!.getItemAt(it).uri })
        }
    }

    @Suppress("DEPRECATION")
    private fun Intent.getStreams(): List<Uri> =
        getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM) ?: listOfNotNull(getParcelableExtra<Uri>(Intent.EXTRA_STREAM))
}
