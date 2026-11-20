package exact.recording

import android.graphics.Bitmap
import android.media.Image
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.media.MediaMetadataRetriever
import android.media.MediaMuxer
import dev.exact.canvas.CameraClip
import dev.exact.canvas.CameraFrame
import dev.exact.canvas.TrackEntry
import dev.exact.canvas.Tracks
import java.io.File
import java.nio.ByteBuffer
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import kotlin.math.roundToLong

/** The input and its track time over each uninterrupted part of the take. A clock jump costs no extra callbacks. */
internal class AgentVideo(private val tracks: Tracks, private val entry: TrackEntry, private val now: () -> Double) {
    data class Part(val clip: CameraClip?, val picture: Bitmap?, val from: Long, val length: Long)
    private val parts = ArrayList<Part>()
    private val origin = entry.startedAt ?: now()
    private var begin: Double? = now()
    private var clip = tracks.cameraClip
    private var picture = entry.picture
    private val unobserve = tracks.observeClip { next ->
        val time = now()
        if (begin != null) { cut(time); begin = time }
        clip = next
        picture = entry.picture
    }

    private fun cut(time: Double) {
        val start = begin ?: return
        val length = ((time - start) * 1000).roundToLong()
        if (length > 0) parts += Part(clip, picture, ((start - origin) * 1000).roundToLong(), length)
        begin = null
    }
    fun pause(time: Double) = cut(time)
    fun resume(time: Double) { begin = time; picture = entry.picture }
    fun finish(time: Double): List<Part>? {
        cut(time)
        unobserve()
        return parts.takeIf { clip != null || it.any { part -> part.clip != null } }
    }
}

/** Loop the no-clip fixture when it occupies only part of a take with a substituted camera. */
private fun fallback(directory: File, bytes: ByteArray): CameraClip {
    val source = File.createTempFile("recording-default-", ".mp4", directory)
    val reader = MediaMetadataRetriever()
    try {
        source.writeBytes(bytes)
        reader.setDataSource(source.path)
        val frames = reader.getFramesAtIndex(0, 3)
        require(frames.size == 3)
        return CameraClip { elapsed ->
            val at = elapsed.coerceAtLeast(0.0) % 300
            CameraFrame(frames[(at / 100).toInt()], 100.0 - at % 100)
        }
    } finally { reader.release(); source.delete() }
}

/** Write each source frame at its presentation time, including a partial first or last frame after a pause. */
internal fun writeAgentVideo(file: File, parts: List<AgentVideo.Part>, defaultBytes: () -> ByteArray) {
    val first = parts.firstNotNullOfOrNull { part -> part.clip?.sample(part.from / 1000.0)?.bitmap }
        ?: error("the recorder stopped before the camera supplied a frame")
    val directory = requireNotNull(file.parentFile)
    directory.mkdirs()
    val pending = File.createTempFile(".recording-", ".mp4", directory)
    val default by lazy { fallback(directory, defaultBytes()) }
    try {
        val width = (first.width + 1) and -2
        val height = (first.height + 1) and -2
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Flexible)
            setInteger(MediaFormat.KEY_BIT_RATE, (width.toLong() * height * 6).coerceIn(128000, 40000000).toInt())
            setInteger(MediaFormat.KEY_FRAME_RATE, 30)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
            setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT601_NTSC)
            setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED)
        }
        val encoder = MediaCodecList(MediaCodecList.REGULAR_CODECS).findEncoderForFormat(format)
            ?: error("no AVC encoder accepts the camera's ${width}×${height} frames")
        val codec = MediaCodec.createByCodecName(encoder)
        try {
            codec.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            codec.start()
            val muxer = MediaMuxer(pending.path, MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4)
            try {
                val writer = FrameWriter(codec, muxer, width, height)
                var time = 0L
                for (part in parts) {
                    var elapsed = 0L
                    while (elapsed < part.length) {
                        val sample = part.clip?.sample((part.from + elapsed) / 1000.0)
                            ?: part.picture?.let { CameraFrame(it, Double.POSITIVE_INFINITY) }
                            ?: default.sample((part.from + elapsed) / 1000.0)
                        writer.write(sample.bitmap, time + elapsed)
                        elapsed += (sample.remaining * 1000).roundToLong().coerceIn(1L, part.length - elapsed)
                    }
                    time += part.length
                }
                writer.finish(time)
                muxer.stop()
            } finally { muxer.release() }
        } finally { codec.release() }
        Files.move(pending.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
    } finally { pending.delete() }
}

private class FrameWriter(private val codec: MediaCodec, private val muxer: MediaMuxer, val width: Int, val height: Int) {
    private val info = MediaCodec.BufferInfo()
    private val pixels = IntArray(width * height)
    private var previous: Bitmap? = null
    private var track = -1
    private fun drain(wait: Long): Boolean {
        val index = codec.dequeueOutputBuffer(info, wait)
        if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) { track = muxer.addTrack(codec.outputFormat); muxer.start() }
        if (index < 0) return false
        try {
            if (info.size > 0 && info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0)
                muxer.writeSampleData(track, requireNotNull(codec.getOutputBuffer(index)), info)
        } finally { codec.releaseOutputBuffer(index, false) }
        return info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
    }
    private fun input(): Int {
        val until = android.os.SystemClock.elapsedRealtime() + 10000
        while (android.os.SystemClock.elapsedRealtime() < until) {
            val index = codec.dequeueInputBuffer(10000)
            if (index >= 0) return index
            drain(0)
        }
        error("the AVC encoder stopped accepting frames")
    }

    fun write(frame: Bitmap, time: Long) {
        val index = input()
        if (previous !== frame) {
            val scaled = Bitmap.createScaledBitmap(frame, width, height, true)
            scaled.getPixels(pixels, 0, width, 0, 0, width, height)
            if (scaled !== frame) scaled.recycle()
            previous = frame
        }
        val image = requireNotNull(codec.getInputImage(index)) { "the AVC encoder did not expose its YUV input" }
        image.use { fill(it) }
        codec.queueInputBuffer(index, 0, width * height * 3 / 2, time, 0)
        drain(0)
    }

    private fun fill(image: Image) {
        val planes = image.planes
        val buffers = planes.map { it.buffer.slice() }
        val rows = planes.map { it.rowStride }
        val steps = planes.map { it.pixelStride }
        fun put(plane: Int, x: Int, y: Int, value: Int) {
            buffers[plane].put(y * rows[plane] + x * steps[plane], value.toByte())
        }
        for (y in 0 until height step 2) for (x in 0 until width step 2) {
            var red = 0; var green = 0; var blue = 0
            for (dy in 0..1) for (dx in 0..1) {
                val rgb = pixels[(y + dy) * width + x + dx]
                val r = rgb shr 16 and 255
                val g = rgb shr 8 and 255
                val b = rgb and 255
                put(0, x + dx, y + dy, 16 + ((66*r + 129*g + 25*b + 128) shr 8))
                red += r; green += g; blue += b
            }
            put(1, x / 2, y / 2, 128 + ((-38*red - 74*green + 112*blue + 512) shr 10))
            put(2, x / 2, y / 2, 128 + ((112*red - 94*green - 18*blue + 512) shr 10))
        }
    }

    fun finish(time: Long) {
        codec.queueInputBuffer(input(), 0, 0, time, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
        val until = android.os.SystemClock.elapsedRealtime() + 10000
        while (!drain(10000)) check(android.os.SystemClock.elapsedRealtime() < until) { "the AVC encoder did not finish" }
        // MediaMuxer uses an empty EOS sample to set the last MP4 sample's duration.
        info.set(0, 0, time, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
        muxer.writeSampleData(track, ByteBuffer.allocate(0), info)
    }
}
