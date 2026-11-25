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
import dev.exact.canvas.Cause
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
            @JvmStatic @Implementation fun openApp(path: String): ParcelFileDescriptor? =
                File(appFile(path)).takeIf(Files::regular)?.let { ParcelFileDescriptor.open(it, ParcelFileDescriptor.MODE_READ_ONLY) }
            /** What happens on the camera worker as its file is published. */
            var publishing: () -> Unit = {}
            @JvmStatic @Implementation fun publishApp(path: String, source: String): Boolean {
                publishing()
                val to = File(appFile(path)).apply { parentFile!!.mkdirs() }
                return File(source).renameTo(to)
            }
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
            /** What happens on the camera worker as it opens the video. */
            var opening: () -> Unit = {}
        }
        override fun onCreate() = true
        override fun getType(uri: Uri) = type
        override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).also { opening() }
        override fun query(uri: Uri, p: Array<out String>?, s: String?, a: Array<out String>?, o: String?): Cursor? = null
        override fun insert(uri: Uri, values: ContentValues?): Uri? = null
        override fun delete(uri: Uri, s: String?, a: Array<out String>?) = 0
        override fun update(uri: Uri, values: ContentValues?, s: String?, a: Array<out String>?) = 0
    }

    private val application = RuntimeEnvironment.getApplication()
    private lateinit var activity: Activity
    private lateinit var provider: Files
    private val answers = ArrayList<JSONObject>()
    /** The looper each answer came on: the main thread's, as the runner's replies must. */
    private val answeredOn = ArrayList<Looper?>()

    @Before fun show() {
        Host.agent = false
        Host.publishing = {}
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
        CameraModule(ExtensionContext(application, "camera")).let { module ->
            module.later(send, args, tap?.let { JSONObject().put("tap", it) }, Cause("app", request = request)) { answers += it; answeredOn += Looper.myLooper() }
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

    /** The video staged at [out], as kept in its private copy, with its facts as the platform's retriever would read them. */
    private fun facts(out: Uri, width: Int, height: Int, rotation: Int, ms: Long) {
        val staged = staged(out).let { File(it.parentFile, "kept-${it.name}") }.path
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

    /** Where the reader keeps the file the camera app writes through [out]. */
    private fun staged(out: Uri) = File(Files.captures(application), out.pathSegments.drop(1).joinToString("/"))

    /** Whether any capture's file is still staged. */
    private fun staging() = Files.captures(application).listFiles().orEmpty().isNotEmpty()

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
        assertFalse("staged outside the app's directories", staged(out).canonicalPath.startsWith(Host.root.canonicalPath + File.separator))
        assertTrue(staged(out).isFile)

        // Another of the app's files stays read-only, the destination among them.
        val other = Files.uri(application, "app:/data/visitor.jpg")!!
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
        assertFalse(staged(out).exists())
        assertFalse(staging())
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
        // The camera app keeps its descriptor past its answer: what it writes then reaches no kept file.
        val held = provider.openFile(output(intent), "rw")
        FileOutputStream(held.fileDescriptor).write(bytes)
        facts(output(intent), 1920, 1080, 90, 31_500)
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
        held.use { FileOutputStream(it.fileDescriptor).write(ByteArray(8) { 9 }) }
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
        File(Host.root, "data/secret.mp4").apply { parentFile!!.mkdirs(); writeText("the app's own") }
        record()
        code = started().second
        Extensions.activityResult(code, Activity.RESULT_OK, Intent().setData(Files.uri(application, "app:/data/secret.mp4")))
        assertEquals("UnknownError", settled().getString("error"))
        assertEquals("kept", kept.readText())

        // Nor under another user's form of the same authority.
        answers.clear()
        record()
        code = started().second
        val mine = Files.uri(application, "app:/data/secret.mp4")!!
        Extensions.activityResult(code, Activity.RESULT_OK, Intent().setData(mine.buildUpon().encodedAuthority("10@${mine.authority}").build()))
        assertEquals("UnknownError", settled().getString("error"))
        assertEquals("kept", kept.readText())
        assertFalse(staging())
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
        assertFalse(staging())
        answers.clear()
        module.later("takePhotoWithCameraApp", JSONObject(), null, Cause("app")) { answers += it }
        assertEquals(MediaStore.ACTION_IMAGE_CAPTURE, started().first.action)
    }

    /** Holds the camera worker at a checkpoint ([Host.publishing], [Gallery.opening]) while the test, on the main thread
     * as the app's `cancel` runs, lets the call go; then lets the worker on and waits for it to finish. */
    private class Checkpoint {
        val reached = java.util.concurrent.CountDownLatch(1)
        val resume = java.util.concurrent.CountDownLatch(1)
        val hold: () -> Unit = { reached.countDown(); resume.await(10, java.util.concurrent.TimeUnit.SECONDS) }
        fun letGo(module: CameraModule) {
            assertTrue("the worker reached its checkpoint", reached.await(10, java.util.concurrent.TimeUnit.SECONDS))
            assertSame(Looper.getMainLooper(), Looper.myLooper())
            module.cancel(7)
        }
        fun release() {
            resume.countDown()
            Thread.getAllStackTraces().keys.filter { it.name == "exact-camera-app" }.forEach { it.join(10_000) }
        }
    }

    /** A let-go is too late once the worker began publishing the file: the commit point, after which the call answers
     * with what it kept, as one let go after the answer does. */
    @Test fun aLetGoOnceThePublishBeganKeepsTheFile() {
        val clip = File(Host.root, "tmp/camera/1.mp4").apply { parentFile!!.mkdirs(); writeText("kept") }
        val module = record(request = 7)
        val (intent, code) = started()
        val bytes = ByteArray(10) { 7 }
        provider.openFile(output(intent), "rw").use { pfd -> FileOutputStream(pfd.fileDescriptor).use { it.write(bytes) } }
        val checkpoint = Checkpoint().also { Host.publishing = it.hold }
        Extensions.activityResult(code, Activity.RESULT_OK, null)
        checkpoint.letGo(module)
        assertTrue("no answer while publication runs", answers.isEmpty())
        checkpoint.release()
        assertEquals("app:/tmp/camera/1.mp4", settled().getString("path"))
        assertArrayEquals(bytes, clip.readBytes())
        assertEquals(listOf(Looper.getMainLooper()), answeredOn)
    }

    /** A let-go while the worker copies the camera app's own video, before publication, is answered at once on the main
     * thread, and keeps nothing: once the worker is done, no capture is left. */
    @Test fun aLetGoBeforeThePublishKeepsNothing() {
        Robolectric.setupContentProvider(Gallery::class.java, "test.camera.media")
        Gallery.type = "video/mp4"
        Gallery.file = File(application.filesDir, "gallery.mp4").apply { writeBytes(ByteArray(1000) { 3 }) }
        val clip = File(Host.root, "tmp/camera/1.mp4").apply { parentFile!!.mkdirs(); writeText("kept") }
        val module = record(request = 7)
        val (_, code) = started()
        val checkpoint = Checkpoint().also { Gallery.opening = it.hold }
        try {
            Extensions.activityResult(code, Activity.RESULT_OK, Intent().setData(Uri.parse("content://test.camera.media/video/7")))
            checkpoint.letGo(module)
            assertEquals("answered at once, while the worker waits", "AbortError", answers.single().getString("error"))
            checkpoint.release()
            shadowOf(Looper.getMainLooper()).idle()
        } finally { Gallery.opening = {} }
        assertEquals(1, answers.size)
        assertEquals(listOf(Looper.getMainLooper()), answeredOn)
        assertEquals("kept", clip.readText())
        assertFalse(staging())
    }

    /** One camera app at a time, a photo's or a video's. */
    @Test fun aSecondCallWhileTheCameraAppIsUpIsInvalidStateError() {
        val module = record()
        started()
        module.later("takePhotoWithCameraApp", JSONObject(), null, Cause("app")) { answers += it }
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
