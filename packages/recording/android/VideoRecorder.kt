package exact.recording

import android.media.MediaMetadataRetriever
import androidx.camera.video.FileOutputOptions
import androidx.camera.video.Recorder
import androidx.camera.video.Recording
import androidx.camera.video.VideoCapture
import androidx.camera.video.VideoRecordEvent
import androidx.core.content.ContextCompat
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.TrackConsumer
import dev.exact.canvas.TrackError
import dev.exact.canvas.VideoSource
import org.json.JSONObject
import java.io.File

internal interface MediaTake {
    val id: String
    val state: String
    fun info(): JSONObject
    fun pause(why: String)
    fun resume()
    fun finish(why: String, failure: String?, done: (JSONObject) -> Unit)
}

private class VideoSlot { var busy = false }

/** One output on the track's camera session. CameraX decides whether it fits beside preview and analysis. */
internal class VideoTake(
    override val id: String, private val path: String, private val file: File,
    private val context: ExtensionContext, private val maxDuration: Double, private val changed: () -> Unit,
    private val track: dev.exact.canvas.TrackEntry? = null
) : MediaTake, TrackConsumer {
    override var state = "recording"
        private set
    var leave: (() -> Unit)? = null
    private var reason: String? = null
    private var failure: String? = null
    private var failureCode = "UnknownError"
    private var answer: JSONObject? = null
    private var result: JSONObject? = null
    private var since = context.now()
    private var recorded = 0.0
    private var limit: (() -> Unit)? = null
    private var source: VideoSource? = null
    private var slot: VideoSlot? = null
    private var output: VideoCapture<Recorder>? = null
    private var recording: Recording? = null
    private val waiting = ArrayList<(JSONObject) -> Unit>()
    private var finishing = false
    private var agentVideo: AgentVideo? = null
    private var encodedClip = false

    private val duration: Double get() = recorded + if (state == "recording") maxOf(0.0, context.now() - since) else 0.0

    fun arm() {
        if (context.agent && agentVideo == null && track != null) agentVideo = AgentVideo(context.tracks, track, context::now)
        limit?.invoke()
        if (context.agent && maxDuration > 0 && state == "recording") {
            limit = context.after(maxOf(0.0, maxDuration - duration)) { finish("max-duration", null) {} }
        }
    }

    override fun info(): JSONObject = JSONObject().put("id", id).put("state", state).put("mimeType", "video/mp4")
        .put("path", path).put("duration", duration).also {
            reason?.let { value -> it.put("reason", value) }
            failure?.let { value -> it.put("error", value) }
            result?.let { value -> it.put("result", value) }
        }

    override fun liveVideo(source: VideoSource?) {
        if (context.agent || source == null || state == "inactive") return
        try {
            val slot = source.shared("recording.video") { VideoSlot() }
            if (slot.busy && slot !== this.slot) throw TrackError("NotReadableError")
            val capture = output ?: VideoCapture.withOutput(Recorder.Builder().build())
            // A released camera clears its outputs and shared objects. Reattach the persistent take on reacquisition.
            // attach rolls back a rejected set, preserving the preview and the package's analysis.
            if (!source.attach(capture)) throw TrackError("NotSupportedError")
            this.slot?.busy = false
            slot.busy = true
            this.slot = slot
            this.source = source
            output = capture
            if (recording != null) return
            file.parentFile?.mkdirs()
            file.delete()
            val options = FileOutputOptions.Builder(file)
            if (maxDuration > 0) options.setDurationLimitMillis(maxDuration.toLong().coerceAtLeast(1))
            recording = capture.output.prepareRecording(context.application, options.build())
                // Analysis joining or leaving rebinds every use case on this session.
                .asPersistentRecording()
                .start(ContextCompat.getMainExecutor(context.application)) { event ->
                    if (event is VideoRecordEvent.Finalize) {
                        recorded = event.recordingStats.recordedDurationNanos / 1_000_000.0
                        if (event.error == VideoRecordEvent.Finalize.ERROR_DURATION_LIMIT_REACHED) reason = "max-duration"
                        else if (event.hasError() && failure == null) failure = "CameraX finalize ${event.error}: ${event.cause?.message ?: "recording failed"}"
                        complete()
                    }
                }
            if (state == "paused") recording?.pause()
        } catch (e: Exception) {
            if (recording != null) {
                failureCode = (e as? TrackError)?.code ?: "NotReadableError"
                finish("error", "the camera could not reattach the recorder: ${e.message}") {}
                return
            }
            this.slot?.busy = false
            this.slot = null
            output?.let { this.source?.remove(it) }
            output = null
            this.source = null
            throw (e as? TrackError ?: TrackError("NotReadableError"))
        }
    }

    override fun pause(why: String) {
        if (state != "recording") return
        agentVideo?.pause(context.now())
        recorded = duration
        state = "paused"
        reason = why
        limit?.invoke(); limit = null
        recording?.pause()
        changed()
    }

    override fun resume() {
        if (state != "paused") return
        state = "recording"
        reason = null
        since = context.now()
        agentVideo?.resume(since)
        recording?.resume()
        arm()
        changed()
    }

    override fun muted(reason: String) { if (reason != "idle") pause(if (reason == "background") reason else "interrupted") }
    override fun unmuted() {}
    override fun ended() = finish("track-ended", null) {}

    override fun finish(why: String, failure: String?, done: (JSONObject) -> Unit) {
        answer?.let { return done(it) }
        waiting += done
        if (finishing) return
        finishing = true
        recorded = duration
        state = "inactive"
        reason = why
        this.failure = failure
        limit?.invoke(); limit = null
        if (context.agent) {
            val parts = agentVideo?.finish(context.now())
            agentVideo = null
            val bytes = { android.util.Base64.decode(AGENT_VIDEO, android.util.Base64.DEFAULT) }
            if (parts != null && failure == null) {
                encodedClip = true
                Thread({
                    val error = try { writeAgentVideo(file, parts, bytes); null } catch (e: Exception) { e.message ?: e.toString() }
                    android.os.Handler(android.os.Looper.getMainLooper()).post {
                        this.failure = error
                        complete()
                    }
                }, "exact-agent-video").start()
            } else {
                if (failure == null) try {
                    file.parentFile?.mkdirs()
                    dev.exact.canvas.Files.write(file, bytes())
                } catch (e: Exception) { this.failure = "the file was not written: ${e.message}" }
                complete()
            }
        } else if (recording != null) recording?.stop() else {
            if (failure == null) failureCode = "NotReadableError"
            this.failure = failure ?: "the camera supplied no video"
            complete()
        }
        changed()
    }

    private fun complete() {
        if (answer != null) return
        state = "inactive"
        reason = reason ?: "track-ended"
        limit?.invoke(); limit = null
        val take = if (failure == null) try {
            val metadata = if (context.agent && !encodedClip) JSONObject().put("duration", 300).put("width", 16).put("height", 16).put("frameRate", 10)
            else metadata(file)
            metadata.put("path", path).put("mimeType", "video/mp4").put("size", file.length()).also { result = it }
        } catch (e: Exception) {
            failure = "the recorded file could not be read: ${e.message}"
            JSONObject().put("error", "UnknownError")
        } else JSONObject().put("error", failureCode)
        if (failure != null) { reason = "error"; take.put("message", failure) }
        output?.let { source?.remove(it) }
        output = null; source = null; recording?.close(); recording = null
        slot?.busy = false; slot = null
        leave?.invoke(); leave = null
        answer = take
        changed()
        waiting.toList().also { waiting.clear() }.forEach { it(take) }
    }

    companion object {
        fun metadata(file: File): JSONObject {
            val reader = MediaMetadataRetriever()
            try {
                reader.setDataSource(file.path)
                fun number(key: Int) = reader.extractMetadata(key)?.toDoubleOrNull()
                val width = number(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH) ?: 0.0
                val height = number(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT) ?: 0.0
                val rotated = (number(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION) ?: 0.0) % 180 == 90.0
                val duration = number(MediaMetadataRetriever.METADATA_KEY_DURATION) ?: 0.0
                val frames = number(MediaMetadataRetriever.METADATA_KEY_VIDEO_FRAME_COUNT)
                val rate = if (frames != null && frames > 0 && duration > 0) frames * 1000 / duration
                    else number(MediaMetadataRetriever.METADATA_KEY_CAPTURE_FRAMERATE)?.takeIf { it > 0 }
                return JSONObject().put("duration", duration)
                    .put("width", if (rotated) height else width)
                    .put("height", if (rotated) width else height)
                    .put("frameRate", rate)
            } finally { reader.release() }
        }
    }
}

// The agent's three-frame MP4 from expo/exact#752, 16×16 at 10 fps.
private const val AGENT_VIDEO = "AAAAHGZ0eXBtcDQyAAAAAWlzb21tcDQxbXA0MgAAAAFtZGF0AAAAAAAAAUAAAAA1BgUtR1ZK3FxMQz+U78URPNFDqAEAAAMAAQMAAAMAAQIAAeYACwAAAwAAAwABBG4MA5ErAYAAAAA5JbggH94I48b/gn3JiSzDkGtC7AAVHn0Yh2pXegoZHua5g6mhDpEeQnKvseY9w7tuUUAAdYAmZTIwAAAAGgYFFUdWStxcTEM/lO/FETzRQ6gDAAADAAGAAAAAOiW4EAf/3gjjxv+D0JieB2A35Cv0AEQgaTZvNHVF8uS3pHtcpBeHl892l/d7Yzk4xDwcAAVkAUMhoMAAAAAaBgUVR1ZK3FxMQz+U78URPNFDqAMAAAMAAYAAAAA8JbgYB//eCOPG/4LEumJ5DOTohdgAKj26BtZwTUYAOG3eiijluVy/iqDvKq3e0V1TETcb2NoASEAdAcN2AAACtW1vb3YAAABsbXZoZAAAAADnIZ8Q5yGfEQAAAlgAAAC0AAEAAAEAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAJBdHJhawAAAFx0a2hkAAAAAechnxHnIZ8RAAAAAQAAAAAAAAC0AAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAQAAAAAAQAAAAEAAAAAAAJGVkdHMAAAAcZWxzdAAAAAAAAAABAAAAtAAAAAAAAQAAAAABuW1kaWEAAAAgbWRoZAAAAADnIZ8R5yGfEQAAAlgAAAC0VcQAAAAAADFoZGxyAAAAAAAAAAB2aWRlAAAAAAAAAAAAAAAAQ29yZSBNZWRpYSBWaWRlbwAAAAFgbWluZgAAABR2bWhkAAAAAQAAAAAAAAAAAAAAJGRpbmYAAAAcZHJlZgAAAAAAAAABAAAADHVybCAAAAABAAABIHN0YmwAAAChc3RzZAAAAAAAAAABAAAAkWF2YzEAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAEAAQAEgAAABIAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY//8AAAAnYXZjQwFkAAv/4QAMJ2QAC6xXDDeBZgpQAQAEKO48sP34+AAAAAAKZmllbAEAAAAACmNocm0AAAAAABhzdHRzAAAAAAAAAAEAAAADAAAAPAAAAA9zZHRwAAAAACgoKAAAABxzdHNjAAAAAAAAAAEAAAABAAAAAwAAAAEAAAAgc3RzegAAAAAAAAAAAAAAAwAAAHYAAABcAAAAXgAAABRzdGNvAAAAAAAAAAEAAAAs"
