package exact.facedetection

import android.content.res.AssetFileDescriptor
import android.content.ComponentCallbacks2
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.graphics.Color
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.PointF
import android.graphics.Rect
import android.media.ExifInterface
import android.os.ParcelFileDescriptor
import android.os.Looper
import com.google.android.gms.tasks.TaskCompletionSource
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.face.Face
import com.google.mlkit.vision.face.FaceDetection
import com.google.mlkit.vision.face.FaceDetector
import com.google.mlkit.vision.face.FaceDetectorOptions
import com.google.mlkit.vision.face.FaceLandmark
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadow.api.Shadow
import org.robolectric.util.ReflectionHelpers
import java.io.File
import java.lang.reflect.Proxy
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [FaceDetectionTest.Host::class,
    FaceDetectionTest.Detection::class, FaceDetectionTest.Images::class, FaceDetectionTest.Faces::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class FaceDetectionTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            var agent = false
            @JvmStatic @Implementation fun underAgent(name: String) = agent
            @JvmStatic @Implementation fun appFile(path: String) = path
            @JvmStatic @Implementation fun extensionLog(name: String, line: String) {}
        }
    }

    /** ML Kit's Face is built from obfuscated internals: a shadow answers its getters instead. */
    @Implements(Face::class, isInAndroidSdk = false)
    class Faces {
        var box = Rect()
        var landmarks = emptyMap<Int, PointF>()
        var angles = floatArrayOf(0f, 0f, 0f)
        var eyes: Pair<Float?, Float?> = null to null
        @Implementation fun getBoundingBox() = box
        @Implementation fun getLandmark(type: Int): FaceLandmark? = landmarks[type]?.let {
            ReflectionHelpers.callConstructor(FaceLandmark::class.java,
                ReflectionHelpers.ClassParameter.from(Int::class.javaPrimitiveType, type),
                ReflectionHelpers.ClassParameter.from(PointF::class.java, it))
        }
        @Implementation fun getHeadEulerAngleX() = angles[0]
        @Implementation fun getHeadEulerAngleY() = angles[1]
        @Implementation fun getHeadEulerAngleZ() = angles[2]
        @Implementation fun getLeftEyeOpenProbability() = eyes.first
        @Implementation fun getRightEyeOpenProbability() = eyes.second
    }

    @Implements(FaceDetection::class, isInAndroidSdk = false)
    class Detection {
        companion object {
            val modes = ArrayList<Int>()
            var closed = 0
            var onProcess: (() -> Unit)? = null
            var pending: TaskCompletionSource<List<Face>>? = null
            var failure: Exception? = null
            var loadError: Error? = null
            var faces = emptyList<Face>()
            var size = emptyList<Int>()
            var corners = emptyList<Int>()
            @JvmStatic @Implementation fun getClient(options: FaceDetectorOptions): FaceDetector {
                loadError?.let { throw it }
                modes += modes.size // One per client made.
                return Proxy.newProxyInstance(FaceDetector::class.java.classLoader, arrayOf(FaceDetector::class.java)) { _, method, args ->
                    when (method.name) {
                        "process" -> {
                            val image = args!![0] as InputImage
                            val pixels = requireNotNull(Shadow.extract<Images>(image).bitmap)
                            size = listOf(pixels.width, pixels.height)
                            corners = listOf(pixels.getPixel(4, 4), pixels.getPixel(pixels.width - 5, 4),
                                pixels.getPixel(pixels.width - 5, pixels.height - 5), pixels.getPixel(4, pixels.height - 5))
                            val task = pending?.task ?: failure?.let { Tasks.forException<List<Face>>(it) } ?: Tasks.forResult(faces)
                            onProcess?.invoke()
                            task
                        }
                        "close" -> { closed++; null }
                        else -> null
                    }
                } as FaceDetector
            }
        }
    }

    @Implements(InputImage::class, isInAndroidSdk = false)
    class Images {
        var bitmap: Bitmap? = null
        companion object {
            @JvmStatic @Implementation fun fromBitmap(bitmap: Bitmap, rotation: Int): InputImage {
                assertEquals(0, rotation)
                return Shadow.newInstanceOf(InputImage::class.java).also { Shadow.extract<Images>(it).bitmap = bitmap }
            }
        }
    }

    private lateinit var module: FaceDetectionModule
    private lateinit var file: File

    private fun picture(width: Int, height: Int, destination: File = file) {
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val paint = Paint()
        for ((index, color) in listOf(Color.RED, Color.GREEN, Color.YELLOW, Color.BLUE).withIndex()) {
            val x = index % 2 * (width / 2f)
            val y = index / 2 * (height / 2f)
            paint.color = color
            canvas.drawRect(x, y, x + width / 2f, y + height / 2f, paint)
        }
        destination.outputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, 100, it) }
        bitmap.recycle()
    }

    private fun face(box: Rect, landmarks: Map<Int, PointF>, angles: FloatArray = floatArrayOf(1f, -2f, 3f),
                     eyes: Pair<Float?, Float?> = 0.9f to null): Face =
        Shadow.newInstanceOf(Face::class.java).also {
            val shadow = Shadow.extract<Faces>(it)
            shadow.box = box; shadow.landmarks = landmarks; shadow.angles = angles; shadow.eyes = eyes
        }

    @Before fun setUp() {
        Host.agent = false
        Detection.loadError = null; Detection.pending = null; Detection.onProcess = null
        Detection.modes.clear(); Detection.closed = 0; Detection.failure = null
        Detection.faces = emptyList(); Detection.size = emptyList(); Detection.corners = emptyList()
        module = FaceDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "face-detection"))
        file = File.createTempFile("face-detection", ".jpg", RuntimeEnvironment.getApplication().cacheDir)
        picture(80, 40)
    }

    @After fun tearDown() {
        module.destroy {}
        assertTrue(module.work.awaitTermination(10, TimeUnit.SECONDS))
        shadowOf(Looper.getMainLooper()).idle()
        file.delete()
        FaceDetectionModule.optionsHatch = null
    }

    private fun call(args: JSONObject = JSONObject().put("image", file.path)): JSONObject {
        val answers = ArrayList<JSONObject>()
        module.later("detectFaces", args) { answers += it }
        module.work.submit {}.get(10, TimeUnit.SECONDS)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("one reply", 1, answers.size)
        return answers[0]
    }

    @Test fun answersTheSpecShapeInFullImagePixels() {
        Detection.faces = listOf(face(Rect(-10, 4, 30, 28), mapOf(
            FaceLandmark.LEFT_EYE to PointF(14f, 10f), FaceLandmark.RIGHT_EYE to PointF(24f, 10f),
            FaceLandmark.MOUTH_LEFT to PointF(15f, 22f), FaceLandmark.MOUTH_BOTTOM to PointF(19f, 24f),
            FaceLandmark.MOUTH_RIGHT to PointF(23f, 22f), FaceLandmark.NOSE_BASE to PointF(19f, 17f),
            FaceLandmark.LEFT_EAR to PointF(9f, 14f))))
        val answer = call()
        assertEquals(80, answer.getInt("width")); assertEquals(40, answer.getInt("height"))
        val face = answer.getJSONArray("faces").getJSONObject(0)
        val box = face.getJSONObject("boundingBox")
        assertEquals(listOf(-10.0, 4.0, 40.0, 24.0), listOf("x", "y", "width", "height").map { box.getDouble(it) })
        val landmarks = face.getJSONArray("landmarks")
        assertEquals(listOf("eye", "eye", "mouth", "nose"), (0 until landmarks.length()).map { landmarks.getJSONObject(it).getString("type") })
        assertEquals(3, landmarks.getJSONObject(2).getJSONArray("locations").length())
        assertEquals(19.0, landmarks.getJSONObject(2).getJSONArray("locations").getJSONObject(1).getDouble("x"), 0.0)
        val android = face.getJSONObject("android")
        val pose = face.getJSONObject("pose")
        assertEquals(listOf(-2.0, 1.0, 3.0), listOf("yaw", "pitch", "roll").map { pose.getDouble(it) })
        assertEquals(0.9, android.getDouble("leftEyeOpenProbability"), 1e-6)
        assertEquals(setOf("leftEyeOpenProbability"), android.keys().asSequence().toSet())
        assertFalse("an unknown probability is absent", android.has("rightEyeOpenProbability"))
    }

    @Test fun omitsAndroidWhenNeitherEyeIsClassified() {
        val f = face(Rect(0, 0, 10, 10), emptyMap(), eyes = null to null)
        assertFalse(FaceDetectionModule.shape(f, 1.0, 1.0, reportsPose = true).has("android"))
    }

    @Test fun trimWaitsForTheReadAndRetiresEachClient() {
        val entered = CountDownLatch(1)
        val pending = TaskCompletionSource<List<Face>>()
        Detection.pending = pending
        Detection.onProcess = { entered.countDown() }
        val args = JSONObject().put("image", file.path)
        val replies = mutableListOf<JSONObject>()
        val memory = ReflectionHelpers.getField<ComponentCallbacks2>(module, "memory")
        module.later("detectFaces", args) { replies += it }
        assertTrue(entered.await(10, TimeUnit.SECONDS))
        memory.onTrimMemory(ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW)
        Detection.pending = null
        module.later("detectFaces", args) { replies += it }
        memory.onLowMemory()
        pending.setResult(emptyList())
        module.work.submit {}.get(10, TimeUnit.SECONDS)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(2, replies.count { it.has("faces") })
        assertEquals(2, Detection.modes.size)
        assertEquals(2, Detection.closed)
    }

    @Test fun answersTheLargestFacesFirst() {
        Detection.faces = listOf(face(Rect(0, 0, 4, 4), emptyMap()), face(Rect(0, 0, 20, 20), emptyMap()), face(Rect(0, 0, 8, 8), emptyMap()))
        fun widths(answer: JSONObject) = answer.getJSONArray("faces").let { faces ->
            (0 until faces.length()).map { faces.getJSONObject(it).getJSONObject("boundingBox").getDouble("width") }
        }
        assertEquals(listOf(20.0, 8.0), widths(call(JSONObject().put("image", file.path).put("maxDetectedFaces", 2))))
        assertTrue(widths(call(JSONObject().put("image", file.path).put("maxDetectedFaces", 0))).isEmpty())
        assertEquals("largest first without a limit too", listOf(20.0, 8.0, 4.0), widths(call()))
    }

    @Test fun everyExifOrientationReachesRecognitionUpright() {
        // Source corners clockwise from top left: red, green, blue, yellow.
        val expected = listOf(
            listOf(Color.RED, Color.GREEN, Color.BLUE, Color.YELLOW),
            listOf(Color.GREEN, Color.RED, Color.YELLOW, Color.BLUE),
            listOf(Color.BLUE, Color.YELLOW, Color.RED, Color.GREEN),
            listOf(Color.YELLOW, Color.BLUE, Color.GREEN, Color.RED),
            listOf(Color.RED, Color.YELLOW, Color.BLUE, Color.GREEN),
            listOf(Color.YELLOW, Color.RED, Color.GREEN, Color.BLUE),
            listOf(Color.BLUE, Color.GREEN, Color.RED, Color.YELLOW),
            listOf(Color.GREEN, Color.BLUE, Color.YELLOW, Color.RED),
        )
        for (orientation in 1..8) {
            ExifInterface(file.path).apply { setAttribute(ExifInterface.TAG_ORIENTATION, orientation.toString()); saveAttributes() }
            val answer = call()
            val size = if (orientation >= 5) listOf(40, 80) else listOf(80, 40)
            assertEquals(size, listOf(answer.getInt("width"), answer.getInt("height")))
            assertEquals(size, Detection.size)
            for ((corner, color) in expected[orientation - 1].withIndex()) {
                val actual = Detection.corners[corner]
                // JPEG compression can change a channel slightly, even away from an edge.
                for (channel in listOf(Color::red, Color::green, Color::blue)) {
                    assertTrue("orientation $orientation corner $corner: expected $color, got $actual",
                        kotlin.math.abs(channel(color) - channel(actual)) <= 8)
                }
            }
        }
    }

    @Test fun aReplacedPathStillReadsTheOpenedPortrait() {
        ExifInterface(file.path).apply { setAttribute(ExifInterface.TAG_ORIENTATION, "6"); saveAttributes() }
        var opened = 0
        val source = ImageDecoder.createSource {
            opened++
            val fd = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
            val replacement = File.createTempFile("new-portrait", ".jpg", file.parentFile)
            picture(240, 160, replacement)
            assertTrue(replacement.renameTo(file))
            AssetFileDescriptor(fd, 0, AssetFileDescriptor.UNKNOWN_LENGTH)
        }
        val (bitmap, width, height) = requireNotNull(module.upright(source))
        try {
            assertEquals(1, opened)
            assertEquals(listOf(40, 80, 40, 80), listOf(width, height, bitmap.width, bitmap.height))
        } finally { bitmap.recycle() }
    }

    @Test fun decodesALargeStillSmallerAndScalesBack() {
        picture(4095, 3073)
        Detection.faces = listOf(face(Rect(100, 50, 300, 250), emptyMap()))
        val answer = call()
        assertTrue(Detection.size.all { it in 1..2048 })
        assertEquals(4095, answer.getInt("width"))
        assertEquals(3073, answer.getInt("height"))
        val box = answer.getJSONArray("faces").getJSONObject(0).getJSONObject("boundingBox")
        val sx = 4095.0 / Detection.size[0]
        val sy = 3073.0 / Detection.size[1]
        assertEquals(listOf(100 * sx, 50 * sy, 200 * sx, 200 * sy), listOf("x", "y", "width", "height").map { box.getDouble(it) })
        assertEquals(1, FaceDetectionModule.sampleSize(2048, 100))
        assertEquals(4, FaceDetectionModule.sampleSize(8160, 6120))
    }

    @Test fun reusesOneClientPerModeAndClosesThem() {
        call(); call(); call(JSONObject().put("image", file.path).put("fastMode", true))
        assertEquals(2, Detection.modes.size)
        module.destroy {}
        assertTrue(module.work.awaitTermination(10, TimeUnit.SECONDS))
        assertEquals(2, Detection.closed)
        // tearDown owns the current module.
        module = FaceDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "face-detection"))
    }

    @Test fun hatchRunsOnceOnMainAndAgentUsesDefaults() {
        var calls = 0
        FaceDetectionModule.optionsHatch = {
            assertSame(Looper.getMainLooper(), Looper.myLooper())
            calls++
        }
        call(); call()
        module.destroy {}
        assertTrue(module.work.awaitTermination(10, TimeUnit.SECONDS))
        Host.agent = true
        module = FaceDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "face-detection"))
        call()
        assertEquals(1, calls)
        assertEquals(2, Detection.modes.size)
    }

    @Test fun onlyTheContourOnlyFastHatchOmitsPose() {
        FaceDetectionModule.optionsHatch = {
            it.setContourMode(FaceDetectorOptions.CONTOUR_MODE_ALL)
                .setLandmarkMode(FaceDetectorOptions.LANDMARK_MODE_NONE)
                .setClassificationMode(FaceDetectorOptions.CLASSIFICATION_MODE_NONE)
                .setMinFaceSize(0.25f)
        }
        Detection.faces = listOf(face(Rect(0, 0, 20, 20), emptyMap(), angles = FloatArray(3)))
        for (fast in listOf(true, false, true)) {
            val args = JSONObject().put("image", file.path).put("fastMode", fast)
            val detected = call(args).getJSONArray("faces").getJSONObject(0)
            assertEquals(!fast, detected.has("pose"))
        }
    }

    @Test fun repliesAfterNativeLoadFailureAndRetries() {
        for (error in listOf(UnsatisfiedLinkError("detector library"), OutOfMemoryError("bitmap"))) {
            Detection.loadError = error
            assertEquals("OperationError", call().getString("error"))
        }
        Detection.loadError = null
        assertEquals(0, call().getJSONArray("faces").length())
    }

    @Test fun failsWithTheSharedErrors() {
        for (limit in listOf(-1, 0.5, 65536)) {
            assertEquals("TypeError", call(JSONObject().put("image", file.path).put("maxDetectedFaces", limit)).getString("error"))
        }
        assertEquals("NotFoundError", call(JSONObject().put("image", File(file.parent, "absent.jpg").path)).getString("error"))
        file.writeText("not a picture")
        assertEquals("InvalidStateError", call().getString("error"))
        picture(80, 40)
        Detection.failure = IllegalStateException("model")
        val failed = call()
        assertEquals("OperationError", failed.getString("error"))
        assertEquals("model", failed.getString("message"))
    }
}
