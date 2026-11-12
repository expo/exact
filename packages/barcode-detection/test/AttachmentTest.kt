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
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun changed(topic: String) {}
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
            @JvmStatic @Implementation fun fromMediaImage(image: android.media.Image, rotation: Int): com.google.mlkit.vision.common.InputImage =
                org.robolectric.shadow.api.Shadow.newInstanceOf(com.google.mlkit.vision.common.InputImage::class.java)
        }
    }
    @Implements(BarcodeScanning::class, isInAndroidSdk = false)
    class Detectors {
        companion object {
            var made = 0
            var closed = 0
            val pending = ArrayList<com.google.android.gms.tasks.TaskCompletionSource<List<com.google.mlkit.vision.barcode.common.Barcode>>>()
            @JvmStatic @Implementation fun getClient(options: BarcodeScannerOptions): BarcodeScanner {
                made++
                return Proxy.newProxyInstance(BarcodeScanner::class.java.classLoader, arrayOf(BarcodeScanner::class.java)) { _, method, _ ->
                    if (method.name == "process") com.google.android.gms.tasks.TaskCompletionSource<List<com.google.mlkit.vision.barcode.common.Barcode>>()
                        .also { pending += it }.task else { if (method.name == "close") closed++; null }
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
        Display.angle = 0; Detectors.pending.clear(); Detectors.made = 0; Detectors.closed = 0
        ReflectionHelpers.setField(CameraCapture, "ready", org.robolectric.shadow.api.Shadow.newInstanceOf(ProcessCameraProvider::class.java))
        preview = Preview.Builder().build()
        CameraCapture.add(preview)
        module = BarcodeDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "barcode-detection"))
        module.context.tracks.add(TrackEntry("scan", "video", owner = object : TrackOwner {
            override fun subscribe(id: String, consumer: TrackConsumer, start: Boolean, origin: String): () -> Unit = error("not a lease")
            override fun observe(id: String, observer: TrackConsumer): () -> Unit {
                observers += observer
                observer.liveVideo(CameraCapture)
                return { observers -= observer }
            }
        }))
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

    @Test fun rejectedPhotoReachesCameraXsErrorCallback() {
        CameraCapture.add(ImageAnalysis.Builder().build())
        Provider.refusePhoto = true
        val photo = androidx.camera.core.ImageCapture.Builder().build()
        var error: androidx.camera.core.ImageCaptureException? = null
        CameraCapture.add(photo)
        photo.takePicture(java.util.concurrent.Executor { it.run() }, object : androidx.camera.core.ImageCapture.OnImageCapturedCallback() {
            override fun onError(exception: androidx.camera.core.ImageCaptureException) { error = exception }
        })
        shadowOf(Looper.getMainLooper()).idle()
        assertNotNull(error)
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
