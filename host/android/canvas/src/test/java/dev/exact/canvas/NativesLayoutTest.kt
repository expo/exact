package dev.exact.canvas

import android.content.Context
import android.graphics.Bitmap
import android.view.View
import android.widget.ImageView
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * A module view whose update asks for a layout is laid out before it is recorded, at an unchanged size. The views
 * here are attached to no window, which does not matter: `View.requestLayout` sets the view's own force-layout flag,
 * which `isLayoutRequested` reads, before it asks its parent, in the framework Robolectric runs as on a device.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativesLayoutTest {
    /** An `ImageView` with a wide picture, its scale type the props' `object-fit`, as the camera's agent picture. */
    private object Picture : NativeModule {
        override fun create(context: Context, props: JSONObject): View = ImageView(context).also {
            it.setImageBitmap(Bitmap.createBitmap(100, 50, Bitmap.Config.ARGB_8888))
            update(it, props)
        }
        override fun update(view: View, props: JSONObject) {
            (view as ImageView).scaleType = when (props.optString("object-fit")) {
                "cover" -> ImageView.ScaleType.CENTER_CROP
                else -> ImageView.ScaleType.FIT_CENTER
            }
        }
    }

    private fun element(fit: String) =
        JSONObject().put("module", "test-picture").put("props", JSONObject().put("object-fit", fit).toString()).toString()

    @Test fun aScaleTypeChangedAtTheSameSizeIsDrawnWithItsOwnMatrix() {
        val natives = Natives(RuntimeEnvironment.getApplication(), { File(".") }, mapOf("test-picture" to Picture))
        // A 100×200 box: `cover` scales the 100×50 picture by 4, `contain` by 1.
        natives.ensure(1, 2, 100, 200, element("cover"), 0)
        val view = natives.getChildAt(0) as ImageView
        val scale = { FloatArray(9).also { view.imageMatrix.getValues(it) }[0] }
        assertEquals(4f, scale(), 0.01f)
        natives.ensure(1, 2, 100, 200, element("contain"), 0)
        assertEquals(1f, scale(), 0.01f)
        natives.ensure(1, 2, 100, 200, element("cover"), 0)
        assertEquals(4f, scale(), 0.01f)
    }
}
