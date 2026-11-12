package exact.barcodedetection

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Point
import android.graphics.Rect
import android.media.ExifInterface
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.barcode.common.internal.BarcodeSource
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
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.lang.reflect.Proxy

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [BarcodeDetectionTest.Host::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class BarcodeDetectionTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = true
            @JvmStatic @Implementation fun changed(topic: String) {}
            @JvmStatic @Implementation fun now() = 125.0
        }
    }

    private lateinit var module: BarcodeDetectionModule
    @Before fun setUp() { module = BarcodeDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "barcode-detection")) }
    @After fun tearDown() { module.destroy {} }

    private fun call(name: String, args: JSONObject = JSONObject()): JSONObject {
        var result: JSONObject? = null
        module.later(name, args) { result = it }
        return requireNotNull(result)
    }

    @Test fun androidHasABundledDetectorForEveryDeclaredFormat() {
        for (name in listOf("getSupportedFormats", "getSupportedFormatsSync")) {
            val answer = call(name)
            assertTrue(answer.getBoolean("tracks"))
            assertTrue(answer.getJSONObject("android").getBoolean("bundled"))
            val formats = answer.getJSONArray("formats")
            assertEquals(setOf("aztec", "code_128", "code_39", "code_93", "codabar", "data_matrix", "ean_13", "ean_8",
                "itf", "pdf417", "qr_code", "upc_a", "upc_e"), (0 until formats.length()).map { formats.getString(it) }.toSet())
        }
    }

    @Test fun formatsValidateAndBothUpcShapesAreRequestedTogether() {
        for (bad in listOf(JSONArray(), JSONArray().put("unknown"), JSONArray().put(4), "qr_code")) {
            assertEquals("TypeError", BarcodeDetectionModule.asked(bad).second!!.getString("error"))
        }
        assertNull(BarcodeDetectionModule.asked(JSONObject.NULL).second)
        assertNull(BarcodeDetectionModule.requestedFormats(null))
        for (name in listOf("upc_a", "ean_13")) {
            assertEquals(setOf(Barcode.FORMAT_UPC_A, Barcode.FORMAT_EAN_13), BarcodeDetectionModule.requestedFormats(setOf(name))!!.toSet())
        }
        for (name in BarcodeDetectionModule.formats) {
            assertTrue(BarcodeDetectionModule.requestedFormats(setOf(name))!!.any { BarcodeDetectionModule.format(it) == name })
        }
        assertEquals("unknown", BarcodeDetectionModule.format(Barcode.FORMAT_UNKNOWN))
    }

    @Test fun upcResultsNormaliseBeforeFilteringWithoutLosingTheirGeometry() {
        for ((format, value) in listOf("upc_a" to "012345678905", "ean_13" to "0012345678905")) {
            for (keep in listOf(null, setOf("upc_a"), setOf("ean_13"), setOf("qr_code"))) {
                val code = JSONObject().put("format", format).put("rawValue", value).put("boundingBox", JSONObject().put("x", 8))
                val found = BarcodeDetectionModule.normalised(listOf(code), keep)
                if (keep == setOf("qr_code")) assertTrue(found.isEmpty()) else {
                    assertEquals(if (keep == setOf("ean_13")) "ean_13" else "upc_a", found.single().getString("format"))
                    assertEquals(if (keep == setOf("ean_13")) "0012345678905" else "012345678905", found.single().getString("rawValue"))
                    assertEquals(8, found.single().getJSONObject("boundingBox").getInt("x"))
                    assertEquals(format, code.getString("format"))
                    assertEquals(value, code.getString("rawValue"))
                }
            }
        }
        val isbn = JSONObject().put("format", "ean_13").put("rawValue", "9780747532743")
        assertTrue(BarcodeDetectionModule.normalised(listOf(isbn), setOf("upc_a")).isEmpty())
    }

    @Test fun barcodeShapeKeepsUprightPixelsAndBinaryPayload() {
        val source = Proxy.newProxyInstance(BarcodeSource::class.java.classLoader, arrayOf(BarcodeSource::class.java)) { _, method, _ ->
            when (method.name) {
                "getFormat" -> Barcode.FORMAT_PDF417
                "getValueType" -> Barcode.TYPE_DRIVER_LICENSE
                "getRawValue" -> "fixture"
                "getRawBytes" -> byteArrayOf(0, 1, -1)
                "getBoundingBox" -> Rect(12, 24, 52, 44)
                "getCornerPoints" -> arrayOf(Point(12, 24), Point(52, 24), Point(52, 44), Point(12, 44))
                else -> null
            }
        } as BarcodeSource
        val shape = BarcodeDetectionModule.shape(Barcode(source))
        assertEquals("fixture", shape.getString("rawValue"))
        assertEquals("pdf417", shape.getString("format"))
        val box = shape.getJSONObject("boundingBox")
        assertEquals(listOf(12, 24, 40, 20), listOf("x", "y", "width", "height").map(box::getInt))
        assertEquals(listOf(12, 52, 52, 12), (0..3).map { shape.getJSONArray("cornerPoints").getJSONObject(it).getInt("x") })
        assertEquals("AAH/", shape.getJSONObject("android").getString("rawBytes"))
        assertEquals("driver_license", shape.getJSONObject("android").getString("valueType"))
    }

    @Test fun emptyUnknownAndNonVideoTracksHaveTheDeclaredAnswers() {
        assertEquals("idle", call("detectedBarcodes").getString("state"))
        assertEquals("NotFoundError", call("detectedBarcodes", JSONObject().put("track", "absent")).getString("error"))
        module.context.tracks.add(dev.exact.canvas.TrackEntry("audio-test", "audio"))
        try {
            assertEquals("NotSupportedError", call("detectedBarcodes", JSONObject().put("track", "audio-test")).getString("error"))
        } finally { module.context.tracks.remove("audio-test") }
        assertEquals("NotFoundError", call("detectBarcodes", JSONObject().put("image", "app:/absent.jpg")).getString("error"))
    }

    @Test fun releasingAScanRemovesOnlyItsObserverAndDestroyRemovesTheRest() {
        val observers = LinkedHashSet<dev.exact.canvas.TrackConsumer>()
        val owner = object : dev.exact.canvas.TrackOwner {
            override fun subscribe(id: String, consumer: dev.exact.canvas.TrackConsumer, start: Boolean, origin: String): () -> Unit =
                throw AssertionError("A barcode scan must never consume the track")
            override fun observe(id: String, observer: dev.exact.canvas.TrackConsumer): () -> Unit {
                observers += observer
                observer.liveVideo(null)
                return { observers -= observer }
            }
        }
        module.context.tracks.add(dev.exact.canvas.TrackEntry("scan-test", "video", owner = owner))
        try {
            val qr = JSONObject().put("track", "scan-test").put("formats", JSONArray().put("qr_code"))
            val pdf = JSONObject().put("track", "scan-test").put("formats", JSONArray().put("pdf417"))
            assertEquals("scanning", call("detectedBarcodes", qr).getString("state"))
            call("detectedBarcodes", qr)
            call("detectedBarcodes", pdf)
            assertEquals(2, observers.size)
            module.release("detectedBarcodes", qr)
            assertEquals(1, observers.size)
            module.destroy {}
            assertTrue(observers.isEmpty())
        } finally { module.context.tracks.remove("scan-test") }
    }

    @Test fun aStillIsDecodedUprightForEveryExifOrientation() {
        val file = java.io.File.createTempFile("barcode-orientation", ".jpg", RuntimeEnvironment.getApplication().cacheDir)
        val bitmap = Bitmap.createBitmap(80, 40, Bitmap.Config.ARGB_8888)
        bitmap.eraseColor(Color.RED)
        for (y in 0 until 40) for (x in 40 until 80) bitmap.setPixel(x, y, Color.BLUE)
        try {
            file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, 100, it) }
            for (orientation in 1..8) {
                ExifInterface(file.path).apply {
                    setAttribute(ExifInterface.TAG_ORIENTATION, orientation.toString()); saveAttributes()
                }
                val upright = requireNotNull(module.upright(file.path))
                assertEquals(if (orientation >= 5) 40 else 80, upright.width)
                assertEquals(if (orientation >= 5) 80 else 40, upright.height)
                val blueFirst = orientation in listOf(2, 3, 7, 8)
                val pixel = upright.getPixel(10, 10)
                assertEquals("orientation $orientation", blueFirst, Color.blue(pixel) > Color.red(pixel))
                upright.recycle()
            }
            file.writeText("not an image")
            assertNull(module.upright(file.path))
        } finally { bitmap.recycle(); file.delete() }
    }
}
