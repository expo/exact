package exact.photos

import android.Manifest
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Looper
import android.provider.MediaStore
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import java.util.concurrent.ExecutorService
import org.json.JSONObject
import org.junit.Assert.assertEquals
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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE, shadows = [SelectionTest.Host::class])
class SelectionTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun changed(topic: String) {}
            @JvmStatic @Implementation fun pictureChanged(source: String, revoked: Boolean) { if (revoked) withdrawn += source }
            val withdrawn = ArrayList<String>()
        }
    }

    /** A media provider that says its version and opens no photo: the person's selection no longer includes it. */
    class Media : ContentProvider() {
        override fun onCreate() = true
        override fun call(method: String, arg: String?, extras: Bundle?) = Bundle().apply { putString(Intent.EXTRA_TEXT, "1") }
        override fun query(uri: Uri, projection: Array<String>?, selection: String?, args: Array<String>?, order: String?) = null
        override fun getType(uri: Uri): String? = null
        override fun insert(uri: Uri, values: ContentValues?): Uri? = null
        override fun delete(uri: Uri, selection: String?, args: Array<String>?) = 0
        override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<String>?) = 0
    }

    /** A photo the selection dialog took out of the limited library has its picture withdrawn when the dialog answers,
     * with no resume. */
    @Test fun aNarrowedSelectionWithdrawsItsPictures() {
        val app = RuntimeEnvironment.getApplication()
        Robolectric.setupContentProvider(Media::class.java, MediaStore.AUTHORITY)
        val selected = "android.permission.READ_MEDIA_VISUAL_USER_SELECTED"
        shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).requestedPermissions =
            arrayOf(Manifest.permission.READ_MEDIA_IMAGES, selected)
        shadowOf(app).grantPermissions(selected)
        val module = PhotosModule(ExtensionContext(app, "photos"))
        val id = "content://media/external/images/media/5"
        ReflectionHelpers.getField<LinkedHashSet<String>>(module, "drawn").add(id)

        var answer: JSONObject? = null
        module.later("managePhotoSelection", JSONObject()) { answer = it }
        ReflectionHelpers.getField<ExecutorService>(module, "work").submit {}.get()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals("limited", answer!!.getString("access"))
        assertEquals(listOf("photos:$id"), Host.withdrawn)
    }
}
