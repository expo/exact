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
 * clip's items and what it leaves out, the drag's flags by API level, and the sheet's intent. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, shadows = [DragOutTest.Journal::class])
class DragOutTest {
    private val context = RuntimeEnvironment.getApplication()
    private val assets = File(context.filesDir, "app").apply { File(this, "assets").mkdirs() }

    @Before fun begin() { Journal.lines.clear() }

    /** exact2's journal, as the reader writes to it with no owner. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Journal {
        companion object {
            val lines = ArrayList<String>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun journal(line: String) { lines += line }
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
        val saved = File(context.filesDir, "saved.txt").apply { writeText("saved") }
        val appFile = { src: String -> saved.takeIf { src == "app:/data/saved.txt" } }

        val clip = DragOut.clip(context, "Mount Fuji", "https://example.com/1016", listOf("assets/fuji.png", "app:/data/saved.txt"), assets, appFile)!!

        assertEquals(4, clip.itemCount)
        val authority = "${context.packageName}.exact.files"
        assertEquals("content", clip.getItemAt(0).uri.scheme)
        assertEquals(authority, clip.getItemAt(0).uri.authority)
        assertEquals(File(assets, "assets/fuji.png").canonicalPath, clip.getItemAt(0).uri.path)
        assertEquals(saved.canonicalPath, clip.getItemAt(1).uri.path)
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
     * nothing else, there is no drag, and the label is a carried file's or the text. */
    @Test fun aFileTheAppDoesNotHaveIsLeftOut() {
        File(context.filesDir, "outside.txt").writeText("not shipped")
        val none = { _: String -> null }
        val refused = listOf("assets/missing.png", "../outside.txt", "file:///etc/hosts", "app:/data/gone")
        assertNull(DragOut.clip(context, "", "", refused, assets, none))
        assertEquals(refused.map { "drag: $it could not be read: the app has no such file, so the drag leaves it out" }, Journal.lines)
        val clip = DragOut.clip(context, "kept", "", listOf("assets/missing.png"), assets, none)!!
        assertEquals(listOf("kept", "kept"), listOf(clip.getItemAt(0).text.toString(), clip.description.label.toString()))
    }

    /** A service's drag says it is one where the platform has the flag, API 32 and later. */
    @Test fun aServicesDragIsMarkedFromApi32() {
        val base = View.DRAG_FLAG_GLOBAL or View.DRAG_FLAG_GLOBAL_URI_READ
        assertEquals(base, DragOut.flags(false, 36))
        assertEquals(base, DragOut.flags(true, 31))
        assertEquals(base or View.DRAG_FLAG_ACCESSIBILITY_ACTION, DragOut.flags(true, 32))
    }

    /** The sheet's intent: text alone; one file as its stream with its type; files of one family as several streams
     * typed by the family; files of two families as any type; each with the clip and the read grant the chooser passes
     * on. */
    @Test fun theSheetIsGivenTheFilesAsStreamsWithAReadGrant() {
        val text = Requests.sendIntent("A post", "Read this", emptyList())
        assertEquals(listOf(Intent.ACTION_SEND, "text/plain", "Read this", "A post"),
            listOf(text.action, text.type, text.getStringExtra(Intent.EXTRA_TEXT), text.getStringExtra(Intent.EXTRA_SUBJECT)))
        assertNull(text.clipData)
        val a = Uri.parse("content://app.exact.files/a.png") to "image/png"
        val b = Uri.parse("content://app.exact.files/b.jpg") to "image/jpeg"
        val c = Uri.parse("content://app.exact.files/c.pdf") to "application/pdf"
        val one = Requests.sendIntent(null, "", listOf(a))
        @Suppress("DEPRECATION")
        assertEquals(listOf(Intent.ACTION_SEND, "image/png", a.first), listOf(one.action, one.type, one.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)))
        assertFalse("no empty text beside a file", one.hasExtra(Intent.EXTRA_TEXT))
        val two = Requests.sendIntent(null, "", listOf(a, b))
        @Suppress("DEPRECATION")
        assertEquals(listOf(Intent.ACTION_SEND_MULTIPLE, "image/*", listOf(a.first, b.first)),
            listOf(two.action, two.type, two.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)))
        assertEquals("*/*", Requests.sendIntent(null, "", listOf(a, c)).type)
        for (send in listOf(one, two)) {
            assertTrue(send.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION != 0)
            assertEquals(send.getStreams(), (0 until send.clipData!!.itemCount).map { send.clipData!!.getItemAt(it).uri })
        }
    }

    @Suppress("DEPRECATION")
    private fun Intent.getStreams(): List<Uri> =
        getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM) ?: listOfNotNull(getParcelableExtra<Uri>(Intent.EXTRA_STREAM))
}
