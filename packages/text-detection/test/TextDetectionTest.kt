package exact.textdetection

import android.content.ComponentCallbacks2
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Point
import android.graphics.Rect
import android.media.ExifInterface
import android.os.Looper
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.Text
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.TextRecognizer
import com.google.mlkit.vision.text.TextRecognizerOptionsInterface
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONArray
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
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [TextDetectionTest.Host::class,
    TextDetectionTest.Recognition::class, TextDetectionTest.Images::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class TextDetectionTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            var agent = false
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = agent
            @JvmStatic @Implementation fun appFile(path: String) = path
            @JvmStatic @Implementation fun extensionLog(name: String, line: String) {}
        }
    }

    @Implements(TextRecognition::class, isInAndroidSdk = false)
    class Recognition {
        companion object {
            var made = 0
            var closed = 0
            var processed = 0
            var failure: Exception? = null
            var creationFailure = false
            var result: Text? = null
            var bitmap: Bitmap? = null
            var corners = emptyList<Int>()
            var size = emptyList<Int>()
            @JvmStatic @Implementation fun getClient(options: TextRecognizerOptionsInterface): TextRecognizer {
                made++
                if (creationFailure) throw IllegalStateException("cannot load model")
                return Proxy.newProxyInstance(TextRecognizer::class.java.classLoader, arrayOf(TextRecognizer::class.java)) { _, method, args ->
                    when (method.name) {
                        "process" -> {
                            processed++
                            val image = args!![0] as InputImage
                            assertEquals(0, image.rotationDegrees)
                            bitmap = image.bitmapInternal
                            val pixels = requireNotNull(bitmap)
                            size = listOf(pixels.width, pixels.height)
                            corners = listOf(pixels.getPixel(10, 10), pixels.getPixel(pixels.width - 11, 10),
                                pixels.getPixel(pixels.width - 11, pixels.height - 11), pixels.getPixel(10, pixels.height - 11))
                            failure?.let { Tasks.forException<Text>(it) } ?: Tasks.forResult(requireNotNull(result))
                        }
                        "close" -> { closed++; null }
                        else -> null
                    }
                } as TextRecognizer
            }
        }
    }
    @Implements(InputImage::class, isInAndroidSdk = false)
    class Images {
        var bitmap: Bitmap? = null
        @Implementation fun getBitmapInternal() = bitmap
        @Implementation fun getRotationDegrees() = 0
        companion object {
            @JvmStatic @Implementation fun fromBitmap(bitmap: Bitmap, rotation: Int): InputImage {
                assertEquals(0, rotation)
                return Shadow.newInstanceOf(InputImage::class.java).also { Shadow.extract<Images>(it).bitmap = bitmap }
            }
        }
    }

    private lateinit var module: TextDetectionModule
    private lateinit var file: File
    @Before fun setUp() {
        Host.agent = false
        Recognition.made = 0; Recognition.closed = 0; Recognition.processed = 0
        Recognition.failure = null; Recognition.creationFailure = false; Recognition.bitmap = null
        Recognition.corners = emptyList(); Recognition.size = emptyList()
        Recognition.result = result(emptyList())
        module = TextDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "text-detection"))
        file = File.createTempFile("text-detection", ".jpg", RuntimeEnvironment.getApplication().cacheDir)
        val bitmap = Bitmap.createBitmap(80, 40, Bitmap.Config.ARGB_8888)
        for (y in 0 until 40) for (x in 0 until 80) {
            bitmap.setPixel(x, y, if (y < 20) { if (x < 40) Color.RED else Color.GREEN }
                else { if (x < 40) Color.YELLOW else Color.BLUE })
        }
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, 100, it) }
        bitmap.recycle()
    }
    @After fun tearDown() {
        TextDetectionModule.optionsHatch = null
        module.destroy {}
        assertTrue(module.work.awaitTermination(10, TimeUnit.SECONDS))
        shadowOf(Looper.getMainLooper()).idle()
        file.delete()
    }

    private fun result(lines: List<List<Pair<String, String>>>): Text = Text("", lines.map { values ->
        val box = Rect(12, 24, 52, 44)
        val points = listOf(Point(12, 24), Point(52, 24), Point(50, 44), Point(10, 44))
        Text.TextBlock("", box, points, "und", null, values.map { (value, language) ->
            Text.Line(value, box, points, language, null, emptyList<Text.Element>(), 1f, 0f)
        })
    })
    private fun call(name: String = "detectText", args: JSONObject = JSONObject().put("image", file.path)): JSONObject {
        val answers = ArrayList<JSONObject>()
        module.later(name, args) { answers += it }
        module.work.submit {}.get(10, TimeUnit.SECONDS)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("one reply", 1, answers.size)
        return answers.single()
    }

    @Test fun availabilityNeedsNoModelAndMatchesRequestedLatinLanguages() {
        val defaults = call("textDetectorAvailability", JSONObject())
        assertEquals("available", defaults.getString("availability"))
        assertFalse(defaults.has("reason"))
        assertEquals("Latn", defaults.getJSONObject("android").getString("script"))
        val languages = defaults.getJSONArray("languages")
        assertTrue((0 until languages.length()).map(languages::getString).containsAll(listOf("en", "fr", "sr-Latn", "ace", "cy")))
        for (value in listOf(JSONArray("[\"en-US\",\"FR_ca\",\"en\",\"sr-Latn-BA\"]"), JSONArray())) {
            val answer = call("textDetectorAvailability", JSONObject().put("languages", value))
            assertEquals("available", answer.getString("availability"))
            assertEquals(if (value.length() == 0) "[]" else "[\"en\",\"fr\",\"sr-Latn\"]", answer.getJSONArray("languages").toString())
        }
        assertEquals(0, Recognition.made)
        assertEquals("de", TextDetectionModule.match("de-1901"))
        assertEquals("NotSupportedError", call("textDetectorAvailabilitySync").getString("error"))
    }

    @Test fun unsupportedLanguagesRefuseAvailabilityAndDetectionBeforeReadingAFile() {
        for (tag in listOf("ja", "zh-TW", "hi", "ru", "sr-Cyrl", "en-Cyrl", "zz")) {
            val args = JSONObject().put("languages", JSONArray().put("en").put(tag)).put("image", "absent")
            val available = call("textDetectorAvailability", args)
            assertEquals("unavailable", available.getString("availability"))
            assertEquals("unsupported-options", available.getString("reason"))
            assertEquals(0, available.getJSONArray("languages").length())
            assertEquals("NotSupportedError", call(args = args).getString("error"))
        }
        assertEquals(0, Recognition.made)
    }

    private val unsupportedScripts = listOf("sr", "sr-RS", "sr-BA", "sr-ME", "ms-zsm-Arab")

    @Test fun unsupportedScriptsRefuseAvailability() {
        val answers = unsupportedScripts.map { tag ->
            val answer = call("textDetectorAvailability", JSONObject().put("languages", JSONArray().put("en").put(tag)))
            listOf(tag, TextDetectionModule.match(tag), answer.getString("availability"),
                answer.optString("reason"), answer.getJSONArray("languages").toString())
        }
        assertEquals(unsupportedScripts.map { listOf(it, null, "unavailable", "unsupported-options", "[]") }, answers)
        assertEquals(0, Recognition.made)
    }

    @Test fun unsupportedScriptsRefuseDetection() {
        val answers = unsupportedScripts.map { tag ->
            tag to call(args = JSONObject().put("image", file.path).put("languages", JSONArray().put(tag))).optString("error")
        }
        assertEquals(unsupportedScripts.map { it to "NotSupportedError" }, answers)
        assertEquals(0, Recognition.processed)
    }

    @Test fun explicitLatinAndPrivateUseSubtagsKeepTheirSupportedLanguage() {
        for ((tag, language) in listOf("sr-Latn-RS" to "sr-Latn", "ms-zsm-Latn" to "ms", "en-x-Arab" to "en")) {
            assertEquals(tag, language, TextDetectionModule.match(tag))
            val args = JSONObject().put("image", file.path).put("languages", JSONArray().put(tag))
            val answer = call("textDetectorAvailability", args)
            assertEquals(tag, "available", answer.getString("availability"))
            assertEquals(tag, JSONArray().put(language).toString(), answer.getJSONArray("languages").toString())
            assertFalse(tag, call(args = args).has("error"))
        }
        assertEquals(3, Recognition.processed)
    }

    @Test fun detectionFlattensLinesInPlatformOrderAndKeepsPixelGeometryAndLanguage() {
        Recognition.result = result(listOf(listOf("First line" to "en", "Deuxième" to "fr"), listOf("Third" to "und")))
        val answer = call(args = JSONObject().put("image", file.path).put("languages", JSONArray("[\"en-US\",\"fr\"]")))
        assertEquals(80, answer.getInt("width")); assertEquals(40, answer.getInt("height"))
        val texts = answer.getJSONArray("texts")
        assertEquals(listOf("First line", "Deuxième", "Third"), (0..2).map { texts.getJSONObject(it).getString("rawValue") })
        assertEquals(listOf("en", "fr", "und"), (0..2).map { texts.getJSONObject(it).getJSONObject("android").getString("recognizedLanguage") })
        val line = texts.getJSONObject(0)
        val box = line.getJSONObject("boundingBox")
        assertEquals(listOf(12, 24, 40, 20), listOf("x", "y", "width", "height").map(box::getInt))
        assertEquals(listOf(12, 52, 50, 10), (0..3).map { line.getJSONArray("cornerPoints").getJSONObject(it).getInt("x") })
        assertTrue(requireNotNull(Recognition.bitmap).isRecycled)
    }

    @Test fun emptyDetectionIsASuccess() {
        val answer = call()
        assertEquals(0, answer.getJSONArray("texts").length())
        assertEquals(80, answer.getInt("width"))
        assertEquals(40, answer.getInt("height"))
    }

    @Test fun missingAndUndecodableFilesDoNotReachTheRecognizer() {
        assertEquals("NotFoundError", call(args = JSONObject().put("image", file.path + ".missing")).getString("error"))
        file.writeText("not an image")
        val answer = call()
        assertEquals("InvalidStateError", answer.getString("error"))
        assertEquals("decode", answer.getString("message"))
        assertEquals(0, Recognition.made)
    }

    @Test fun creationAndProcessingFailuresReplyOnceAndReleaseTheImage() {
        Recognition.creationFailure = true
        assertEquals("OperationError", call().getString("error"))
        Recognition.creationFailure = false
        Recognition.failure = IllegalStateException("recognition failed")
        val answer = call()
        assertEquals("OperationError", answer.getString("error"))
        assertEquals("recognition failed", answer.getString("message"))
        assertTrue(requireNotNull(Recognition.bitmap).isRecycled)
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
            assertEquals(size, Recognition.size)
            for ((corner, color) in expected[orientation - 1].withIndex()) {
                val actual = Recognition.corners[corner]
                // JPEG compression can change a channel slightly, even away from an edge.
                for (channel in listOf(Color::red, Color::green, Color::blue)) {
                    assertTrue("orientation $orientation corner $corner: expected $color, got $actual",
                        kotlin.math.abs(channel(color) - channel(actual)) <= 8)
                }
            }
        }
    }

    @Test fun theClientIsReusedUntilMemoryPressure() {
        var hatches = 0
        TextDetectionModule.optionsHatch = { assertSame(Looper.getMainLooper(), Looper.myLooper()); hatches++ }
        call(); call()
        assertEquals(1, Recognition.made)
        assertTrue(hatches > 0)
        val memory = ReflectionHelpers.getField<ComponentCallbacks2>(module, "memory")
        memory.onLowMemory()
        module.work.submit {}.get(10, TimeUnit.SECONDS)
        assertEquals(1, Recognition.closed)
        call()
        assertEquals(2, Recognition.made)
        assertEquals(3, Recognition.processed)
    }

    @Test fun theAgentRunsRecognitionWithoutCallingAHatch() {
        module.destroy {}
        assertTrue(module.work.awaitTermination(10, TimeUnit.SECONDS))
        Host.agent = true
        module = TextDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "text-detection"))
        TextDetectionModule.optionsHatch = { throw AssertionError("agent called the hatch") }
        assertEquals(0, call().getJSONArray("texts").length())
        assertEquals(1, Recognition.processed)
    }
}
