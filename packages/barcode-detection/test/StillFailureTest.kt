package exact.barcodedetection

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Looper
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [StillFailureTest.Host::class, StillFailureTest.Decoder::class])
class StillFailureTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun appFile(path: String) = path
        }
    }
    @Implements(BitmapFactory::class)
    class Decoder {
        companion object {
            var allocated: Bitmap? = null
            @JvmStatic @Implementation fun decodeFile(path: String): Bitmap? = allocated ?: throw OutOfMemoryError("large photo")
        }
    }
    @Implements(android.media.ExifInterface::class)
    class Exif {
        @Implementation fun getAttributeInt(tag: String, defaultValue: Int) = 6
    }
    @Implements(android.graphics.Matrix::class)
    class Rotation {
        @Implementation fun setRotate(angle: Float) { throw OutOfMemoryError("rotate photo") }
    }
    @Test
    @Config(shadows = [Host::class, Decoder::class, Exif::class, Rotation::class])
    fun anErrorAfterDecodingRecyclesTheAllocatedBitmap() {
        val bitmap = Bitmap.createBitmap(80, 40, Bitmap.Config.ARGB_8888)
        Decoder.allocated = bitmap
        try {
            decodingAnErrorStillRepliesExactlyOnce()
            assertTrue(bitmap.isRecycled)
        } finally { Decoder.allocated = null; bitmap.recycle() }
    }
    @Test fun decodingAnErrorStillRepliesExactlyOnce() {
        val app = RuntimeEnvironment.getApplication()
        val file = java.io.File.createTempFile("barcode-oom", ".jpg", app.cacheDir)
        val module = BarcodeDetectionModule(ExtensionContext(app, "barcode-detection"))
        val answers = ArrayList<JSONObject>()
        try {
            module.later("detectBarcodes", JSONObject().put("image", file.path)) { answers += it }
            module.work.shutdown()
            assertTrue(module.work.awaitTermination(10, TimeUnit.SECONDS))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(1, answers.size)
            assertEquals("OperationError", answers.single().getString("error"))
        } finally { module.destroy {}; file.delete() }
    }
}
