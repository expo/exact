package exact.camera

import android.content.Context
import android.graphics.Bitmap
import android.media.MediaExtractor
import android.media.MediaFormat
import android.media.MediaMetadataRetriever
import java.io.File
import java.security.MessageDigest

internal class AgentClip(val frames: List<Frame>, val duration: Double, val sha256: String) : dev.exact.canvas.CameraClip {
    data class Frame(val at: Double, val bitmap: Bitmap)

    override fun sample(trackElapsed: Double): dev.exact.canvas.CameraFrame {
        val at = trackElapsed.coerceAtLeast(0.0) % duration
        var lo = 0
        var hi = frames.size
        while (lo + 1 < hi) {
            val mid = (lo + hi) / 2
            if (frames[mid].at <= at) lo = mid else hi = mid
        }
        val end = frames.getOrNull(lo + 1)?.at ?: duration
        return dev.exact.canvas.CameraFrame(frames[lo].bitmap, end - at)
    }

    companion object {
        fun read(context: Context, path: String): AgentClip {
            require(!path.startsWith('/') && ':' !in path && ".." !in path.split('/')) { "camera.clip must name an app asset" }
            val bytes = context.assets.open("app/$path").use { it.readBytes() }
            val sha = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
            // APK assets may be compressed. Both platform readers need a seekable file.
            val file = File.createTempFile("camera-clip-", ".mp4", context.cacheDir)
            val extractor = MediaExtractor()
            val reader = MediaMetadataRetriever()
            try {
                file.writeBytes(bytes)
                extractor.setDataSource(file.path)
                val track = (0 until extractor.trackCount).firstOrNull {
                    extractor.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("video/") == true
                } ?: error("camera.clip has no video track")
                val duration = extractor.getTrackFormat(track).getLong(MediaFormat.KEY_DURATION) / 1000.0
                extractor.selectTrack(track)
                val times = ArrayList<Double>()
                while (extractor.sampleTime >= 0) {
                    times += extractor.sampleTime / 1000.0
                    if (!extractor.advance()) break
                }
                times.sort()
                require(times.isNotEmpty() && duration.isFinite() && duration > 0) { "camera.clip has no timed frames" }
                reader.setDataSource(file.path)
                val bitmaps = reader.getFramesAtIndex(0, times.size)
                require(bitmaps.size == times.size) { "camera.clip: the decoder omitted a frame" }
                return AgentClip(times.mapIndexed { i, at -> Frame(at - times[0], bitmaps[i]) }, duration, sha)
            } finally {
                reader.release()
                extractor.release()
                file.delete()
            }
        }
    }
}
