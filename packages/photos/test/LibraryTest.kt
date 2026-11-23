package exact.photos

import android.Manifest
import android.content.ContentProvider
import android.content.ContentResolver
import android.content.ContentValues
import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.os.Bundle
import android.os.CancellationSignal
import android.os.Looper
import android.provider.MediaStore
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import java.util.concurrent.ExecutorService
import org.json.JSONObject
import org.junit.Assert.assertEquals
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
import org.robolectric.util.ReflectionHelpers

/** MediaStore's list as a replacement reads it (LLP 1133.019.000 D1, D3): a failed query is a failure, never an empty
 * last page, and every resume under a grant announces the library's readers. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE, shadows = [LibraryTest.Host::class])
class LibraryTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun changed(topic: String) { announced += topic }
            @JvmStatic @Implementation fun pictureChanged(source: String, revoked: Boolean) { if (revoked) withdrawn += source }
            val announced = ArrayList<String>()
            val withdrawn = ArrayList<String>()
        }
    }

    /** A media provider that says its version and answers its list query with [answer]. */
    class Media : ContentProvider() {
        companion object {
            var answer: () -> Cursor? = { null }
            var asked: Bundle? = null
            var version = "1"
        }
        override fun onCreate() = true
        override fun call(method: String, arg: String?, extras: Bundle?) = Bundle().apply { putString(Intent.EXTRA_TEXT, version) }
        override fun query(uri: Uri, projection: Array<String>?, args: Bundle?, signal: CancellationSignal?): Cursor? {
            asked = args
            return answer()
        }
        override fun query(uri: Uri, projection: Array<String>?, selection: String?, args: Array<String>?, order: String?) = answer()
        override fun getType(uri: Uri): String? = null
        override fun insert(uri: Uri, values: ContentValues?): Uri? = null
        override fun delete(uri: Uri, selection: String?, args: Array<String>?) = 0
        override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<String>?) = 0
    }

    private lateinit var module: PhotosModule

    @Before fun granted() {
        val app = RuntimeEnvironment.getApplication()
        Robolectric.setupContentProvider(Media::class.java, MediaStore.AUTHORITY)
        shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).requestedPermissions =
            arrayOf(Manifest.permission.READ_MEDIA_IMAGES)
        shadowOf(app).grantPermissions(Manifest.permission.READ_MEDIA_IMAGES)
        module = PhotosModule(ExtensionContext(app, "photos"))
        Host.announced.clear()
        Host.withdrawn.clear()
        Media.version = "1"
    }

    private fun assets(args: JSONObject = JSONObject()): JSONObject {
        var answer: JSONObject? = null
        module.later("photoAssets", args.put("limit", 10)) { answer = it }
        ReflectionHelpers.getField<ExecutorService>(module, "work").submit {}.get()
        shadowOf(Looper.getMainLooper()).idle()
        return answer!!
    }

    @Test fun noCursorIsAnUnknownError() {
        Media.answer = { null }
        assertEquals("UnknownError", assets().optString("error"))
    }

    @Test fun aProvidersFailureIsAnUnknownError() {
        Media.answer = { throw IllegalStateException("the volume went away") }
        assertEquals("UnknownError", assets().optString("error"))
    }

    @Test fun aHatchsFailureIsAnUnknownError() {
        Media.answer = { android.database.MatrixCursor(arrayOf(MediaStore.Images.Media._ID)) }
        PhotosModule.queryArgsHatch = { throw IllegalStateException("the app's own lookup failed") }
        try { assertEquals("UnknownError", assets().optString("error")) } finally { PhotosModule.queryArgsHatch = null }
    }

    /** Under a selection, a resume withdraws a picture whose photo has no row, though nothing read the status first,
     * and keeps one whose check answered no cursor. */
    @Test fun aLimitedResumeWithdrawsOnlyWhatHasNoRow() {
        val app = RuntimeEnvironment.getApplication()
        val selected = "android.permission.READ_MEDIA_VISUAL_USER_SELECTED"
        shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).requestedPermissions =
            arrayOf(Manifest.permission.READ_MEDIA_IMAGES, selected)
        shadowOf(app).denyPermissions(Manifest.permission.READ_MEDIA_IMAGES)
        shadowOf(app).grantPermissions(selected)
        val drawn = ReflectionHelpers.getField<LinkedHashSet<String>>(module, "drawn")
        drawn.add("content://media/external/images/media/5")
        Media.answer = { android.database.MatrixCursor(arrayOf(MediaStore.Images.Media._ID)) }
        module.resumed()
        ReflectionHelpers.getField<ExecutorService>(module, "work").submit {}.get()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf("photos:content://media/external/images/media/5"), Host.withdrawn)

        drawn.add("content://media/external/images/media/6")
        Media.answer = { null }
        module.resumed()
        ReflectionHelpers.getField<ExecutorService>(module, "work").submit {}.get()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, Host.withdrawn.size)
    }

    @Test fun aRefusedQueryIsANotAllowedErrorWithNoProviderMessage() {
        Media.answer = { throw SecurityException("content://media/external/images/media/5 is not yours") }
        val answer = assets()
        assertEquals("NotAllowedError", answer.optString("error"))
        assertEquals("", answer.optString("message"))
    }

    @Test fun aCancelledQueryIsAnAbortError() {
        Media.answer = { throw android.os.OperationCanceledException() }
        assertEquals("AbortError", assets().optString("error"))
    }

    @Test fun aResumeWithTheSameStatusAnnouncesTheLibrary() {
        Media.answer = { android.database.MatrixCursor(arrayOf(MediaStore.Images.Media._ID)) }
        assertEquals(0, assets().getJSONArray("assets").length())
        Host.announced.clear()
        module.resumed()
        assertEquals(listOf("photos.photoAssets", "photos.photoChanges"), Host.announced)
    }

    /** The screenshot filter and a page's key join the `queryArgs` hatch's selection, whose arguments keep their
     * places (LLP 1133.019 D9, 1133.019.000 D4). */
    @Test fun theFilterJoinsTheHatchsSelection() {
        Media.answer = { android.database.MatrixCursor(arrayOf(MediaStore.Images.Media._ID)) }
        PhotosModule.queryArgsHatch = {
            it.putString(ContentResolver.QUERY_ARG_SQL_SELECTION, "${MediaStore.Images.Media.MIME_TYPE} = ?")
            it.putStringArray(ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS, arrayOf("image/png"))
        }
        try {
            val cursor = android.util.Base64.encodeToString("mediastore\n100\n\n7".toByteArray(), android.util.Base64.NO_WRAP)
            assets(JSONObject().put("subtypes", org.json.JSONArray().put("screenshot")).put("cursor", cursor))
        } finally { PhotosModule.queryArgsHatch = null }
        val asked = Media.asked!!
        val path = "rtrim(relative_path, '/')"
        assertEquals("(mime_type = ?) AND ($path LIKE 'Screenshots' OR $path LIKE '%/Screenshots') AND " +
            "((datetaken < 100 OR datetaken IS NULL OR (datetaken = 100 AND (date_added IS NULL AND _id < 7))))",
            asked.getString(ContentResolver.QUERY_ARG_SQL_SELECTION))
        assertEquals(listOf("image/png"), asked.getStringArray(ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS)!!.toList())
    }

    /** While the read is `prompt`, a resume with the same status announces nothing, so nothing asks again. */
    @Test fun aResumeWithoutAGrantAnnouncesNothing() {
        shadowOf(RuntimeEnvironment.getApplication()).denyPermissions(Manifest.permission.READ_MEDIA_IMAGES)
        module.later("photoLibraryStatus", JSONObject()) {}
        module.resumed()
        assertEquals(emptyList<String>(), Host.announced)
    }

    /** A status that changed but not in its read (here MediaStore's version) announces the status alone. */
    @Test fun aResumeWithAnotherVersionButNoGrantAnnouncesTheStatusAlone() {
        shadowOf(RuntimeEnvironment.getApplication()).denyPermissions(Manifest.permission.READ_MEDIA_IMAGES)
        module.later("photoLibraryStatus", JSONObject()) {}
        Media.version = "2"
        module.resumed()
        assertEquals(listOf("photos.photoLibraryStatus"), Host.announced)
    }
}
