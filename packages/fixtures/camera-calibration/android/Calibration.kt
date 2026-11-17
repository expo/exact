package fixture.calibration

import android.hardware.camera2.CameraCharacteristics as Keys
import androidx.camera.camera2.interop.Camera2CameraInfo
import androidx.camera.camera2.interop.ExperimentalCamera2Interop
import androidx.camera.core.Camera
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject

class CalibrationModule(context: ExtensionContext) : ExactExtension(context) {
    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        if (name != "getCameraCalibration") return reply(JSONObject().put("error", "NotSupportedError"))
        val track = context.tracks.lookup(args.optString("track"))
        val camera = track?.video?.invoke()?.camera as? Camera
        reply(if (camera == null) empty("track-not-running") else read(camera))
    }

    companion object {
        fun empty(reason: String) = JSONObject().apply {
            put("reason", reason)
            put("deviceId", "")
            put("referenceFrame", "")
            for (key in listOf("intrinsicMatrix", "distortion", "focalLengths", "sensorSize",
                "pixelArraySize", "preCorrectionActiveArray", "referenceDimensions")) put(key, JSONArray())
        }

        @OptIn(ExperimentalCamera2Interop::class)
        fun read(camera: Camera): JSONObject {
            val info = try { Camera2CameraInfo.from(camera.cameraInfo) }
                catch (_: IllegalArgumentException) { return empty("camera2-unavailable") }
            val k = info.getCameraCharacteristic(Keys.LENS_INTRINSIC_CALIBRATION)
            val distortion = info.getCameraCharacteristic(Keys.LENS_DISTORTION)
            val focal = info.getCameraCharacteristic(Keys.LENS_INFO_AVAILABLE_FOCAL_LENGTHS)
            val sensor = info.getCameraCharacteristic(Keys.SENSOR_INFO_PHYSICAL_SIZE)
            val pixels = info.getCameraCharacteristic(Keys.SENSOR_INFO_PIXEL_ARRAY_SIZE)
            val active = info.getCameraCharacteristic(Keys.SENSOR_INFO_PRE_CORRECTION_ACTIVE_ARRAY_SIZE)
            return empty(if (k?.size == 5) "" else "intrinsics-not-reported").apply {
                put("deviceId", info.cameraId)
                put("referenceFrame", "pre-correction-active-array")
                if (k?.size == 5) put("intrinsicMatrix", JSONArray(listOf(
                    k[0], k[4], k[2], 0f, k[1], k[3], 0f, 0f, 1f)))
                if (distortion != null) put("distortion", JSONArray(distortion.toList()))
                if (focal != null) put("focalLengths", JSONArray(focal.toList()))
                if (sensor != null) put("sensorSize", JSONArray(listOf(sensor.width, sensor.height)))
                if (pixels != null) put("pixelArraySize", JSONArray(listOf(pixels.width, pixels.height)))
                if (active != null) {
                    put("preCorrectionActiveArray", JSONArray(listOf(active.left, active.top, active.right, active.bottom)))
                    put("referenceDimensions", JSONArray(listOf(active.width(), active.height())))
                }
            }
        }
    }
}
