package dev.exact.canvas

import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.io.File
import java.io.FileNotFoundException
import java.io.FileOutputStream

/** The provider's URIs ([Files]): each names its source and reads it as it is at the open, an `app:/` file through the
 * reader's confined read, which `canvas::picker` tests on a real file system; only a capture's opens for writing, and
 * only while it lasts. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, shadows = [FilesUriTest.Reads::class])
class FilesUriTest {
    /** The reader's confined read, standing in: a regular file under [root], and each path it was asked for. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Reads {
        companion object {
            lateinit var root: File
            val asked = ArrayList<String>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun assetRoot(): String? = null
            @JvmStatic @Implementation fun openApp(path: String): ParcelFileDescriptor? {
                asked += path
                val file = File(root, path.removePrefix("app:/")).takeIf(Files::regular) ?: return null
                return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
            }
        }
    }

    private val application = RuntimeEnvironment.getApplication()
    private val provider = Robolectric.setupContentProvider(Files::class.java, "${application.packageName}.exact.files")

    @Before fun roots() {
        Reads.root = File(application.cacheDir, "roots").apply { mkdirs() }
        Reads.asked.clear()
        shadowOf(android.webkit.MimeTypeMap.getSingleton()).addExtensionMimeTypeMapping("pdf", "application/pdf")
    }

    private fun read(uri: android.net.Uri) = provider.openFile(uri, "r").use { ParcelFileDescriptor.AutoCloseInputStream(it).readBytes().decodeToString() }

    /** A URI names the `app:/` path, never where it is, and each open reads what is there then: a file written over or
     * removed after the URI was made is read as it is, or not at all. */
    @Test fun anAppFilesUriReadsItsPathAtEachOpen() {
        val file = File(Reads.root, "data/notes/a b.pdf").apply { parentFile!!.mkdirs(); writeText("first") }
        val uri = Files.uri(application, "app:/data/notes/a b.pdf")!!
        assertEquals(listOf("app", "data", "notes", "a b.pdf"), uri.pathSegments)
        assertFalse(uri.toString().contains(Reads.root.path))

        assertEquals("first", read(uri))
        file.writeText("second, longer")
        assertEquals("second, longer", read(uri))
        provider.query(uri, null, null, null, null)!!.use { row ->
            row.moveToFirst()
            assertEquals("a b.pdf", row.getString(row.getColumnIndexOrThrow(OpenableColumns.DISPLAY_NAME)))
            assertEquals(14L, row.getLong(row.getColumnIndexOrThrow(OpenableColumns.SIZE)))
        }
        assertEquals("application/pdf", provider.getType(uri))
        file.delete()
        assertThrows(FileNotFoundException::class.java) { provider.openFile(uri, "r") }
        assertNull("no URI for a file the app does not have", Files.uri(application, "app:/data/notes/a b.pdf"))
        assertTrue(Reads.asked.all { it == "app:/data/notes/a b.pdf" })
    }

    /** No mode but reading opens an app's or a shipped file, and a URI of another shape names nothing. */
    @Test fun anAppOrShippedFileOpensForReadingOnly() {
        File(Reads.root, "data/a.pdf").apply { parentFile!!.mkdirs(); writeText("kept") }
        val uri = Files.uri(application, "app:/data/a.pdf")!!
        for (mode in listOf("w", "wt", "wa", "rw", "rwt")) assertThrows(SecurityException::class.java) { provider.openFile(uri, mode) }
        for (path in listOf("elsewhere/data/a.pdf", "app/home/a.pdf", "app/data%2Fa.pdf", "app/data/./a.pdf", "app/data/a.pdf/", "app//data/a.pdf")) {
            assertThrows(path, FileNotFoundException::class.java) { provider.openFile(uri.buildUpon().encodedPath(path).build(), "r") }
        }
        assertEquals("kept", read(uri))
    }

    /** A shipped name reads under the running generation's assets, and one that leaves them names nothing. */
    @Test fun aShippedFileIsReadUnderTheAssets() {
        File(application.filesDir, "app/assets/fuji.pdf").apply { parentFile!!.mkdirs(); writeText("shipped") }
        File(application.filesDir, "outside.pdf").writeText("not shipped")
        val uri = Files.uri(application, "assets/fuji.pdf")!!
        assertEquals(listOf("shipped", "assets", "fuji.pdf"), uri.pathSegments)
        assertEquals("shipped", read(uri))
        assertNull(Files.uri(application, "../outside.pdf"))
        assertNull(Files.uri(application, "file:///etc/hosts"))
        assertNull("a NUL names nothing, and throws nothing", Files.uri(application, "assets/fuji\u0000.pdf"))
        assertThrows(FileNotFoundException::class.java) { provider.openFile(uri.buildUpon().path("shipped/../outside.pdf").build(), "r") }
        File(application.filesDir, "app/plan.json").writeText("not a drag's")
        assertNull("only `assets/` is shipped to other apps", Files.uri(application, "plan.json"))
        java.nio.file.Files.createSymbolicLink(File(application.filesDir, "app/assets/plan.pdf").toPath(), File(application.filesDir, "app/plan.json").toPath())
        assertNull("nor what a link in it names", Files.uri(application, "assets/plan.pdf"))
        // Nor anything where `assets` is itself a link.
        val shipped = File(application.filesDir, "app/assets")
        val elsewhere = File(application.filesDir, "elsewhere").apply { mkdirs(); File(this, "fuji.pdf").writeText("elsewhere") }
        shipped.renameTo(File(application.filesDir, "app/assets-moved"))
        java.nio.file.Files.createSymbolicLink(shipped.toPath(), elsewhere.toPath())
        assertNull(Files.uri(application, "assets/fuji.pdf"))
        assertThrows(FileNotFoundException::class.java) { provider.openFile(uri.buildUpon().path("shipped/plan.json").build(), "r") }
    }

    /** The camera app's file: new and empty in a directory of its own, written through its URI in any mode while its
     * capture lasts, never made again by a mode once gone, and named by nothing after its capture ends. */
    @Test fun aCaptureIsWritableUntilItEnds() {
        val capture = Files.capture(application, "photo.jpg")!!
        assertEquals(0L, capture.file.length())
        assertEquals(Files.captures(application), capture.file.parentFile!!.parentFile)
        provider.openFile(capture.uri, "w").use { FileOutputStream(it.fileDescriptor).use { out -> out.write("jpeg".toByteArray()) } }
        provider.openFile(capture.uri, "wa").use { FileOutputStream(it.fileDescriptor).use { out -> out.write("!".toByteArray()) } }
        assertEquals("jpeg!", capture.file.readText())
        assertEquals("jpeg!", read(capture.uri))
        // Android checks the write grant only for a mode with `w`, so `rt`, which would empty it, opens nothing.
        assertThrows(FileNotFoundException::class.java) { provider.openFile(capture.uri, "rt") }
        assertEquals("jpeg!", capture.file.readText())

        val next = Files.capture(application, "photo.jpg")!!
        assertNotEquals(capture.uri, next.uri)
        Files.endCapture(application, capture)
        assertFalse(capture.file.parentFile!!.exists())
        assertThrows(FileNotFoundException::class.java) { provider.openFile(capture.uri, "w") }
        assertFalse("a mode makes no file", capture.file.exists())
        next.file.delete()
        assertThrows(FileNotFoundException::class.java) { provider.openFile(next.uri, "rw") }
        assertFalse(next.file.exists())
        val outside = next.uri.buildUpon().path("capture/../photo.jpg").build()
        assertThrows(FileNotFoundException::class.java) { provider.openFile(outside, "w") }
    }
}
