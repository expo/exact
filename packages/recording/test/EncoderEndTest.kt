package exact.recording

import android.media.MediaCodec
import android.media.MediaFormat
import android.media.MediaMuxer
import java.io.File
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.util.ReflectionHelpers

/** A stop ends the take's file whatever the encoder holds: with every input buffer queued and none drained, the end
 * of the stream waits for the drain to free one, and the stop returns. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class EncoderEndTest {
    @Test fun aStopWithEveryInputBufferFullStillEndsTheStream() {
        val file = File(RuntimeEnvironment.getApplication().filesDir, "takes/full.m4a")
        val form = Form(MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4, MediaFormat.MIMETYPE_AUDIO_AAC, null)
        val encoder = RecordingModule.Encoder(file, form, 48000, 0.0, null)
        val codec = ReflectionHelpers.getField<MediaCodec>(encoder, "codec")
        var queued = 0
        while (true) {
            val index = codec.dequeueInputBuffer(0)
            if (index < 0) break
            codec.queueInputBuffer(index, 0, 2, queued * 1000L, 0)
            queued++
        }
        assertTrue("the case under test: no input buffer is free", queued > 0)

        var failure: Throwable? = null
        val stop = Thread { try { encoder.finish() } catch (e: Throwable) { failure = e } }.apply { isDaemon = true; start() }
        stop.join(10_000)
        assertFalse("the stop returned", stop.isAlive)
        assertNull(failure)
    }
}
