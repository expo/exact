package exact.recording

import android.content.Context
import android.media.MediaCodec
import android.media.MediaFormat
import java.io.File
import java.io.IOException
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

    /** Stopping before any AAC frame was emitted fails finish() and clears the ledger once the empty part is deleted. */
    @Test fun finishWithZeroFramesThrowsAndClearsLedger() {
        val file = File(dir, "empty.m4a")
        val w = writer(file)
        assertThrows(java.io.IOException::class.java) { w.finish() }
        assertFalse(file.exists())
        assertFalse(w.part.exists())
        val owners = app.getSharedPreferences("exact.recording.parts", Context.MODE_PRIVATE).all
        assertFalse("empty part must not remain in ledger", file.path in owners)
    }

    /** A rename failure fails finish(), preserves the part and its ledger entry, and leaves recovery intact. */
    @Test fun finishWhenRenameFailsThrowsAndKeepsPartInLedger() {
        val file = File(dir, "blocked.m4a")
        val w = writer(file)
        w.write(ByteBuffer.wrap(byteArrayOf(1, 2)), 2)
        // A non-empty directory at the destination makes File.renameTo return false on POSIX.
        assertTrue(file.mkdirs())
        File(file, "blocker").writeText("keep")
        assertThrows(java.io.IOException::class.java) { w.finish() }
        assertTrue("part must survive a failed rename", w.part.exists())
        assertFalse("temp mux file is cleaned up", File(file.path + ".mp4.tmp").exists())
        val prefs = app.getSharedPreferences("exact.recording.parts", Context.MODE_PRIVATE)
        assertTrue("ledger keeps the part for recovery", file.path in prefs.all)

        // Once the destination is clear and another process recovers, the saved part becomes an MP4.
        file.deleteRecursively()
        prefs.edit().putString(file.path, "gone").commit()
        recoverParts(app) { lines += it }
        assertEquals(listOf("recording: recovered ${file.path}"), lines)
        assertTrue(file.isFile)
        assertFalse(w.part.exists())
    }

    /** When writeSampleData or stop fails and MediaMuxer.release() throws IllegalStateException from its started
     * state, mux() still deletes `<file>.mp4.tmp` and propagates the original failure unmasked. */
    @Test fun muxFailureDeletesTempAndKeepsOriginalErrorWhenReleaseThrows() {
        for (failIn in listOf("write", "stop")) {
            val file = File(dir, "fail-$failIn.m4a")
            val w = writer(file)
            w.write(ByteBuffer.wrap(byteArrayOf(1, 2)), 2)
            w.close()
            var released = false
            val open: (String) -> SampleMuxer = { path ->
                File(path).writeBytes(byteArrayOf(0, 1))
                object : SampleMuxer {
                    override fun addTrack(format: MediaFormat) = 0
                    override fun start() {}
                    override fun writeSampleData(trackIndex: Int, byteBuf: ByteBuffer, bufferInfo: MediaCodec.BufferInfo) {
                        if (failIn == "write") throw IOException("write failed")
                    }
                    override fun stop() {
                        if (failIn == "stop") throw IllegalStateException("stop failed")
                    }
                    override fun release() {
                        released = true
                        throw IllegalStateException("muxer still started")
                    }
                }
            }
            if (failIn == "write") {
                val error = assertThrows(IOException::class.java) { mux(w.part, file, open) }
                assertEquals("write failed", error.message)
            } else {
                val error = assertThrows(IllegalStateException::class.java) { mux(w.part, file, open) }
                assertEquals("stop failed", error.message)
            }
            assertTrue("release was called for $failIn failure", released)
            assertFalse("temp mux file is deleted on $failIn failure", File(file.path + ".mp4.tmp").exists())
            assertTrue("part is preserved on $failIn failure", w.part.exists())
            assertFalse("target MP4 is not created on $failIn failure", file.exists())
        }
    }
}
