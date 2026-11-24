package exact.mediacapture

import android.Manifest
import android.content.Context
import dev.exact.canvas.Cause
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class PermissionResumeTest {
    private val app get() = RuntimeEnvironment.getApplication()
    private lateinit var module: MediaCaptureModule
    private val answers = LinkedHashMap<String, JSONObject>()

    @Before fun setUp() {
        shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).requestedPermissions =
            arrayOf(Manifest.permission.CAMERA, Manifest.permission.RECORD_AUDIO)
        app.getSharedPreferences("exact.permissions", Context.MODE_PRIVATE).edit()
            .putBoolean(Manifest.permission.CAMERA, true).commit()
        app.getSharedPreferences("exact.media-capture", Context.MODE_PRIVATE).edit()
            .putBoolean("permanent", true).commit()
        module = MediaCaptureModule(ExtensionContext(app, "media-capture"))
        for (name in listOf("cameraStatus", "microphoneStatus")) answers[name] = read(name)
        Host.onChanged = { topic ->
            val name = topic.removePrefix("media-capture.")
            if (name in answers) answers[name] = read(name)
        }
    }

    @After fun tearDown() {
        Host.onChanged = {}
        module.destroy {}
    }

    private fun read(name: String): JSONObject {
        var answer: JSONObject? = null
        module.later(name, JSONObject(), null, Cause("app")) { answer = it }
        return requireNotNull(answer)
    }

    private fun state(name: String) = answers.getValue(name).getString("state")

    @Test fun cameraPermissionChangesInSettingsRefreshTheStatusOnResume() {
        assertEquals("denied", state("cameraStatus"))
        shadowOf(app).grantPermissions(Manifest.permission.CAMERA)
        module.resumed()
        assertEquals("granted", state("cameraStatus"))
        shadowOf(app).denyPermissions(Manifest.permission.CAMERA)
        module.resumed()
        assertEquals("prompt", state("cameraStatus"))
    }

    @Test fun microphonePermissionChangesInSettingsStillRefreshTheStatusOnResume() {
        assertEquals("denied", state("microphoneStatus"))
        shadowOf(app).grantPermissions(Manifest.permission.RECORD_AUDIO)
        module.resumed()
        assertEquals("granted", state("microphoneStatus"))
        shadowOf(app).denyPermissions(Manifest.permission.RECORD_AUDIO)
        module.resumed()
        assertEquals("prompt", state("microphoneStatus"))
    }
}
