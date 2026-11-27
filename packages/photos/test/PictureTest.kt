package exact.photos

import android.content.ContentProvider
import android.content.ContentResolver
import android.content.ContentValues
import android.content.Intent
import android.database.MatrixCursor
import android.graphics.Point
import android.net.Uri
import android.os.Bundle
import android.os.CancellationSignal
import android.provider.MediaStore
import android.util.Size
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import java.io.FileNotFoundException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

/** The size a photo's picture is fetched at follows the photo as it is shown, turned by its orientation. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE, shadows = [PictureTest.Host::class])
class PictureTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun changed(topic: String) {}
            @JvmStatic @Implementation fun pictureChanged(source: String, revoked: Boolean) {}
        }
    }

    /** A media provider with one photo, its [row] the stored width, height and orientation, which notes the thumbnail size asked
     * of it and has no file to give. */
    class Media : ContentProvider() {
        companion object {
            var row = arrayOf<Any>(4000, 3000, 0)
            var asked: Size? = null
        }
        override fun onCreate() = true
        override fun call(method: String, arg: String?, extras: Bundle?) = Bundle().apply { putString(Intent.EXTRA_TEXT, "1") }
        override fun query(uri: Uri, projection: Array<String>?, selection: String?, args: Array<String>?, order: String?) =
            MatrixCursor(projection).apply {
                val columns = listOf(MediaStore.Images.Media.WIDTH, MediaStore.Images.Media.HEIGHT, MediaStore.Images.Media.ORIENTATION)
                addRow(projection!!.map { row[columns.indexOf(it)] })
            }
        override fun openTypedAssetFile(uri: Uri, mimeTypeFilter: String, opts: Bundle?, signal: CancellationSignal?) =
            throw FileNotFoundException().also { asked = opts?.getParcelable(ContentResolver.EXTRA_SIZE, Point::class.java)?.let { Size(it.x, it.y) } }
        override fun getType(uri: Uri): String? = null
        override fun insert(uri: Uri, values: ContentValues?): Uri? = null
        override fun delete(uri: Uri, selection: String?, args: Array<String>?) = 0
        override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<String>?) = 0
    }

    private fun fetched(width: Int, height: Int, orientation: Int, box: Size, fit: String): Size? {
        Media.row = arrayOf(width, height, orientation)
        Media.asked = null
        Robolectric.setupContentProvider(Media::class.java, MediaStore.AUTHORITY)
        val module = PhotosModule(ExtensionContext(RuntimeEnvironment.getApplication(), "photos"))
        val done = CountDownLatch(1)
        module.picture("photos:content://media/external/images/media/5", box.width, box.height, fit, false) { _, _ -> done.countDown() }
        done.await(5, TimeUnit.SECONDS)
        return Media.asked
    }

    /** A portrait stored on its side (4000 by 3000, turned 90) shows 3000 by 4000: a 1080 by 2400 box holds it at 0.36. */
    @Test fun aTurnedPhotoIsFetchedForItsUprightSides() {
        assertEquals(Size(1440, 1440), fetched(4000, 3000, 90, Size(1080, 2400), "contain"))
        assertEquals(Size(1440, 1440), fetched(4000, 3000, 270, Size(1080, 2400), "contain"))
        assertEquals(Size(2400, 2400), fetched(4000, 3000, 90, Size(1080, 2400), "cover"))
    }

    @Test fun anUnturnedPhotoIsFetchedForItsStoredSides() {
        assertEquals(Size(1080, 1080), fetched(4000, 3000, 0, Size(1080, 2400), "contain"))
        assertEquals(Size(3200, 3200), fetched(4000, 3000, 180, Size(1080, 2400), "cover"))
    }
}
