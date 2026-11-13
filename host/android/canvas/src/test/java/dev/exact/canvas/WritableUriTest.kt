package dev.exact.canvas

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.io.File

/** A file handed out for another app to write ([Files.writableUri]) is one of the app's own: any other is refused
 * before anything empties it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class WritableUriTest {
    @get:Rule val elsewhere = TemporaryFolder()
    private val application = RuntimeEnvironment.getApplication()

    @Test fun aFileOutsideTheAppsDataIsRefusedAndKept() {
        val outside = elsewhere.newFile("photo.jpg").apply { writeText("kept") }
        assertFalse(outside.canonicalPath.startsWith(application.dataDir.canonicalPath))
        assertNull(Files.writableUri(application, outside))
        assertEquals("kept", outside.readText())
        assertFalse(Files.writes(outside))
    }

    @Test fun aFileOfTheAppsIsEmptiedAndHandedOut() {
        val own = File(application.cacheDir, "exact.camera-app/photo.jpg").apply { parentFile!!.mkdirs(); writeText("old") }
        val uri = Files.writableUri(application, own)
        assertNotNull(uri)
        assertEquals(0L, own.length())
        assertTrue(Files.writes(own))
        Files.endWrite(own)
    }
}
