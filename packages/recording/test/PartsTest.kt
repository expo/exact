package exact.recording

import android.content.Context
import android.media.MediaFormat
import java.io.File
import java.nio.ByteBuffer
import org.junit.After
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** A take that survives its process (LLP 1133.033 D5): recovery makes MP4s of the parts a dead process left, and never
 * touches one this process is still writing. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class PartsTest {
    private val app = RuntimeEnvironment.getApplication()
    private val dir = File(app.filesDir, "takes")
    private val lines = ArrayList<String>()

    @After fun tearDown() {
        dir.deleteRecursively()
        app.getSharedPreferences("exact.recording.parts", Context.MODE_PRIVATE).edit().clear().commit()
    }

    /** AAC-LC at 44.1 kHz, mono: the encoder's `csd-0`. */
    private fun writer(file: File) = PartWriter(app, file).also {
        it.configure(MediaFormat().apply { setByteBuffer("csd-0", ByteBuffer.wrap(byteArrayOf(0x12, 0x08))) })
    }

    /** Recovery that runs late, after this process's first take began, leaves that take to finish whole. */
    @Test fun recoveryLeavesATakeUnderWay() {
        val file = File(dir, "live.m4a")
        val w = writer(file)
        w.write(ByteBuffer.wrap(byteArrayOf(1, 2)), 2)
        recoverParts(app) { lines += it }
        assertEquals(emptyList<String>(), lines)
        assertTrue("the take's part is still there", w.part.exists())
        assertFalse(file.exists())

        w.write(ByteBuffer.wrap(byteArrayOf(3, 4)), 2)
        w.finish()
        assertTrue(file.exists())
        assertFalse(w.part.exists())
    }

    /** A part a process that died left is made an MP4 while this process records another take. */
    @Test fun recoveryMakesADeadProcessesPartAnMp4() {
        val left = File(dir, "left.m4a")
        val dead = writer(left)
        dead.write(ByteBuffer.wrap(byteArrayOf(1, 2)), 2)
        dead.close()
        // The process that wrote it is gone: the ledger names another owner.
        app.getSharedPreferences("exact.recording.parts", Context.MODE_PRIVATE).edit().putString(left.path, "gone").commit()
        val live = writer(File(dir, "live.m4a"))
        live.write(ByteBuffer.wrap(byteArrayOf(3, 4)), 2)

        recoverParts(app) { lines += it }
        assertEquals(listOf("recording: recovered ${left.path}"), lines)
        assertTrue(left.exists())
        assertFalse(File(left.path + ".part").exists())
        assertTrue(live.part.exists())
        live.finish()
    }
}
