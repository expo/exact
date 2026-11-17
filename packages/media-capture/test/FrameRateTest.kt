@file:androidx.annotation.OptIn(androidx.camera.camera2.interop.ExperimentalCamera2Interop::class)

package exact.mediacapture

import android.Manifest
import android.content.pm.PackageManager
import android.util.Range
import android.util.Size
import androidx.camera.camera2.interop.Camera2CameraInfo
import androidx.camera.core.Camera
import androidx.camera.core.CameraControl
import androidx.camera.core.CameraInfo
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.SessionConfig
import androidx.camera.core.ZoomState
import androidx.camera.core.UseCase
import androidx.camera.core.impl.CameraInfoInternal
import androidx.camera.core.impl.StreamSpec
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.MutableLiveData
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import dev.exact.canvas.TrackConsumer
import dev.exact.canvas.VideoSource
import com.google.common.util.concurrent.Futures
import java.lang.reflect.Proxy
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
import org.robolectric.shadows.ShadowLog
import org.robolectric.shadow.api.Shadow
import org.robolectric.util.ReflectionHelpers

// Run the real track and lease path, replacing only CameraX's hardware boundary.
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE,
    shadows = [Host::class, FrameRateTest.Provider::class, FrameRateTest.Camera2Info::class, FrameRateTest.Camera2Factory::class])
class FrameRateTest {
    @Implements(Camera2CameraInfo::class, isInAndroidSdk = false)
    class Camera2Info {
        @Implementation fun getCameraId() = "0"
    }

    @Implements(className = "androidx.camera.camera2.interop.Camera2CameraInfo\$Companion", isInAndroidSdk = false)
    class Camera2Factory {
        @Implementation fun from(info: CameraInfo): Camera2CameraInfo = Shadow.newInstanceOf(Camera2CameraInfo::class.java)
    }

    @Implements(ProcessCameraProvider::class, isInAndroidSdk = false)
    class Provider {
        @Implementation fun getAvailableCameraInfos(): List<CameraInfo> = listOf(info)
        @Implementation fun unbindAll() {
            unbinds++
            zoom = 1f
            torch = false
            bound = null
        }
        @Implementation fun bindToLifecycle(owner: LifecycleOwner, selector: CameraSelector, vararg cases: UseCase): Camera {
            attempts++
            bound = null
            negotiated(cases.toList(), Range(2, 30))
            return camera
        }
        @Implementation fun bindToLifecycle(owner: LifecycleOwner, selector: CameraSelector, config: SessionConfig): Camera {
            attempts++
            check(!refuseRate) { "Unsupported frame rate" }
            bound = config
            negotiated(config.useCases, config.frameRateRange)
            return camera
        }

        private fun negotiated(cases: List<UseCase>, rate: Range<Int>) {
            for (case in cases) ReflectionHelpers.setField(case, "mAttachedStreamSpec",
                StreamSpec.builder(Size(640, 480)).setExpectedFrameRateRange(rate).build())
        }

        companion object {
            val ranges = setOf(Range(2, 15), Range(15, 15), Range(2, 30), Range(30, 30))
            var deviceRanges = ranges
            var sessionRanges = ranges
            var failQuery = false
            var zoom = 1f
            var torch = false
            var unbinds = 0
            var bound: SessionConfig? = null
            var refuseRate = false
            var attempts = 0
            val info = Proxy.newProxyInstance(CameraInfoInternal::class.java.classLoader,
                arrayOf(CameraInfoInternal::class.java)) { proxy, method, args ->
                when (method.name) {
                    "getLensFacing" -> CameraSelector.LENS_FACING_BACK
                    "hasFlashUnit" -> true
                    "getZoomState" -> MutableLiveData(object : ZoomState {
                        override fun getZoomRatio() = zoom
                        override fun getMinZoomRatio() = 1f
                        override fun getMaxZoomRatio() = 4f
                        override fun getLinearZoom() = (zoom - 1) / 3
                    })
                    "getTorchState" -> MutableLiveData(if (torch) 1 else 0)
                    "getCameraState" -> MutableLiveData<Any>()
                    "getSupportedFrameRateRanges" -> if (args.isNullOrEmpty()) deviceRanges else {
                        check(!failQuery) { "Session query failed" }
                        sessionRanges
                    }
                    "hashCode" -> System.identityHashCode(proxy)
                    "equals" -> proxy === args?.get(0)
                    "toString" -> "Test camera"
                    else -> error("Unexpected CameraInfo call: ${method.name}")
                }
            } as CameraInfo
            val control = Proxy.newProxyInstance(CameraControl::class.java.classLoader, arrayOf(CameraControl::class.java)) { _, method, args ->
                when (method.name) {
                    "setZoomRatio" -> zoom = args!![0] as Float
                    "enableTorch" -> torch = args!![0] as Boolean
                    else -> error("Unexpected CameraControl call: ${method.name}")
                }
                Futures.immediateFuture<Void?>(null)
            } as CameraControl
            val camera = Proxy.newProxyInstance(Camera::class.java.classLoader, arrayOf(Camera::class.java)) { _, method, _ ->
                when (method.name) {
                    "getCameraInfo" -> info
                    "getCameraControl" -> control
                    else -> error("Unexpected Camera call: ${method.name}")
                }
            } as Camera
        }
    }

    private lateinit var context: ExtensionContext
    private lateinit var module: MediaCaptureModule
    private val preview by lazy { Preview.Builder().build() }

    @Before fun setUp() {
        val app = RuntimeEnvironment.getApplication()
        shadowOf(app.packageManager).setSystemFeature(PackageManager.FEATURE_CAMERA_ANY, true)
        shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).requestedPermissions = arrayOf(Manifest.permission.CAMERA)
        shadowOf(app).grantPermissions(Manifest.permission.CAMERA)
        ReflectionHelpers.setField(CameraCapture, "ready", Shadow.newInstanceOf(ProcessCameraProvider::class.java))
        Provider.bound = null
        Provider.deviceRanges = Provider.ranges
        Provider.sessionRanges = Provider.ranges
        Provider.failQuery = false
        Provider.zoom = 1f
        Provider.torch = false
        Provider.unbinds = 0
        Provider.refuseRate = false
        Provider.attempts = 0
        context = ExtensionContext(app, "media-capture")
        module = MediaCaptureModule(context)
    }

    @After fun tearDown() {
        module.destroy {}
        ReflectionHelpers.setField(CameraCapture, "ready", null)
    }

    private fun call(name: String, args: JSONObject): JSONObject {
        var answer: JSONObject? = null
        module.later(name, args) { answer = it }
        return requireNotNull(answer)
    }

    private fun open(rate: Double? = null): String = call("getUserMedia", JSONObject().put("video", true)
        .put("videoConstraints", JSONObject().put("frameRate", rate)))
        .getJSONArray("tracks").getJSONObject(0).getString("id")

    private fun show(id: String) = context.tracks.subscribe(id, object : TrackConsumer {
        override fun liveVideo(source: VideoSource?) { requireNotNull(source).add(preview) }
        override fun muted(reason: String) {}
        override fun unmuted() {}
        override fun ended() {}
    }, false, "app")

    private fun state(id: String) = call("mediaStreamTrack", JSONObject().put("track", id))

    private fun apply(id: String, rate: Double) = call("applyConstraints", JSONObject().put("track", id).put("frameRate", rate))

    @Test fun anAuthoredFrameRateReachesTheSessionAndItsTrackSettings() {
        val id = open(15.0)
        show(id)
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
        val track = state(id)
        assertEquals(15, track.getJSONObject("settings").getInt("frameRate"))
        val capability = track.getJSONObject("capabilities").getJSONObject("frameRate")
        assertEquals(2, capability.getInt("min"))
        assertEquals(30, capability.getInt("max"))
    }

    @Test fun anUnavailableIdealUsesTheClosestSessionRate() {
        val id = open(60.0)
        show(id)
        assertEquals(Range(30, 30), Provider.bound?.frameRateRange)
        assertEquals(30, state(id).getJSONObject("settings").getInt("frameRate"))
        apply(id, 24.0)
        assertEquals(Range(30, 30), Provider.bound?.frameRateRange)
        apply(id, 15.0)
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
    }

    @Test fun everyOutputAndLeaseUsesTheCapturesRate() {
        val first = open(15.0)
        val leave = show(first)
        val second = open(30.0)
        show(second)
        assertEquals(15, state(second).getJSONObject("settings").getInt("frameRate"))
        val analysis = ImageAnalysis.Builder().build()
        assertTrue(CameraCapture.attach(analysis))
        assertEquals(setOf(preview, analysis), Provider.bound?.useCases?.toSet())
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
        apply(second, 30.0)
        for (id in listOf(first, second)) assertEquals(30, state(id).getJSONObject("settings").getInt("frameRate"))
        leave()
        CameraCapture.remove(analysis)
        assertEquals(Range(30, 30), Provider.bound?.frameRateRange)
    }

    @Test fun theFirstLeasesDepartureDoesNotChangeTheRemainingCapturesRate() {
        val leave = show(open(15.0))
        val second = open(30.0)
        show(second)
        leave()
        assertTrue(CameraCapture.attach(ImageAnalysis.Builder().build()))
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
        assertEquals(15, state(second).getJSONObject("settings").getInt("frameRate"))
    }

    @Test fun anIdleConstraintIsUsedWhenItsConsumerReturns() {
        val id = open(30.0)
        val leave = show(id)
        leave()
        val before = Provider.attempts
        apply(id, 15.0)
        assertEquals(before, Provider.attempts)
        show(id)
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
        assertEquals(15, state(id).getJSONObject("settings").getInt("frameRate"))
    }

    @Test fun anUnsaidRateLeavesCameraXItsDefault() {
        val id = open()
        show(id)
        assertNull(Provider.bound)
        assertEquals(30, state(id).getJSONObject("settings").getInt("frameRate"))
        // A later lease does not override an unsaid first lease's default.
        val second = open(15.0)
        show(second)
        assertNull(Provider.bound)
        assertEquals(30, state(second).getJSONObject("settings").getInt("frameRate"))
    }

    @Test fun aRefusedRateReportsTheFallbackInsteadOfTheRequest() {
        val id = open(15.0)
        Provider.refuseRate = true
        show(id)
        assertNull(Provider.bound)
        assertEquals(30, state(id).getJSONObject("settings").getInt("frameRate"))
        Provider.refuseRate = false
        apply(id, 15.0)
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
        assertEquals(15, state(id).getJSONObject("settings").getInt("frameRate"))
    }

    @Test fun sessionSupportTakesPrecedenceOverTheCamerasFullRangeList() {
        Provider.sessionRanges = setOf(Range(15, 30))
        val id = open(15.0)
        show(id)
        assertEquals(Range(15, 30), Provider.bound?.frameRateRange)
        assertEquals(30, state(id).getJSONObject("settings").getInt("frameRate"))
        // Attaching another output re-evaluates the whole session.
        Provider.sessionRanges = setOf(Range(15, 15))
        assertTrue(CameraCapture.attach(ImageAnalysis.Builder().build()))
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
        assertEquals(15, state(id).getJSONObject("settings").getInt("frameRate"))
    }

    @Test fun aRefusedCameraMoveDoesNotChangeTheFrameRateRequest() {
        val id = open(15.0)
        show(id)
        context.origin = "tool"
        val result = call("applyConstraints", JSONObject().put("track", id).put("facingMode", "user").put("frameRate", 30))
        assertEquals("NotAllowedError", result.getString("error"))
        assertTrue(CameraCapture.attach(ImageAnalysis.Builder().build()))
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
    }

    @Test fun frameRateAndOutputRebindsRestoreTheSharedZoomAndTorch() {
        val first = open(30.0)
        show(first)
        val second = open()
        show(second)
        call("applyConstraints", JSONObject().put("track", first).put("zoom", 2).put("torch", true))
        val before = Provider.unbinds
        apply(second, 15.0)
        assertEquals(before + 1, Provider.unbinds)
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
        assertEquals(2f, Provider.zoom)
        assertTrue(Provider.torch)
        val analysis = ImageAnalysis.Builder().build()
        assertTrue(CameraCapture.attach(analysis))
        CameraCapture.remove(analysis)
        // A refused rate's default bind must restore the controls too.
        Provider.refuseRate = true
        apply(second, 30.0)
        assertNull(Provider.bound)
        for (id in listOf(first, second)) {
            val settings = state(id).getJSONObject("settings")
            assertEquals(2.0, settings.getDouble("zoom"), 0.0)
            assertTrue(settings.getBoolean("torch"))
        }
    }

    @Test fun equivalentFrameRateRequestsDoNotUnbindTheSession() {
        val id = open(30.0)
        show(id)
        val before = Provider.unbinds
        val attempts = Provider.attempts
        apply(id, 30.0)
        apply(id, 29.97)
        apply(id, 60.0)
        assertEquals(before, Provider.unbinds)
        assertEquals(attempts, Provider.attempts)
    }

    @Test fun legacyDeviceRangesSupplyAnEmptySessionQueryInFramesPerSecond() {
        Provider.deviceRanges = setOf(Range(15000, 15000), Range(15000, 30000))
        Provider.sessionRanges = emptySet()
        val id = open(15.0)
        show(id)
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
        val capability = state(id).getJSONObject("capabilities").getJSONObject("frameRate")
        assertEquals(15, capability.getInt("min"))
        assertEquals(30, capability.getInt("max"))
        apply(id, 30.0)
        assertEquals(Range(15, 30), Provider.bound?.frameRateRange)
        assertEquals(30, state(id).getJSONObject("settings").getInt("frameRate"))
    }

    @Test fun legacySessionRangesAreNormalizedBeforeSelection() {
        Provider.sessionRanges = setOf(Range(15000, 15000), Range(15000, 30000))
        show(open(15.0))
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
    }

    @Test fun aFailedSessionQueryIsLoggedAndUsesTheDeviceRanges() {
        Provider.failQuery = true
        show(open(15.0))
        assertEquals(Range(15, 15), Provider.bound?.frameRateRange)
        assertTrue(ShadowLog.getLogsForTag("exact").any { it.msg.contains("Session query failed") })
    }

    @Test fun capabilitiesAndSelectionUseTheCameraXLegacyUnitCorrection() {
        val ranges = listOf(Range(0, 0), Range(15000, 30000), Range(15, 30000))
        val capability = FrameRates.capability(ranges)!!
        assertEquals(15, capability.getInt("min"))
        assertEquals(30, capability.getInt("max"))
        assertEquals(Range(15, 30), FrameRates.choose(ranges, 30.0))
        assertNull(FrameRates.capability(listOf(Range(0, 0))))
    }

    @Test fun mediaCaptureFitnessChoosesSixtyForAFortyFourIdeal() {
        assertEquals(Range(60, 60), FrameRates.choose(listOf(Range(30, 30), Range(60, 60)), 44.0))
    }

    @Test fun rangeSelectionUsesIdealFitnessAndPrefersAFixedRate() {
        assertEquals(Range(15, 15), FrameRates.choose(Provider.ranges, 15.0))
        assertEquals(Range(30, 30), FrameRates.choose(Provider.ranges, 29.97))
        assertEquals(Range(30, 30), FrameRates.choose(Provider.ranges, 60.0))
        assertEquals(Range(15, 15), FrameRates.choose(Provider.ranges, 1.0))
        assertEquals(Range(15, 30), FrameRates.choose(listOf(Range(2, 30), Range(15, 30)), 30.0))
        assertNull(FrameRates.choose(emptyList(), 30.0))
        assertNull(FrameRates.choose(listOf(Range(0, 0)), 30.0))
    }

    @Test fun onlyFinitePositiveBareNumbersAreFrameRateIdeals() {
        assertEquals(15.0, FrameRates.requested(15))
        assertEquals(29.97, FrameRates.requested(29.97))
        for (value in listOf(null, JSONObject.NULL, "30", 0, -1, Double.NaN, Double.POSITIVE_INFINITY, JSONObject().put("exact", 15))) {
            assertNull(FrameRates.requested(value))
        }
        assertNull(FrameRates.capability(emptyList()))
    }
}
