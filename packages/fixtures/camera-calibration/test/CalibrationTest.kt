package dev.exact.canvas

import android.graphics.Rect
import android.hardware.camera2.CameraCharacteristics
import android.util.Size
import android.util.SizeF
import androidx.camera.camera2.interop.Camera2CameraInfo
import androidx.camera.core.Camera
import androidx.camera.core.CameraInfo
import fixture.calibration.CalibrationModule
import java.lang.reflect.Proxy
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadow.api.Shadow
import org.robolectric.util.ReflectionHelpers

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE,
    shadows = [CalibrationTest.Host::class, CalibrationTest.Info::class, CalibrationTest.Factory::class])
class CalibrationTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
        }
    }

    @Implements(Camera2CameraInfo::class, isInAndroidSdk = false)
    class Info {
        @Implementation fun <T> getCameraCharacteristic(key: CameraCharacteristics.Key<T>): T? {
            @Suppress("UNCHECKED_CAST")
            return values[key] as T?
        }
    }

    @Implements(className = "androidx.camera.camera2.interop.Camera2CameraInfo\$Companion", isInAndroidSdk = false)
    class Factory {
        @Implementation fun from(info: CameraInfo): Camera2CameraInfo = Shadow.newInstanceOf(Camera2CameraInfo::class.java).also {
            ReflectionHelpers.setField(it, "cameraId", "back")
        }
    }

    companion object { val values = HashMap<CameraCharacteristics.Key<*>, Any>() }

    private val context = ExtensionContext(RuntimeEnvironment.getApplication(), "camera-calibration")
    private val module = CalibrationModule(context)
    private val cameraInfo = Proxy.newProxyInstance(CameraInfo::class.java.classLoader, arrayOf(CameraInfo::class.java)) {
        _, method, _ -> error("Unexpected CameraInfo call: ${method.name}")
    } as CameraInfo
    private val camera = Proxy.newProxyInstance(Camera::class.java.classLoader, arrayOf(Camera::class.java)) {
        _, method, _ -> if (method.name == "getCameraInfo") cameraInfo else error(method.name)
    } as Camera

    @Before fun open() {
        values.clear()
        context.tracks.add(TrackEntry("calibration-track", "video").apply { video = { object : VideoSource {
            override val camera: Any = this@CalibrationTest.camera
            override fun add(useCase: Any) {}
            override fun attach(useCase: Any) = true
            override fun remove(useCase: Any) {}
            override fun <T : Any> shared(key: String, make: () -> T): T = make()
            override fun apply(constraints: JSONObject) {}
        } } })
    }

    @After fun close() { context.tracks.remove("calibration-track"); values.clear() }

    private fun read(track: String = "calibration-track"): JSONObject {
        var answer: JSONObject? = null
        module.later("getCameraCalibration", JSONObject().put("track", track)) { answer = it }
        return answer!!
    }

    @Test fun readsTheOpenTracksCameraAndPreservesItsReferenceFrame() {
        values[CameraCharacteristics.LENS_INTRINSIC_CALIBRATION] = floatArrayOf(500f, 510f, 320f, 240f, 2f)
        values[CameraCharacteristics.LENS_DISTORTION] = floatArrayOf(1f, 2f, 3f, 4f, 5f)
        values[CameraCharacteristics.LENS_INFO_AVAILABLE_FOCAL_LENGTHS] = floatArrayOf(4f, 8f)
        values[CameraCharacteristics.SENSOR_INFO_PHYSICAL_SIZE] = SizeF(6f, 4f)
        values[CameraCharacteristics.SENSOR_INFO_PIXEL_ARRAY_SIZE] = Size(4200, 3200)
        values[CameraCharacteristics.SENSOR_INFO_PRE_CORRECTION_ACTIVE_ARRAY_SIZE] = Rect(10, 20, 4010, 3020)
        val answer = read()
        assertEquals("", answer.getString("reason"))
        assertEquals("back", answer.getString("deviceId"))
        assertEquals("pre-correction-active-array", answer.getString("referenceFrame"))
        assertEquals("[500,2,320,0,510,240,0,0,1]", answer.getJSONArray("intrinsicMatrix").toString())
        assertEquals("[1,2,3,4,5]", answer.getJSONArray("distortion").toString())
        assertEquals("[4,8]", answer.getJSONArray("focalLengths").toString())
        assertEquals("[6,4]", answer.getJSONArray("sensorSize").toString())
        assertEquals("[4200,3200]", answer.getJSONArray("pixelArraySize").toString())
        assertEquals("[10,20,4010,3020]", answer.getJSONArray("preCorrectionActiveArray").toString())
        assertEquals("[4000,3000]", answer.getJSONArray("referenceDimensions").toString())
    }

    @Test fun absentCharacteristicsAreEmptyWithAReasonAndPartialDataIsKept() {
        val absent = read()
        assertEquals("intrinsics-not-reported", absent.getString("reason"))
        for (key in listOf("intrinsicMatrix", "distortion", "sensorSize", "pixelArraySize")) assertEquals(0, absent.getJSONArray(key).length())
        values[CameraCharacteristics.LENS_INFO_AVAILABLE_FOCAL_LENGTHS] = floatArrayOf(4f)
        assertEquals("[4]", read().getJSONArray("focalLengths").toString())
        assertEquals("track-not-running", read("missing").getString("reason"))
    }
}
