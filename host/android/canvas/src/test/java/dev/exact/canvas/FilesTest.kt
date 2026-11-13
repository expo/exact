package dev.exact.canvas

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.nio.file.Paths

/** A module empties a directory of the app's following no symbolic link the app's own code made in it. */
class FilesTest {
    @get:Rule val dir = TemporaryFolder()

    /** `app:/tmp/photos` holding a photo and links to the app's `databases`, as a data module with `fs.write
     * app:/tmp` can make them; the photos module empties it at launch. */
    @Test fun aTreeIsRemovedWithoutEmptyingADirectoryALinkInItNames() {
        val databases = File(dir.root, "databases").apply { mkdirs() }
        File(databases, "app.db").writeText("kept")
        val photos = File(dir.root, "cache/temporary/photos").apply { mkdirs() }
        File(photos, "1.jpg").writeText("photo")
        File(photos, "nested").mkdirs()
        java.nio.file.Files.createSymbolicLink(File(photos, "out").toPath(), Paths.get("../../../databases"))
        java.nio.file.Files.createSymbolicLink(File(photos, "nested/db").toPath(), databases.toPath())
        java.nio.file.Files.createSymbolicLink(File(photos, "x").toPath(), databases.toPath())
        assertTrue(File(photos, "out/app.db").isFile)

        Files.removeTree(photos)

        assertFalse(photos.exists())
        assertEquals("kept", File(databases, "app.db").readText())
    }

    /** A link where the directory was is removed, and what it names is left. */
    @Test fun aLinkInPlaceOfTheTreeIsRemovedAlone() {
        val databases = File(dir.root, "databases").apply { mkdirs() }
        File(databases, "app.db").writeText("kept")
        val contacts = File(dir.root, "contacts")
        java.nio.file.Files.createSymbolicLink(contacts.toPath(), databases.toPath())

        Files.removeTree(contacts)

        assertFalse(java.nio.file.Files.exists(contacts.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS))
        assertEquals("kept", File(databases, "app.db").readText())
        Files.removeTree(File(dir.root, "absent"))
    }

    /** A file a module was answered and writes, made a link to a file elsewhere after the answer: every way a module
     * writes refuses it and the file it names is unchanged (a photo copied to `app:/tmp/photos/1.jpg`, the agent's photo
     * library's `agent-7.jpg`, a transcript's appended lines), and a reader does not take it for a regular file. */
    @Test fun aWriteRefusesALinkPlantedAfterTheAnswerAndLeavesWhatItNames() {
        val databases = File(dir.root, "databases").apply { mkdirs() }
        File(databases, "app.db").writeText("kept")
        val photos = File(dir.root, "cache/temporary/photos").apply { mkdirs() }
        val answered = File(photos, "1.jpg")
        assertFalse(answered.exists())
        java.nio.file.Files.createSymbolicLink(answered.toPath(), File(databases, "app.db").toPath())

        assertThrows(java.io.IOException::class.java) { Files.write(answered, "jpeg".toByteArray()) }
        assertThrows(java.io.IOException::class.java) { Files.write(answered, "line".toByteArray(), append = true) }
        assertThrows(java.io.IOException::class.java) { Files.output(answered).use { "jpeg".byteInputStream().copyTo(it) } }
        assertFalse(Files.regular(answered))
        assertTrue(answered.isFile)
        assertEquals("kept", File(databases, "app.db").readText())

        val fresh = File(photos, "2.jpg")
        Files.write(fresh, "jpeg".toByteArray())
        Files.write(fresh, "!".toByteArray(), append = true)
        assertEquals("jpeg!", fresh.readText())
        Files.output(fresh).use { "new".byteInputStream().copyTo(it) }
        assertEquals("new", fresh.readText())
        assertTrue(Files.regular(fresh))
    }

    /** The camera app's photo (`takePhotoWithCameraApp`): the one file the app hands out for writing is made empty and
     * writable until the app takes it back; any other file stays read-only. */
    @Test fun onlyAFileHandedOutForWritingIsWritableAndOnlyUntilItIsTakenBack() {
        val photo = File(dir.root, "cache/exact.camera-app/photo.jpg")
        val other = File(dir.root, "cache/temporary/camera/2.jpg").apply { parentFile!!.mkdirs(); writeText("kept") }
        assertFalse(Files.writes(photo))

        assertTrue(Files.openForWrite(photo))
        assertTrue(photo.isFile)
        assertEquals(0L, photo.length())
        assertTrue(Files.writes(photo))
        assertFalse(Files.writes(other))

        Files.endWrite(photo)
        assertFalse(Files.writes(photo))
        assertEquals("kept", other.readText())
    }

    /** A link planted where the photo goes is not handed out, and what it names is unchanged. */
    @Test fun aLinkWhereThePhotoGoesIsNotHandedOutForWriting() {
        val databases = File(dir.root, "databases").apply { mkdirs() }
        File(databases, "app.db").writeText("kept")
        val photo = File(dir.root, "cache/exact.camera-app/photo.jpg").apply { parentFile!!.mkdirs() }
        java.nio.file.Files.createSymbolicLink(photo.toPath(), File(databases, "app.db").toPath())

        assertFalse(Files.openForWrite(photo))
        assertFalse(Files.writes(File(databases, "app.db")))
        assertEquals("kept", File(databases, "app.db").readText())
    }
}
