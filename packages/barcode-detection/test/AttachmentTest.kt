package exact.barcodedetection

import android.os.Looper
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.UseCase
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.lifecycle.LifecycleOwner
import com.google.mlkit.vision.barcode.BarcodeScanner
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import dev.exact.canvas.*
import exact.mediacapture.CameraCapture
import exact.mediacapture.VideoTracks
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
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers
import java.lang.reflect.Proxy
import java.time.Duration

/** Real capture attachment with the hardware binder and ML Kit replaced. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [AttachmentTest.Host::class, AttachmentTest.Provider::class, AttachmentTest.Detectors::class, AttachmentTest.Display::class, AttachmentTest.Images::class])
class AttachmentTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            var agent = false
            val disclosed = ArrayList<JSONObject>()
            @JvmStatic @Implementation fun underAgent(name: String) = agent
            @JvmStatic @Implementation fun appFile(path: String) = java.io.File(RuntimeEnvironment.getApplication().cacheDir, "photo.jpg").path
            @JvmStatic @Implementation fun changed(topic: String) {
                if (topic.startsWith("exact-state:")) {
                    val report = JSONObject(topic.removePrefix("exact-state:"))
                    if (report.getString("extension") == "barcode-detection") disclosed += report.getJSONObject("state")
                }
            }
            @JvmStatic @Implementation fun now() = 100.0
        }
    }
    @Implements(ExtensionContext::class, isInAndroidSdk = false)
    class Display {
        companion object { var angle = 0 }
        @Implementation fun getOrientation() = angle
    }
    @Implements(com.google.mlkit.vision.common.InputImage::class, isInAndroidSdk = false)
    class Images {
        companion object {
            // Keep the bitmap input, without starting ML Kit's telemetry in this detector stand-in.
            @JvmStatic @Implementation fun fromBitmap(bitmap: android.graphics.Bitmap, rotation: Int): com.google.mlkit.vision.common.InputImage =
                ReflectionHelpers.callConstructor(com.google.mlkit.vision.common.InputImage::class.java,
                    ReflectionHelpers.ClassParameter.from(android.graphics.Bitmap::class.java, bitmap),
                    ReflectionHelpers.ClassParameter.from(Int::class.javaPrimitiveType, rotation))
            @JvmStatic @Implementation fun fromMediaImage(image: android.media.Image, rotation: Int): com.google.mlkit.vision.common.InputImage =
                org.robolectric.shadow.api.Shadow.newInstanceOf(com.google.mlkit.vision.common.InputImage::class.java)
        }
    }
    @Implements(BarcodeScanning::class, isInAndroidSdk = false)
    class Detectors {
        companion object {
            var made = 0
            var closed = 0
            var immediate = false
            val inputs = ArrayList<com.google.mlkit.vision.common.InputImage>()
            val pending = ArrayList<com.google.android.gms.tasks.TaskCompletionSource<List<com.google.mlkit.vision.barcode.common.Barcode>>>()
            @JvmStatic @Implementation fun getClient(options: BarcodeScannerOptions): BarcodeScanner {
                made++
                return Proxy.newProxyInstance(BarcodeScanner::class.java.classLoader, arrayOf(BarcodeScanner::class.java)) { _, method, args ->
                    if (method.name == "process" && immediate) com.google.android.gms.tasks.Tasks.forResult(emptyList<com.google.mlkit.vision.barcode.common.Barcode>())
                    else if (method.name == "process") com.google.android.gms.tasks.TaskCompletionSource<List<com.google.mlkit.vision.barcode.common.Barcode>>()
                        .also { pending += it; inputs += args!![0] as com.google.mlkit.vision.common.InputImage }.task else { if (method.name == "close") closed++; null }
                } as BarcodeScanner
            }
        }
    }
    @Implements(ProcessCameraProvider::class, isInAndroidSdk = false)
    class Provider {
        companion object {
            var bound = emptyList<UseCase>()
            var refuseAnalysis = false
            var refusePreview = false
            var refusePhoto = false
            var attempts = 0
        }
        @Implementation fun unbindAll() { bound = emptyList() }
        @Implementation fun bindToLifecycle(owner: LifecycleOwner, selector: CameraSelector, vararg cases: UseCase): Camera? {
            attempts++
            check(!refusePreview || cases.none { it is Preview }) { "No front camera" }
            check(!refusePhoto || cases.none { it is androidx.camera.core.ImageCapture }) { "Unsupported photo" }
            check(cases.count { it is ImageAnalysis } <= 1) { "Only one analysis output" }
            check(!refuseAnalysis || cases.none { it is ImageAnalysis }) { "Unsupported use cases" }
            bound = cases.toList()
            return null
        }
    }

    private lateinit var module: BarcodeDetectionModule
    private val observers = LinkedHashSet<TrackConsumer>()
    private lateinit var preview: Preview
    @Before fun setUp() {
        Provider.bound = emptyList(); Provider.refuseAnalysis = false
        Provider.refusePreview = false; Provider.refusePhoto = false; Provider.attempts = 0
        Host.agent = false; Host.disclosed.clear(); Display.angle = 0; Detectors.inputs.clear(); Detectors.pending.clear(); Detectors.made = 0; Detectors.closed = 0; Detectors.immediate = false
        ReflectionHelpers.setField(CameraCapture, "ready", org.robolectric.shadow.api.Shadow.newInstanceOf(ProcessCameraProvider::class.java))
        preview = Preview.Builder().build()
        CameraCapture.add(preview)
        module = BarcodeDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "barcode-detection"))
        module.context.tracks.add(TrackEntry("scan", "video", owner = object : TrackOwner {
            override fun subscribe(id: String, consumer: TrackConsumer, start: Boolean, origin: String): () -> Unit {
                consumer.liveVideo(if (Host.agent) null else CameraCapture)
                return {}
            }
            override fun observe(id: String, observer: TrackConsumer): () -> Unit {
                observers += observer
                observer.liveVideo(if (Host.agent) null else CameraCapture)
                return { observers -= observer }
            }
        }).apply {
            video = { CameraCapture }
            state = { JSONObject().put("readyState", "live").put("enabled", true).put("muted", false) }
        })
    }
    @After fun tearDown() {
        Provider.refuseAnalysis = false
        module.destroy {}
        module.context.tracks.remove("scan")
        ReflectionHelpers.getField<MutableSet<UseCase>>(CameraCapture, "useCases").clear()
        ReflectionHelpers.getField<MutableMap<String, Any>>(CameraCapture, "objects").clear()
        ReflectionHelpers.setField(CameraCapture, "ready", null)
    }
    private fun args(format: String = "qr_code", interval: Int = 100) = JSONObject().put("track", "scan")
        .put("formats", JSONArray().put(format)).put("interval", interval)
    private fun call(args: JSONObject): List<JSONObject> = ArrayList<JSONObject>().also { answers ->
        module.later("detectedBarcodes", args) { answers += it }
    }
    private fun complete(scan: Scan, value: String = "kept") {
        val poll = ReflectionHelpers.getField<Any>(scan, "pending")
        scan.javaClass.declaredMethods.single { it.name == "complete" }.apply { isAccessible = true }
            .invoke(scan, poll, listOf(JSONObject().put("format", "qr_code").put("rawValue", value)), 640, 480)
    }

    private fun disclosedScan(format: String) = Host.disclosed.last().getJSONArray("scans").let { scans ->
        (0 until scans.length()).map(scans::getJSONObject).single { it.getJSONArray("formats").getString(0) == format }
    }

    @Test fun successfulStillsDiscloseTheirPathAndCountWithABoundedHistory() {
        Host.agent = true
        module.destroy {}
        module = BarcodeDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "barcode-detection"))
        Detectors.immediate = true
        val file = java.io.File(module.context.application.cacheDir, "photo.jpg")
        val bitmap = android.graphics.Bitmap.createBitmap(80, 40, android.graphics.Bitmap.Config.ARGB_8888)
        val answers = ArrayList<JSONObject>()
        try {
            file.outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.JPEG, 100, it) }
            repeat(17) { index ->
                module.later("detectBarcodes", JSONObject().put("image", "app:/still-$index.jpg")) { answers += it }
            }
            module.work.shutdown()
            assertTrue(module.work.awaitTermination(10, java.util.concurrent.TimeUnit.SECONDS))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(17, answers.size)
            assertTrue(answers.all { it.getJSONArray("barcodes").length() == 0 })
            val stills = Host.disclosed.last().getJSONArray("stills")
            assertEquals(16, stills.length())
            assertEquals("app:/still-1.jpg", stills.getJSONObject(0).getString("path"))
            assertEquals("app:/still-16.jpg", stills.getJSONObject(15).getString("path"))
            assertEquals(0, stills.getJSONObject(15).getInt("count"))
        } finally { bitmap.recycle(); file.delete() }
    }

    @Test fun rejectedPreviewStillReturnsTheViewfindersUnsubscribe() {
        CameraCapture.remove(preview)
        Provider.refusePreview = true
        val videos = VideoTracks(module.context) { 1 }
        val track = VideoTracks.Track("front", "front-stream").apply { camera = "front" }
        ReflectionHelpers.getField<MutableMap<String, VideoTracks.Track>>(videos, "tracks")[track.id] = track
        val consumer = object : TrackConsumer {
            override fun liveVideo(source: VideoSource?) { source!!.add(preview) }
            override fun muted(reason: String) {}
            override fun unmuted() {}
            override fun ended() {}
        }
        try {
            val leave = videos.subscribe(track.id, consumer, false, "app")
            assertTrue(track.leased)
            leave()
            assertFalse(track.leased)
            assertTrue(track.consumers.isEmpty())
            assertTrue(CameraCapture.holders.isEmpty())
        } finally { videos.endAll("test ended"); Provider.refusePreview = false }
    }

    @Test fun refusedPhotoRestoresPreviewAndAnalysisAndIsNeverCached() {
        val first = call(args())
        val analysis = Provider.bound.filterIsInstance<ImageAnalysis>().single()
        val camera = exact.camera.CameraModule(ExtensionContext(RuntimeEnvironment.getApplication(), "camera"))
        Provider.refusePhoto = true
        val photos = ArrayList<JSONObject>()
        val photoArgs = JSONObject().put("track", "scan").put("android", JSONObject().put("shutterSound", true))
        repeat(2) {
            val attempts = Provider.attempts
            camera.later("takePhoto", photoArgs) { photos += it }
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals("UnknownError", photos.last().getString("error"))
            assertEquals(setOf(preview, analysis), Provider.bound.toSet())
            assertTrue(Provider.attempts > attempts)
            assertTrue(ReflectionHelpers.getField<Map<String, Any>>(CameraCapture, "objects").values.none { it is androidx.camera.core.ImageCapture })
        }
        val analyzer = ReflectionHelpers.getField<ImageAnalysis.Analyzer>(analysis, "mSubscribedAnalyzer")
        val (proxy, closes) = frame()
        analyzer.analyze(proxy)
        Detectors.pending.single().setResult(emptyList())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, closes())
        assertEquals(1, first.single().getInt("frame"))
        Provider.refusePhoto = false
        camera.later("takePhoto", photoArgs) { photos += it }
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, Provider.bound.count { it is androidx.camera.core.ImageCapture })
    }

    @Test fun agentPollsDetectTheDrawnPictureAndKeepHistoryWhenItChanges() {
        Host.agent = true
        module.destroy {}
        module = BarcodeDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "barcode-detection"))
        assertEquals("idle", call(args()).single().getString("state"))
        val created = disclosedScan("qr_code")
        assertEquals("scan", created.getString("track"))
        assertEquals(100.0, created.getDouble("interval"), 0.0)
        assertEquals("idle", created.getString("state"))
        assertEquals(0, created.getInt("frames"))
        assertEquals(0, created.getJSONArray("seen").length())
        val camera = exact.camera.CameraModule(ExtensionContext(RuntimeEnvironment.getApplication(), "camera"))
        val view = camera.view("exact-camera", "1", "finder", module.context.application,
            JSONObject().put("srcObject", "scan")) { _, _ -> } as android.widget.ImageView
        camera.bound("1", "finder", "srcObject", "app")
        camera.displayed("1", "finder", true, "app")
        fun bitmap() = (view.drawable as android.graphics.drawable.BitmapDrawable).bitmap
        try {
            val drawn = bitmap()
            assertEquals("scanning", disclosedScan("qr_code").getString("state"))
            val first = call(args("upc_a"))
            val concurrent = call(args("upc_a"))
            assertEquals(first.toString(), 1, Detectors.pending.size)
            assertSame(drawn, Detectors.inputs.single().bitmapInternal)
            val source = Proxy.newProxyInstance(com.google.mlkit.vision.barcode.common.internal.BarcodeSource::class.java.classLoader,
                arrayOf(com.google.mlkit.vision.barcode.common.internal.BarcodeSource::class.java)) { _, method, _ ->
                when (method.name) {
                    "getFormat" -> com.google.mlkit.vision.barcode.common.Barcode.FORMAT_EAN_13
                    "getValueType" -> com.google.mlkit.vision.barcode.common.Barcode.TYPE_PRODUCT
                    "getRawValue" -> "0012345678905"
                    else -> null
                }
            } as com.google.mlkit.vision.barcode.common.internal.BarcodeSource
            Detectors.pending.single().setResult(listOf(com.google.mlkit.vision.barcode.common.Barcode(source)))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(1, concurrent.single().getInt("frame"))
            assertEquals("012345678905", first.single().getJSONArray("seen").getJSONObject(0).getString("rawValue"))
            assertEquals(drawn.width, first.single().getInt("width"))
            assertEquals(drawn.height, first.single().getInt("height"))
            assertEquals(1, disclosedScan("upc_a").getInt("frames"))
            assertEquals("upc_a 012345678905", disclosedScan("upc_a").getJSONArray("seen").getString(0))
            camera.preferred(JSONObject().put("picture", JSONObject().put("path", "missing-fixture.png")))
            assertNotSame(drawn, bitmap())
            val second = call(args("upc_a"))
            assertSame(bitmap(), Detectors.inputs.last().bitmapInternal)
            Detectors.pending.last().setResult(emptyList())
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(2, second.single().getInt("frame"))
            assertEquals(0, second.single().getJSONArray("barcodes").length())
            assertEquals(1, second.single().getJSONArray("seen").length())
            assertEquals(2, disclosedScan("upc_a").getInt("frames"))
            val inFlight = call(args("upc_a"))
            camera.displayed("1", "finder", false, "app")
            assertEquals("idle", inFlight.single().getString("state"))
            Detectors.pending.last().setResult(emptyList())
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals("idle", call(args("upc_a")).single().getString("state"))
            assertEquals("idle", disclosedScan("upc_a").getString("state"))
            call(args("upc_a", interval = 250))
            module.release("detectedBarcodes", args("upc_a"))
            assertEquals(250.0, disclosedScan("upc_a").getDouble("interval"), 0.0)
            observers.forEach { it.ended() }
            assertEquals("ended", disclosedScan("upc_a").getString("state"))
            module.release("detectedBarcodes", args("upc_a", interval = 250))
            assertEquals(1, Host.disclosed.last().getJSONArray("scans").length())
            module.release("detectedBarcodes")
            assertEquals(0, Host.disclosed.last().getJSONArray("scans").length())
            assertEquals(Detectors.made, Detectors.closed)
        } finally { camera.destroyView(view) }
    }

    @Test fun aFailureReadingTheFrameStillClosesItAndAnswersThePoll() {
        val answer = call(args())
        val analysis = Provider.bound.filterIsInstance<ImageAnalysis>().single()
        val analyzer = ReflectionHelpers.getField<ImageAnalysis.Analyzer>(analysis, "mSubscribedAnalyzer")
        var closes = 0
        val proxy = Proxy.newProxyInstance(androidx.camera.core.ImageProxy::class.java.classLoader,
            arrayOf(androidx.camera.core.ImageProxy::class.java)) { _, method, _ ->
            when (method.name) {
                "getImage" -> throw IllegalStateException("frame unavailable")
                "close" -> { closes++; null }
                else -> null
            }
        } as androidx.camera.core.ImageProxy
        runCatching { analyzer.analyze(proxy) }
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, closes)
        assertEquals("frame unavailable", answer.single().getString("error"))
        assertTrue(Detectors.pending.isEmpty())
    }

    @Test fun onlyAChangedFormatUnionRebuildsTheDetector() {
        call(args())
        assertEquals(1, Detectors.made)
        call(args("pdf417"))
        assertEquals(2, Detectors.made)
        assertEquals(1, Detectors.closed)
        call(args().put("formats", JSONArray().put("qr_code").put("pdf417")))
        assertEquals(2, Detectors.made)
        module.release("detectedBarcodes", args())
        module.release("detectedBarcodes", args("pdf417"))
        assertEquals(2, Detectors.made)
        module.release("detectedBarcodes", args().put("formats", JSONArray().put("qr_code").put("pdf417")))
        assertEquals(2, Detectors.closed)
    }

    @Test fun twoFormatGroupsAttachOneAnalysisBesideThePreview() {
        val qr = call(args())
        val pdf = call(args("pdf417"))
        assertEquals(2, observers.size)
        assertEquals(1, Provider.bound.count { it is ImageAnalysis })
        assertTrue(Provider.bound.contains(preview))
        assertTrue(qr.isEmpty()); assertTrue(pdf.isEmpty())
        module.release("detectedBarcodes", args())
        assertEquals(1, Provider.bound.count { it is ImageAnalysis })
        module.release("detectedBarcodes", args("pdf417"))
        assertEquals(listOf(preview), Provider.bound)
    }

    @Test fun failedAnalysisBindRestoresPreviewAndAnswersAnErrorThenCanRetry() {
        Provider.refuseAnalysis = true
        val answer = call(args())
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(Scan.PATIENCE_MS))
        assertEquals(listOf(preview), Provider.bound)
        assertEquals("OperationError", answer.single().getString("error"))
        val attempts = Provider.attempts
        repeat(3) { assertEquals("idle", call(args()).single().getString("state")) }
        assertEquals(attempts, Provider.attempts)
        observers.single().muted("background")
        observers.single().unmuted()
        assertEquals("idle", call(args()).single().getString("state"))
        assertTrue(Provider.attempts > attempts)
        Provider.refuseAnalysis = false
        observers.single().muted("background")
        observers.single().unmuted()
        val retry = call(args())
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(Scan.PATIENCE_MS))
        assertEquals("scanning", retry.single().getString("state"))
        assertFalse(retry.single().has("error"))
        assertEquals(1, Provider.bound.count { it is ImageAnalysis })
        assertTrue(Provider.bound.contains(preview))
    }

    @Test fun removingOneIntervalGroupPreservesTheOtherReadersHistory() {
        val first = call(args())
        complete(observers.single() as Scan)
        assertEquals(1, first.single().getJSONArray("seen").length())
        val second = call(args(interval = 200))
        module.release("detectedBarcodes", args())
        assertEquals(1, observers.size)
        complete(observers.single() as Scan)
        assertEquals(2, second.single().getJSONArray("seen").getJSONObject(0).getInt("frames"))
        module.release("detectedBarcodes", args(interval = 200))
        assertTrue(observers.isEmpty())
    }

    @Test fun changingIntervalAndOriginalFormatArrayPreservesSeen() {
        call(args())
        complete(observers.single() as Scan)
        val next = args(interval = 250).put("formats", JSONArray().put("qr_code").put("qr_code"))
        val changed = call(next)
        module.release("detectedBarcodes", args())
        assertEquals(1, observers.size)
        complete(observers.single() as Scan, "new")
        assertEquals(2, changed.single().getJSONArray("seen").length())
        module.release("detectedBarcodes", next)
        assertTrue(observers.isEmpty())
    }

    @Test fun displayRotationReachesEveryLiveAnalysis() {
        Display.angle = 180
        call(args())
        assertEquals(android.view.Surface.ROTATION_180, Provider.bound.filterIsInstance<ImageAnalysis>().single().targetRotation)
        call(args("pdf417"))
        module.orientation(90)
        assertTrue(Provider.bound.filterIsInstance<ImageAnalysis>().isNotEmpty())
        Provider.bound.filterIsInstance<ImageAnalysis>().forEach { assertEquals(android.view.Surface.ROTATION_90, it.targetRotation) }
        module.orientation(270)
        Provider.bound.filterIsInstance<ImageAnalysis>().forEach { assertEquals(android.view.Surface.ROTATION_270, it.targetRotation) }
    }

    @Test fun unionChangesAndReleaseWaitForTheInFlightDetection() {
        call(args())
        val analysis = Provider.bound.filterIsInstance<ImageAnalysis>().single()
        val analyzer = ReflectionHelpers.getField<ImageAnalysis.Analyzer>(analysis, "mSubscribedAnalyzer")
        val (first, firstCloses) = frame()
        analyzer.analyze(first)
        call(args("pdf417"))
        call(args("aztec"))
        assertEquals(1, Detectors.made)
        assertEquals(0, Detectors.closed)
        Detectors.pending.single().setResult(emptyList())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, firstCloses())
        assertEquals(2, Detectors.made)
        assertEquals(1, Detectors.closed)
        val (last, lastCloses) = frame()
        analyzer.analyze(last)
        assertEquals(2, Detectors.pending.size)
        module.release("detectedBarcodes")
        assertEquals(1, Detectors.closed)
        assertEquals(0, lastCloses())
        Detectors.pending.last().setResult(emptyList())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, lastCloses())
        assertEquals(2, Detectors.closed)
        assertEquals(listOf(preview), Provider.bound)
    }

    private fun frame(): Pair<androidx.camera.core.ImageProxy, () -> Int> {
        val media = org.robolectric.shadow.api.Shadow.newInstanceOf(Class.forName("android.media.ImageReader\$SurfaceImage"))
        val info = Proxy.newProxyInstance(androidx.camera.core.ImageInfo::class.java.classLoader, arrayOf(androidx.camera.core.ImageInfo::class.java)) { _, method, _ ->
            if (method.name == "getRotationDegrees") 90 else null
        }
        var closes = 0
        val proxy = Proxy.newProxyInstance(androidx.camera.core.ImageProxy::class.java.classLoader, arrayOf(androidx.camera.core.ImageProxy::class.java)) { _, method, _ ->
            when (method.name) {
                "getImage" -> media
                "getImageInfo" -> info
                "getWidth" -> 640
                "getHeight" -> 480
                "close" -> { closes++; null }
                else -> null
            }
        } as androidx.camera.core.ImageProxy
        return proxy to { closes }
    }

    @Test fun oneDetectionFeedsBothFormatGroupsAndClosesTheFrame() {
        val qr = call(args())
        val pdf = call(args("pdf417"))
        val analysis = Provider.bound.filterIsInstance<ImageAnalysis>().single()
        val analyzer = ReflectionHelpers.getField<ImageAnalysis.Analyzer>(analysis, "mSubscribedAnalyzer")
        val (proxy, closes) = frame()
        analyzer.analyze(proxy)
        assertEquals(1, Detectors.pending.size)
        assertEquals(0, closes())
        val codes = listOf(com.google.mlkit.vision.barcode.common.Barcode.FORMAT_QR_CODE,
            com.google.mlkit.vision.barcode.common.Barcode.FORMAT_PDF417).map { format ->
            val source = Proxy.newProxyInstance(com.google.mlkit.vision.barcode.common.internal.BarcodeSource::class.java.classLoader,
                arrayOf(com.google.mlkit.vision.barcode.common.internal.BarcodeSource::class.java)) { _, method, _ ->
                when (method.name) {
                    "getFormat" -> format
                    "getValueType" -> com.google.mlkit.vision.barcode.common.Barcode.TYPE_TEXT
                    "getRawValue" -> "value-$format"
                    else -> null
                }
            } as com.google.mlkit.vision.barcode.common.internal.BarcodeSource
            com.google.mlkit.vision.barcode.common.Barcode(source)
        }
        Detectors.pending[0].setResult(codes)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, closes())
        assertEquals("qr_code", qr.single().getJSONArray("barcodes").getJSONObject(0).getString("format"))
        assertEquals("pdf417", pdf.single().getJSONArray("barcodes").getJSONObject(0).getString("format"))
        for (answer in listOf(qr.single(), pdf.single())) {
            assertEquals(1, answer.getJSONArray("seen").length())
            assertEquals(480, answer.getInt("width")); assertEquals(640, answer.getInt("height"))
        }
        val (idle, idleCloses) = frame()
        analyzer.analyze(idle)
        assertEquals(1, idleCloses())
        assertEquals(1, closes())
        assertEquals(1, Detectors.pending.size)
    }
}
