package exact.camera

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.MediaStore
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Extensions
import dev.exact.canvas.Files
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream

/** `takePhotoWithCameraApp` on Android (expo/exact#381): the system's camera app over a Robolectric activity standing
 * for the reader's, the camera app played by the test through the reader's `Files` provider. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [CameraAppTest.Host::class])
class CameraAppTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            var agent = false
            /** Where `app:/` names, as the reader's roots under its cache directory. */
            lateinit var root: File
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = agent
            @JvmStatic @Implementation fun appFile(path: String): String = File(root, path.removePrefix("app:/")).path
        }
    }

    /** An activity on which no app handles an intent. */
    class NoCameraApp : Activity() {
        @Deprecated("Activity's own") override fun startActivityForResult(intent: Intent, requestCode: Int) =
            throw android.content.ActivityNotFoundException()
    }

    private val application = RuntimeEnvironment.getApplication()
    private lateinit var activity: Activity
    private lateinit var provider: Files
    private val answers = ArrayList<JSONObject>()

    @Before fun show() {
        Host.agent = false
        Host.root = File(application.cacheDir, "app").apply { mkdirs() }
        provider = Robolectric.setupContentProvider(Files::class.java, "${application.packageName}.exact.files")
        shown(Activity::class.java)
    }

    private fun shown(type: Class<out Activity>) {
        activity = Robolectric.buildActivity(type).setup().get()
        Extensions.front = { activity }
        Extensions.started()
        Extensions.resumed()
    }

    @After fun leave() {
        Extensions.paused()
        Extensions.stopped(false)
        Extensions.front = { null }
    }

    private fun take(args: JSONObject = JSONObject(), tap: String? = null) =
        CameraModule(ExtensionContext(application, "camera")).let { module ->
            module.later("takePhotoWithCameraApp", args, tap?.let { JSONObject().put("tap", it) }) { answers += it }
            module
        }

    private fun jpeg(width: Int, height: Int): ByteArray =
        ByteArrayOutputStream().also { Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).compress(Bitmap.CompressFormat.JPEG, 90, it) }.toByteArray()

    /** The camera app's write: through the provider, as another app opens an `EXTRA_OUTPUT` URI. */
    private fun write(uri: Uri, bytes: ByteArray) =
        provider.openFile(uri, "w").use { pfd -> FileOutputStream(pfd.fileDescriptor).use { it.write(bytes) } }

    private fun started(): Pair<Intent, Int> {
        val made = shadowOf(activity).nextStartedActivityForResult ?: error("no activity started")
        return made.intent to made.requestCode
    }

    @Suppress("DEPRECATION")
    private fun output(intent: Intent): Uri = intent.getParcelableExtra(MediaStore.EXTRA_OUTPUT)!!

    /** An app that asks for no permission opens the camera app at once; the photo it writes through the one URI the
     * reader handed out replaces the answered path only once the person keeps it, and the URI is read-only after. */
    @Test fun theKeptPhotoReplacesThePathAndNoOtherFileIsWritable() {
        val visitor = File(Host.root, "data/visitor.jpg").apply { parentFile!!.mkdirs(); writeText("the last visitor") }
        take(JSONObject().put("path", "app:/data/visitor").put("facingMode", "user"))
        assertNull("no permission is asked", shadowOf(activity).lastRequestedPermission)
        val (intent, code) = started()
        assertEquals(MediaStore.ACTION_IMAGE_CAPTURE, intent.action)
        assertEquals(1, intent.getIntExtra("android.intent.extras.CAMERA_FACING", -1))
        assertTrue(intent.flags and Intent.FLAG_GRANT_WRITE_URI_PERMISSION != 0)
        val out = output(intent)
        assertEquals("content", out.scheme)
        assertFalse("staged outside the app's directories", out.path!!.startsWith(Host.root.canonicalPath))

        // Another of the app's files stays read-only, the destination among them.
        val other = Files.uri(application, visitor)!!
        assertThrows(SecurityException::class.java) { provider.openFile(other, "w") }
        val photo = jpeg(40, 30)
        write(out, photo)
        assertEquals("the last visitor", visitor.readText())

        Extensions.activityResult(code, Activity.RESULT_OK, null)
        val answer = answers.single()
        assertEquals("app:/data/visitor.jpg", answer.getString("path"))
        assertEquals("image/jpeg", answer.getString("type"))
        assertEquals(photo.size.toLong(), answer.getLong("size"))
        assertEquals(40, answer.getInt("width"))
        assertEquals(30, answer.getInt("height"))
        assertArrayEquals(photo, visitor.readBytes())
        assertThrows(Exception::class.java) { provider.openFile(out, "w") }
    }

    /** The person's cancel is `AbortError`, and leaves the answered path as it was and nothing staged. */
    @Test fun aCancelIsAbortErrorAndLeavesThePathAsItWas() {
        val visitor = File(Host.root, "tmp/camera/1.jpg").apply { parentFile!!.mkdirs(); writeText("kept") }
        take()
        val (intent, code) = started()
        val out = output(intent)
        write(out, jpeg(4, 3).copyOf(10))
        Extensions.activityResult(code, Activity.RESULT_CANCELED, null)
        assertEquals("AbortError", answers.single().getString("error"))
        assertEquals("kept", visitor.readText())
        assertFalse(File(out.path!!).exists())
    }

    /** With no camera app nothing starts, the call is `NotFoundError`, and the next call starts afresh. */
    @Test fun noCameraAppIsNotFoundError() {
        shown(NoCameraApp::class.java)
        take()
        assertEquals("NotFoundError", answers.single().getString("error"))
        shown(Activity::class.java)
        take()
        assertEquals(MediaStore.ACTION_IMAGE_CAPTURE, started().first.action)
    }

    /** An app that declares CAMERA (it also uses the viewfinder) and does not hold it asks first, as Android requires;
     * the camera app opens from the answer once the activity is resumed after the dialog (expo/exact#382). Refused, the
     * call is `NotAllowedError` and nothing starts. */
    @Test fun anAppThatDeclaresCameraAsksForItFirst() {
        shadowOf(application.packageManager).getInternalMutablePackageInfo(application.packageName).requestedPermissions = arrayOf(Manifest.permission.CAMERA)
        shadowOf(application).denyPermissions(Manifest.permission.CAMERA)
        take()
        val asked = shadowOf(activity).lastRequestedPermission
        assertArrayEquals(arrayOf(Manifest.permission.CAMERA), asked.requestedPermissions)
        assertEquals("android.content.pm.action.REQUEST_PERMISSIONS", started().first.action)
        Extensions.paused()
        shadowOf(application).grantPermissions(Manifest.permission.CAMERA)
        Extensions.permissionsResult(asked.requestCode, asked.requestedPermissions, intArrayOf(PackageManager.PERMISSION_GRANTED))
        assertNull(shadowOf(activity).peekNextStartedActivityForResult())
        Extensions.resumed()
        assertEquals(MediaStore.ACTION_IMAGE_CAPTURE, started().first.action)
        assertTrue(answers.isEmpty())

        shadowOf(application).denyPermissions(Manifest.permission.CAMERA)
        // A fresh activity: the platform's keeps its one request pending until its own dispatch, which this skips.
        shown(Activity::class.java)
        take()
        val again = shadowOf(activity).lastRequestedPermission
        started()
        Extensions.permissionsResult(again.requestCode, again.requestedPermissions, intArrayOf(PackageManager.PERMISSION_DENIED))
        assertEquals("NotAllowedError", answers.single().getString("error"))
        assertNull(shadowOf(activity).peekNextStartedActivityForResult())
    }

    /** Under the agent the runner held the person's sheet: `take` writes the agent's picture and `cancel` is
     * `AbortError`; no app starts. */
    @Test fun underTheAgentTheSheetsTapAnswers() {
        Host.agent = true
        take(tap = "take")
        val answer = answers.single()
        assertEquals("app:/tmp/camera/1.jpg", answer.getString("path"))
        assertEquals(1280, answer.getInt("width"))
        assertTrue(File(Host.root, "tmp/camera/1.jpg").length() > 0)
        take(tap = "cancel")
        assertEquals("AbortError", answers.last().getString("error"))
        assertNull(shadowOf(activity).peekNextStartedActivityForResult())
    }
}
