// Each poll asks for one frame. CameraX drops intervening frames; concurrent polls share the same analysis.
@file:androidx.annotation.OptIn(androidx.camera.core.ExperimentalGetImage::class)

package exact.barcodedetection

import android.util.Size
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import com.google.mlkit.vision.barcode.BarcodeScanner
import com.google.mlkit.vision.common.InputImage
import dev.exact.canvas.TrackConsumer
import dev.exact.canvas.VideoSource
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executor

internal class Scan(
    private val module: BarcodeDetectionModule,
    private val track: String,
    private val keep: Set<String>?,
) : TrackConsumer {
    companion object {
        const val KEPT = 32
        const val PATIENCE_MS = 500L
        fun key(track: String, keep: Set<String>?) = "$track|${keep?.sorted()?.joinToString(",") ?: "*"}"
    }

    private class Poll {
        val replies = ArrayList<(JSONObject) -> Unit>()
        var timeout: Runnable? = null
    }

    private var leave: (() -> Unit)? = null
    private var leavePicture: (() -> Unit)? = null
    private var live = false
    private var agentBusy = false
    private var state = "idle"
    private var stopped = false
    private var frame = 0
    private var width = 0
    private var height = 0
    private var error: String? = null
    private var barcodes = JSONArray()
    private val seen = LinkedHashMap<Pair<String, String>, JSONObject>()
    private var pending: Poll? = null
    private var wanted: Poll? = null
    private var output: Output? = null
    private var source: VideoSource? = null
    private var available: VideoSource? = null

    fun start() {
        if (module.context.agent) leavePicture = module.context.tracks.lookup(track)?.observePicture { if (live) resume() }
        leave = module.context.tracks.observe(track, this)
    }

    /** One detector and analysis output per capture; each scan filters the result and owns its history. */
    private class Output {
        private val main = android.os.Handler(android.os.Looper.getMainLooper())
        private val executor = Executor { main.post(it) }
        private val scans = LinkedHashSet<Scan>()
        private var scanner: BarcodeScanner? = null
        private var formats: Set<String>? = null
        private var busy = false
        // Dense PDF417 needs more pixels than CameraX's default analysis resolution.
        val analysis = ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .setResolutionSelector(ResolutionSelector.Builder().setResolutionStrategy(
                ResolutionStrategy(Size(1920, 1080), ResolutionStrategy.FALLBACK_RULE_CLOSEST_LOWER_THEN_HIGHER)
            ).build())
            .build()

        fun attach(source: VideoSource, scan: Scan, angle: Int): Boolean {
            rotation(angle)
            if (scan in scans) return true
            scans.add(scan)
            try {
                updateDetector()
                if (!source.attach(analysis)) {
                    scans.remove(scan)
                    updateDetector()
                    return false
                }
                analysis.setAnalyzer(executor, ::analyze)
                return true
            } catch (e: Throwable) {
                scans.remove(scan)
                updateDetector()
                throw e
            }
        }

        fun detach(source: VideoSource, scan: Scan) {
            scans.remove(scan)
            if (scans.isEmpty()) {
                analysis.clearAnalyzer()
                source.remove(analysis)
            }
            updateDetector()
        }

        private fun updateDetector() {
            // ML Kit owns the image until completion. Never close or replace its client mid-task.
            if (busy) return
            if (scans.isEmpty()) {
                scanner?.close(); scanner = null
                return
            }
            val union = if (scans.any { it.keep == null }) null else scans.flatMap { it.keep!! }.toSet()
            if (scanner != null && formats == union) return
            val next = BarcodeDetectionModule.scanner(union)
            scanner?.close()
            scanner = next
            formats = union
        }

        private fun analyze(proxy: ImageProxy) {
            val readers = scans.mapNotNull { scan -> scan.wanted?.also { scan.wanted = null }?.let { scan to it } }
            var w = 0
            var h = 0
            fun finish(found: Result<List<JSONObject>>?) {
                try {
                    for ((scan, poll) in readers) {
                        if (found == null) scan.answerWaiting(poll) else scan.complete(poll, found, w, h)
                    }
                } finally {
                    try { proxy.close() } finally {
                        busy = false
                        updateDetector()
                    }
                }
            }
            busy = true
            runCatching {
                val media = if (readers.isEmpty()) null else proxy.image
                media?.let {
                    val rotation = proxy.imageInfo.rotationDegrees
                    val turned = rotation == 90 || rotation == 270
                    w = if (turned) proxy.height else proxy.width
                    h = if (turned) proxy.width else proxy.height
                    requireNotNull(scanner).process(InputImage.fromMediaImage(it, rotation))
                }
            }.fold(onSuccess = { task ->
                if (task == null) finish(null)
                else task.addOnCompleteListener(executor) { completed ->
                    finish(runCatching {
                        if (!completed.isSuccessful) throw completed.exception ?: IllegalStateException("ML Kit failed")
                        completed.result.map(BarcodeDetectionModule::shape)
                    })
                }
            }, onFailure = { finish(Result.failure(it)) })
        }

        fun rotation(angle: Int) {
            analysis.targetRotation = when (angle) {
                90 -> android.view.Surface.ROTATION_90
                180 -> android.view.Surface.ROTATION_180
                270 -> android.view.Surface.ROTATION_270
                else -> android.view.Surface.ROTATION_0
            }
        }
    }

    fun orientation(angle: Int) { output?.rotation(angle) }

    fun stop() {
        if (stopped) return
        stopped = true
        leave?.invoke(); leave = null
        leavePicture?.invoke(); leavePicture = null
        detach()
        available = null
        pending?.let(::answerWaiting)
    }

    override fun liveVideo(source: VideoSource?) {
        if (stopped) return
        detach()
        live = true
        available = source
        resume()
    }

    private fun resume() {
        if (stopped) return
        val capture = available
        if (capture != null && output == null) {
            try {
                val shared = capture.shared("barcode-detection.analysis") { Output() }
                check(shared.attach(capture, this, module.context.orientation)) { "analysis rejected" }
                source = capture
                output = shared
                error = null
            } catch (e: Throwable) {
                error = "OperationError"
                set("idle")
                pending?.let(::answerWaiting)
                module.context.changed("barcode-detection.detectedBarcodes")
                return
            }
        }
        set(if (output != null || (module.context.agent && picture() != null)) "scanning" else "idle")
    }

    override fun muted(reason: String) {
        live = false
        if (reason == "idle") { detach(); available = null }
        set("idle")
    }
    override fun unmuted() { live = true; resume() }
    override fun ended() { live = false; detach(); available = null; set("ended") }

    private fun detach() {
        source?.let { output?.detach(it, this) }
        output = null
        source = null
    }

    private fun set(next: String) {
        if (stopped || state == next) return
        state = next
        module.disclose()
        if (next != "scanning") pending?.let(::answerWaiting)
        module.context.changed("barcode-detection.detectedBarcodes")
    }

    fun analyse(reply: (JSONObject) -> Unit) {
        if (stopped || state != "scanning") return reply(answer())
        pending?.let { it.replies += reply; return }
        if (module.context.agent && agentBusy) return reply(answer())
        val poll = Poll().also { it.replies += reply }
        pending = poll
        if (module.context.agent) return analysePicture(poll)
        wanted = poll
        poll.timeout = Runnable {
            // A frame already in ML Kit is still in flight; only a capture that delivered none times out.
            if (wanted === poll) answerWaiting(poll)
        }.also { module.main.postDelayed(it, PATIENCE_MS) }
    }

    private fun picture() = module.context.tracks.lookup(track)?.picture

    private fun analysePicture(poll: Poll) {
        val bitmap = picture() ?: return answerWaiting(poll)
        var client: BarcodeScanner? = null
        agentBusy = true
        fun finish(found: Result<List<JSONObject>>) {
            try { complete(poll, found, bitmap.width, bitmap.height) }
            finally { agentBusy = false; client?.close() }
        }
        try {
            client = BarcodeDetectionModule.scanner(keep)
            client.process(InputImage.fromBitmap(bitmap, 0)).addOnCompleteListener(Executor { module.main.post(it) }) { task ->
                finish(runCatching {
                    if (!task.isSuccessful) throw task.exception ?: IllegalStateException("ML Kit failed")
                    task.result.map(BarcodeDetectionModule::shape)
                })
            }
        } catch (e: Throwable) { finish(Result.failure(e)) }
    }

    private fun complete(poll: Poll, found: Result<List<JSONObject>>, w: Int, h: Int) {
        // A result from before mute, end or release cannot answer a newer poll or repopulate its history.
        if (pending !== poll || stopped) return
        took(found, w, h)
        answerWaiting(poll)
    }

    private fun took(found: Result<List<JSONObject>>, w: Int, h: Int) {
        found.onFailure { error = it.message ?: "ML Kit failed" }.onSuccess { codes ->
            val kept = BarcodeDetectionModule.normalised(codes, keep)
            val at = module.context.now()
            frame += 1; width = w; height = h; barcodes = JSONArray(kept); error = null
            for (b in kept.distinctBy { it.optString("format", "unknown") to it.optString("rawValue") }) {
                val format = b.optString("format", "unknown")
                val raw = b.optString("rawValue")
                val key = format to raw
                val was = seen.remove(key)
                seen[key] = JSONObject().put("rawValue", raw).put("format", format)
                    .put("firstSeen", was?.optDouble("firstSeen") ?: at).put("lastSeen", at)
                    .put("frames", (was?.optInt("frames") ?: 0) + 1)
            }
            while (seen.size > KEPT) seen.remove(seen.keys.first())
        }
        module.disclose()
    }

    private fun answerWaiting(poll: Poll) {
        if (pending !== poll) return
        pending = null
        if (wanted === poll) wanted = null
        poll.timeout?.let(module.main::removeCallbacks)
        val answer = answer()
        poll.replies.forEach { it(answer) }
    }

    private fun answer(): JSONObject {
        val answer = JSONObject().put("state", state).put("barcodes", barcodes).put("seen", JSONArray(seen.values.reversed()))
            .put("frame", frame).put("width", width).put("height", height)
        error?.let { answer.put("error", it) }
        return answer
    }

    fun disclosure(interval: Double): JSONObject = JSONObject().put("track", track)
        .put("formats", keep?.let { JSONArray(it.sorted()) } ?: JSONObject.NULL)
        .put("interval", interval).put("state", state).put("frames", frame)
        .put("seen", JSONArray(seen.values.map { "${it.getString("format")} ${it.getString("rawValue")}" }))
}
