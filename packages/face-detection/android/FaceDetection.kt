// ML Kit's bundled face detector on still images, by the Shape Detection API's FaceDetector names.
// https://wicg.github.io/shape-detection-api/#face-detection-api
package exact.facedetection

import android.content.ComponentCallbacks2
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.graphics.PointF
import android.os.Handler
import android.os.Looper
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.face.Face
import com.google.mlkit.vision.face.FaceDetection
import com.google.mlkit.vision.face.FaceDetectorOptions
import com.google.mlkit.vision.face.FaceLandmark
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.FileNotFoundException
import java.util.concurrent.ExecutionException
import java.util.concurrent.Executors
import kotlin.math.max

class FaceDetectionModule(context: ExtensionContext) : ExactExtension(context) {
    internal val work = Executors.newSingleThreadExecutor { Thread(it, "exact.face-detection") }
    private val main = Handler(Looper.getMainLooper())
    // Main owns the options and cache. Only the worker initializes, uses and closes each client.
    private class Detector(options: FaceDetectorOptions, val reportsPose: Boolean) {
        val client = lazy(LazyThreadSafetyMode.NONE) { FaceDetection.getClient(options) }
    }
    private val detectors = HashMap<Boolean, Detector>()
    private var watchingMemory = false
    private val memory = object : ComponentCallbacks2 {
        override fun onConfigurationChanged(configuration: Configuration) {}
        override fun onLowMemory() = discard()
        override fun onTrimMemory(level: Int) {
            if (level != ComponentCallbacks2.TRIM_MEMORY_UI_HIDDEN) discard()
        }
    }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        when (name) {
            "detectFaces" -> still(args, reply)
            else -> reply(JSONObject().put("error", "NotSupportedError"))
        }
    }

    companion object {
        // A full 50 MP camera bitmap would use 200 MB before the detector sees it.
        const val DECODE_SIDE = 2048

        /** The power of two the decoder samples by so the longer side is at most [DECODE_SIDE]. */
        internal fun sampleSize(width: Int, height: Int): Int {
            var sample = 1
            while (max(width, height) > DECODE_SIDE.toLong() * sample) sample *= 2
            return sample
        }

        /** On main before creating a client. A drive uses the standard options. */
        var optionsHatch: ((FaceDetectorOptions.Builder) -> Unit)? = null

        private fun options(fast: Boolean) = FaceDetectorOptions.Builder()
            .setPerformanceMode(if (fast) FaceDetectorOptions.PERFORMANCE_MODE_FAST else FaceDetectorOptions.PERFORMANCE_MODE_ACCURATE)
            .setMinFaceSize(0.1f)
            .setLandmarkMode(FaceDetectorOptions.LANDMARK_MODE_ALL)
            .setClassificationMode(FaceDetectorOptions.CLASSIFICATION_MODE_ALL)

        private fun point(p: PointF, sx: Double, sy: Double) =
            JSONObject().put("x", p.x * sx).put("y", p.y * sy)

        /** Coordinates scale independently because the decoder rounds each sampled dimension. */
        internal fun shape(face: Face, sx: Double, sy: Double, reportsPose: Boolean): JSONObject {
            val box = face.boundingBox
            val landmarks = JSONArray()
            for (type in intArrayOf(FaceLandmark.LEFT_EYE, FaceLandmark.RIGHT_EYE)) {
                face.getLandmark(type)?.let {
                    landmarks.put(JSONObject().put("type", "eye").put("locations", JSONArray().put(point(it.position, sx, sy))))
                }
            }
            val mouth = listOf(FaceLandmark.MOUTH_LEFT, FaceLandmark.MOUTH_BOTTOM, FaceLandmark.MOUTH_RIGHT)
                .mapNotNull { face.getLandmark(it)?.position }
            if (mouth.isNotEmpty()) {
                landmarks.put(JSONObject().put("type", "mouth").put("locations", JSONArray(mouth.map { point(it, sx, sy) })))
            }
            face.getLandmark(FaceLandmark.NOSE_BASE)?.let {
                landmarks.put(JSONObject().put("type", "nose").put("locations", JSONArray().put(point(it.position, sx, sy))))
            }
            val android = JSONObject()
            face.leftEyeOpenProbability?.let { android.put("leftEyeOpenProbability", it.toDouble()) }
            face.rightEyeOpenProbability?.let { android.put("rightEyeOpenProbability", it.toDouble()) }
            return JSONObject()
                .put("boundingBox", JSONObject().put("x", box.left * sx).put("y", box.top * sy)
                    .put("width", box.width() * sx).put("height", box.height() * sy))
                .put("landmarks", landmarks)
                .apply {
                    if (reportsPose) put("pose", JSONObject().put("yaw", face.headEulerAngleY.toDouble())
                        .put("pitch", face.headEulerAngleX.toDouble()).put("roll", face.headEulerAngleZ.toDouble()))
                    if (android.length() > 0) put("android", android)
                }
        }

        internal fun largest(faces: List<Face>, limit: Int?): List<Face> {
            val sorted = faces.sortedByDescending { it.boundingBox.width().toLong() * it.boundingBox.height() }
            return if (limit == null) sorted else sorted.take(limit)
        }
    }

    private fun still(args: JSONObject, reply: (JSONObject) -> Unit) {
        val path = args.optString("image")
        val fast = args.optBoolean("fastMode", false)
        val maxFaces = (args.opt("maxDetectedFaces") as? Number)?.toDouble()
        if (maxFaces != null && (maxFaces !in 0.0..65535.0 || maxFaces % 1 != 0.0)) {
            return reply(JSONObject().put("error", "TypeError"))
        }
        val limit = maxFaces?.toInt()
        val file = context.fileToRead(path) ?: return reply(JSONObject().put("error", "NotFoundError"))
        if (!watchingMemory) {
            context.application.registerComponentCallbacks(memory)
            watchingMemory = true
        }
        val detector = try {
            detectors.getOrPut(fast) {
                val builder = options(fast)
                val hatch = optionsHatch.takeUnless { context.agent }
                hatch?.invoke(builder)
                val config = builder.build()
                // ML Kit has no public mode getters. Compare snapshots that differ only in
                // the four modes for which its primitive Euler getters have no measurements.
                val reportsPose = hatch == null || config != builder
                    .setLandmarkMode(FaceDetectorOptions.LANDMARK_MODE_NONE)
                    .setContourMode(FaceDetectorOptions.CONTOUR_MODE_ALL)
                    .setClassificationMode(FaceDetectorOptions.CLASSIFICATION_MODE_NONE)
                    .setPerformanceMode(FaceDetectorOptions.PERFORMANCE_MODE_FAST).build()
                Detector(config, reportsPose)
            }
        } catch (e: Exception) { return reply(failure(e)) }
        catch (e: LinkageError) { return reply(failure(e)) }
        work.execute {
            val answer = try {
                val decoded = upright(ImageDecoder.createSource(file))
                if (decoded == null) JSONObject().put("error", "InvalidStateError").put("message", "decode")
                else {
                    val (image, width, height) = decoded
                    try {
                        val sx = width.toDouble() / image.width
                        val sy = height.toDouble() / image.height
                        val found = Tasks.await(detector.client.value.process(InputImage.fromBitmap(image, 0)))
                        JSONObject().put("faces", JSONArray(largest(found, limit).map { shape(it, sx, sy, detector.reportsPose) }))
                            .put("width", width).put("height", height)
                    } finally { image.recycle() }
                }
            } catch (e: FileNotFoundException) { if (!file.isFile) JSONObject().put("error", "NotFoundError") else failure(e) }
            catch (e: Exception) { failure(e) }
            catch (e: LinkageError) { failure(e) }
            catch (e: OutOfMemoryError) { failure(e) }
            main.post {
                answer.optJSONArray("faces")?.let { context.log("face-detection: $path: ${it.length()} faces") }
                reply(answer)
            }
        }
    }

    /** The header and upright bitmap come from one decoder, even if the app replaces the photo. */
    internal fun upright(source: ImageDecoder.Source): Triple<Bitmap, Int, Int>? {
        var width = 0
        var height = 0
        val bitmap = try {
            ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
                width = info.size.width
                height = info.size.height
                decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                decoder.setTargetSampleSize(sampleSize(width, height))
            }
        } catch (_: ImageDecoder.DecodeException) { return null }
        return Triple(bitmap, width, height)
    }

    private fun failure(e: Throwable): JSONObject {
        val cause = if (e is ExecutionException) e.cause ?: e else e
        return JSONObject().put("error", "OperationError").put("message", cause.message ?: "ML Kit failed")
    }

    private fun discard() {
        if (detectors.isEmpty()) return
        val retired = detectors.values.toList()
        detectors.clear()
        work.execute { retired.forEach { if (it.client.isInitialized()) it.client.value.close() } }
    }

    override fun destroy(done: () -> Unit) {
        if (watchingMemory) context.application.unregisterComponentCallbacks(memory)
        discard()
        work.execute { main.post { done() } }
        work.shutdown()
    }
}
