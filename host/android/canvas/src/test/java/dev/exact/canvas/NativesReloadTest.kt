package dev.exact.canvas

import android.app.Activity
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.HardwareRenderer
import android.graphics.RenderNode
import android.graphics.SurfaceTexture
import android.os.Looper
import android.view.Choreographer
import android.view.TextureView
import java.io.File
import org.junit.After
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.RealObject
import org.robolectric.shadow.api.Shadow
import org.robolectric.shadows.ShadowTextureView
import org.robolectric.util.ReflectionHelpers

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], manifest = Config.NONE, shadows = [
    NativesWindowFrameRateTest.Host::class, NativesWindowFrameRateTest.Wrapped::class,
    NativesReloadTest.Texture::class,
])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativesReloadTest {
    /** TextureView latches a queued frame during draw, then notifies its listener. */
    @Implements(TextureView::class)
    class Texture : ShadowTextureView() {
        @RealObject private lateinit var view: TextureView
        var queued = false
        var latched = false
        @Implementation override fun draw(canvas: Canvas) {
            if (queued) {
                queued = false
                latched = true
                view.surfaceTextureListener!!.onSurfaceTextureUpdated(SurfaceTexture(0))
            }
            if (latched) canvas.drawColor(Color.RED)
        }
    }

    private val frames get() = NativesWindowFrameRateTest.Host.frames
    private val readiness get() = NativesWindowFrameRateTest.Host.readiness

    @After fun finish() { frames.clear(); readiness.clear() }

    private fun reader(buffered: Boolean): Natives {
        val intent = Intent().putExtra("gpubuf", if (buffered) "1" else "0")
        val activity = Robolectric.buildActivity(Activity::class.java, intent).setup().get()
        return Natives(activity, { File(".") }, emptyMap()).apply {
            ensure(7, 3, 8, 8, "{}", 0)
            assertEquals("one canvas must sustain its own observation", 1, childCount)
        }
    }

    private fun frame() {
        var ran = false
        Choreographer.getInstance().postFrameCallback { ran = true }
        while (!ran) shadowOf(Looper.getMainLooper()).runOneTask()
    }

    private fun element(natives: Natives): Any =
        ReflectionHelpers.getField<Map<Int, Any>>(natives, "elements")[7]!!

    private fun pixel(natives: Natives): Int {
        val hardware = HardwareRenderer::class.java.getDeclaredMethod("createHardwareBitmap",
            RenderNode::class.java, Int::class.javaPrimitiveType, Int::class.javaPrimitiveType)
            .invoke(null, natives.node(7)!!, 8, 8) as Bitmap
        val bitmap = hardware.copy(Bitmap.Config.ARGB_8888, false)
        return bitmap.getPixel(4, 4).also { bitmap.recycle(); hardware.recycle() }
    }

    @Test fun aSingleBufferedCanvasObservesALateFrameAfterClearAndReload() {
        val natives = reader(true)
        frames[7] = 1L
        frame()
        assertNotNull(ReflectionHelpers.getField<Bitmap?>(element(natives), "last"))
        frames.remove(7)
        readiness[7] = 3
        frame()
        assertNull(ReflectionHelpers.getField<Bitmap?>(element(natives), "last"))
        assertTrue(ReflectionHelpers.getField<Map<Long, Bitmap>>(element(natives), "wrapped").isEmpty())
        val presentation = ReflectionHelpers.getField<CanvasPresentation>(element(natives), "presentation")
        assertTrue(presentation.awaitingFrame)
        natives.ensure(7, 3, 8, 8, "{}", 0)
        frame()
        frame()
        assertNull(ReflectionHelpers.getField<Bitmap?>(element(natives), "last"))
        readiness[7] = 2
        frames[7] = 2L
        frame()
        assertNotNull("no other canvas wakes the reader", ReflectionHelpers.getField<Bitmap?>(element(natives), "last"))
        assertFalse(presentation.awaitingFrame)
    }

    @Test fun aSingleTextureCanvasLatchesALateFrameAfterClearAndReload() {
        val natives = reader(false)
        val original = natives.getChildAt(0) as TextureView
        val first = Shadow.extract<Texture>(original)
        first.queued = true
        natives.onDescendantInvalidated(original, original)
        frame()
        assertTrue(first.latched)
        assertEquals(Color.RED, pixel(natives))
        readiness[7] = 3
        frame()
        val fresh = natives.getChildAt(0) as TextureView
        assertNotSame(original, fresh)
        val texture = Shadow.extract<Texture>(fresh)
        assertFalse(texture.latched)
        assertEquals(Color.TRANSPARENT, pixel(natives))
        natives.ensure(7, 3, 8, 8, "{}", 0)
        frame()
        frame()
        assertEquals(Color.TRANSPARENT, pixel(natives))
        readiness[7] = 2
        texture.queued = true
        frame()
        assertTrue("the fresh TextureView must draw to latch its queued frame", texture.latched)
        val presentation = ReflectionHelpers.getField<CanvasPresentation>(element(natives), "presentation")
        assertTrue(presentation.ready)
        assertFalse(presentation.awaitingFrame)
        assertEquals("the latch draw is invisible", Color.TRANSPARENT, pixel(natives))
        frame()
        assertEquals(Color.RED, pixel(natives))
    }
}
