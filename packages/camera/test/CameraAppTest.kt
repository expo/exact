package exact.camera

import android.Manifest
import android.app.Activity
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Intent
import android.content.pm.PackageManager
import android.database.Cursor
import android.graphics.Bitmap
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.os.Looper
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
import org.robolectric.shadows.ShadowMediaMetadataRetriever
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream

/** `takePhotoWithCameraApp` (expo/exact#381) and `recordVideoWithCameraApp` (expo/exact#633, LLP 1133.008.000) on
 * Android: the system's camera app over a Robolectric activity standing for the reader's, the camera app played by the
 * test through the reader's `Files` provider. */
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

    /** A camera app's own media store, which answers a video it recorded there instead of at `EXTRA_OUTPUT`. */
    class Gallery : ContentProvider() {
        companion object {
            var type: String? = "video/mp4"
            lateinit var file: File
        }
        override fun onCreate() = true
        override fun getType(uri: Uri) = type
        override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
        override fun query(uri: Uri, p: Array<out String>?, s: String?, a: Array<out String>?, o: String?): Cursor? = null
        override fun insert(uri: Uri, values: ContentValues?): Uri? = null
        override fun delete(uri: Uri, s: String?, a: Array<out String>?) = 0
        override fun update(uri: Uri, values: ContentValues?, s: String?, a: Array<out String>?) = 0
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

    private fun take(args: JSONObject = JSONObject(), tap: String? = null, send: String = "takePhotoWithCameraApp", request: Int? = null) =
        CameraModule(ExtensionContext(application, "camera").also { it.request = request }).let { module ->
            module.later(send, args, tap?.let { JSONObject().put("tap", it) }) { answers += it }
            module
        }

    private fun record(args: JSONObject = JSONObject(), tap: String? = null, request: Int? = null) =
        take(args, tap, "recordVideoWithCameraApp", request)

    /** The kept file is read and published off the main thread, then answered on it: wait for the answer. */
    private fun settled(): JSONObject {
        val end = System.currentTimeMillis() + 10_000
        while (answers.isEmpty() && System.currentTimeMillis() < end) {
            Thread.sleep(5)
            shadowOf(Looper.getMainLooper()).idle()
        }
        return answers.single()
    }

    /** The staged video's facts as the platform's retriever would read them. */
    private fun facts(width: Int, height: Int, rotation: Int, ms: Long) {
        val staged = File(application.cacheDir, "exact.camera-app/video.mp4").path
        ShadowMediaMetadataRetriever.addMetadata(staged, MediaMetadataRetriever.METADATA_KEY_MIMETYPE, "video/mp4")
        ShadowMediaMetadataRetriever.addMetadata(staged, MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH, "$width")
        ShadowMediaMetadataRetriever.addMetadata(staged, MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT, "$height")
        ShadowMediaMetadataRetriever.addMetadata(staged, MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION, "$rotation")
        ShadowMediaMetadataRetriever.addMetadata(staged, MediaMetadataRetriever.METADATA_KEY_DURATION, "$ms")
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
        val answer = settled()
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

    /** A video: the camera app's video intent with the call's limits as its extras, and no permission asked, the
     * microphone's included, though the app declares it; the file it writes through the URI, opened `rw` as a muxer
     * opens it, replaces the path once kept and is answered with its size as it plays and its length. */
    @Test fun aKeptVideoIsAnsweredWithItsRotatedSizeAndLength() {
        shadowOf(application.packageManager).getInternalMutablePackageInfo(application.packageName).requestedPermissions = arrayOf(Manifest.permission.RECORD_AUDIO)
        shadowOf(application).denyPermissions(Manifest.permission.RECORD_AUDIO)
        val clip = File(Host.root, "data/clip.mp4").apply { parentFile!!.mkdirs(); writeText("the last clip") }
        val android = JSONObject().put("videoQuality", "low").put("sizeLimit", 50_000_000)
        record(JSONObject().put("path", "app:/data/clip").put("maxDuration", 30.2).put("facingMode", "environment").put("android", android))
        assertNull("no permission is asked", shadowOf(activity).lastRequestedPermission)
        val (intent, code) = started()
        assertEquals(MediaStore.ACTION_VIDEO_CAPTURE, intent.action)
        assertEquals(31, intent.getIntExtra(MediaStore.EXTRA_DURATION_LIMIT, -1))
        assertEquals(0, intent.getIntExtra(MediaStore.EXTRA_VIDEO_QUALITY, -1))
        assertEquals(50_000_000L, intent.getLongExtra(MediaStore.EXTRA_SIZE_LIMIT, -1))
        assertEquals(0, intent.getIntExtra("android.intent.extras.CAMERA_FACING", -1))
        val bytes = ByteArray(4096) { it.toByte() }
        provider.openFile(output(intent), "rw").use { pfd -> FileOutputStream(pfd.fileDescriptor).use { it.write(bytes) } }
        facts(1920, 1080, 90, 31_500)
        assertEquals("the last clip", clip.readText())

        Extensions.activityResult(code, Activity.RESULT_OK, null)
        val answer = settled()
        assertEquals("app:/data/clip.mp4", answer.getString("path"))
        assertEquals("video/mp4", answer.getString("type"))
        assertEquals(4096L, answer.getLong("size"))
        assertEquals(31.5, answer.getDouble("duration"), 0.0)
        assertEquals(1080, answer.getInt("width"))
        assertEquals(1920, answer.getInt("height"))
        assertArrayEquals(bytes, clip.readBytes())
    }

    /** A camera app that records into its own store and answers that URI has the video copied in, when its type is a
     * video's; any other type, or a URI naming one of the app's own files, is read never and answered `UnknownError`,
     * the path keeping what it had. */
    @Test fun aCameraAppsOwnVideoIsCopiedInOnlyWhenItIsAVideoOfAnotherApp() {
        val authority = "test.camera.media"
        Robolectric.setupContentProvider(Gallery::class.java, authority)
        Gallery.file = File(application.filesDir, "gallery.mp4").apply { writeBytes(ByteArray(200_000) { (it % 251).toByte() }) }
        val theirs = Intent().setData(Uri.parse("content://$authority/video/7"))

        Gallery.type = "video/mp4"
        record()
        var (_, code) = started()
        Extensions.activityResult(code, Activity.RESULT_OK, theirs)
        val copied = settled()
        assertEquals("app:/tmp/camera/1.mp4", copied.getString("path"))
        assertArrayEquals(Gallery.file.readBytes(), File(Host.root, "tmp/camera/1.mp4").readBytes())

        answers.clear()
        Gallery.type = "image/jpeg"
        val kept = File(Host.root, "tmp/camera/1.mp4").apply { writeText("kept") }
        record()
        code = started().second
        Extensions.activityResult(code, Activity.RESULT_OK, theirs)
        assertEquals("UnknownError", settled().getString("error"))
        assertEquals("kept", kept.readText())

        answers.clear()
        val secret = File(Host.root, "data/secret.mp4").apply { parentFile!!.mkdirs(); writeText("the app's own") }
        record()
        code = started().second
        Extensions.activityResult(code, Activity.RESULT_OK, Intent().setData(Files.uri(application, secret)))
        assertEquals("UnknownError", settled().getString("error"))
        assertEquals("kept", kept.readText())
        assertFalse(File(application.cacheDir, "exact.camera-app/video.mp4").exists())
    }

    /** A call let go while the camera app is up (LLP 1133.008.000 D7): what the person keeps there is not written, the
     * path keeps what it had, nothing stays staged, and the next call opens the camera app again. */
    @Test fun aCallLetGoWritesNothingOfWhatThePersonKeeps() {
        val clip = File(Host.root, "tmp/camera/1.mp4").apply { parentFile!!.mkdirs(); writeText("kept") }
        val module = record(request = 7)
        val (intent, code) = started()
        provider.openFile(output(intent), "rw").use { pfd -> FileOutputStream(pfd.fileDescriptor).use { it.write(ByteArray(10)) } }
        module.cancel(8)
        module.cancel(7)
        Extensions.activityResult(code, Activity.RESULT_OK, null)
        assertEquals("AbortError", settled().getString("error"))
        assertEquals("kept", clip.readText())
        assertFalse(File(application.cacheDir, "exact.camera-app/video.mp4").exists())
        answers.clear()
        module.later("takePhotoWithCameraApp", JSONObject(), null) { answers += it }
        assertEquals(MediaStore.ACTION_IMAGE_CAPTURE, started().first.action)
    }

    /** One camera app at a time, a photo's or a video's. */
    @Test fun aSecondCallWhileTheCameraAppIsUpIsInvalidStateError() {
        val module = record()
        started()
        module.later("takePhotoWithCameraApp", JSONObject(), null) { answers += it }
        assertEquals("InvalidStateError", answers.single().getString("error"))
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

    /** … and a video's `take` writes the package's clip, the same bytes as `fixture/clip.mp4`. */
    @Test fun underTheAgentTakeWritesTheClip() {
        Host.agent = true
        record(tap = "take")
        val answer = answers.single()
        assertEquals("app:/tmp/camera/1.mp4", answer.getString("path"))
        assertEquals("video/mp4", answer.getString("type"))
        val fixture = File("../../../packages/camera/fixture/clip.mp4")
        assertArrayEquals(fixture.readBytes(), File(Host.root, "tmp/camera/1.mp4").readBytes())
        assertEquals(fixture.length(), answer.getLong("size"))
        assertNull(shadowOf(activity).peekNextStartedActivityForResult())
    }
}
