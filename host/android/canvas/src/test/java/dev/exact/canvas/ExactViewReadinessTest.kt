package dev.exact.canvas

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.os.Looper
import android.view.View
import android.widget.FrameLayout
import java.nio.ByteBuffer
import java.nio.ByteOrder
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
import org.robolectric.annotation.LooperMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ExactViewReadinessTest.Host::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@LooperMode(LooperMode.Mode.PAUSED)
class ExactViewReadinessTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            var page: ByteBuffer? = null
            var barStatement: IntArray? = null
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun frame(): ByteBuffer? = page.also { page = null }
            @JvmStatic @Implementation fun bars(): IntArray? = barStatement.also { barStatement = null }
            @JvmStatic @Implementation fun fields(): IntArray? = null
            @JvmStatic @Implementation fun chrome(): IntArray? = null
            @JvmStatic @Implementation fun scrollers(): IntArray? = null
            @JvmStatic @Implementation fun requests(): IntArray? = null
            @JvmStatic @Implementation fun timerHeld() = 0
            @JvmStatic @Implementation fun animating() = false
            @JvmStatic @Implementation fun owed() = false
            @JvmStatic @Implementation fun nextDue() = -1.0
        }
    }

    private fun rect(color: Int) = intArrayOf(2, color, 0, 0, 64f.toBits(), 64f.toBits()) + IntArray(8)

    @Test fun anEmptyPageReportsReadyOnce() = presented(null, null)

    @Test fun barsWithoutPagePaintReportReadyOnce() {
        val top = rect(Color.RED)
        val bottom = rect(Color.BLUE)
        presented(null, intArrayOf(2, Bars.TOP, 0, 0, 64, 16, top.size) + top +
            intArrayOf(Bars.BOTTOM, 0, 48, 64, 16, bottom.size) + bottom)
    }

    @Test fun ordinaryPagePaintReportsReadyOnce() = presented(rect(Color.GREEN), null)

    private fun presented(page: IntArray?, bars: IntArray?) {
        Host.page = page?.let { words ->
            ByteBuffer.allocate(words.size * 4).order(ByteOrder.LITTLE_ENDIAN).also { it.asIntBuffer().put(words) }
        }
        Host.barStatement = bars
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        activity.intent.putExtra("warm", "0")
        var notifications = 0
        val view = ExactView(activity, false) { notifications++ }
        val root = FrameLayout(activity)
        val hostedBars = Bars(activity, root)
        view.bars = hostedBars
        val bitmap = Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val main = shadowOf(Looper.getMainLooper())

        view.draw(canvas)
        main.idle()
        assertEquals("a draw before startup reports nothing", 0, notifications)
        view.ownerStarted(false)
        view.draw(canvas)
        main.idle()
        assertEquals("failed startup reports nothing", 0, notifications)
        view.ownerStarted(true)
        main.idle()
        assertEquals("startup alone is not a presentation", 0, notifications)

        view.layout(0, 0, 64, 64)
        view.draw(canvas)
        view.draw(canvas)
        assertEquals("readiness is posted after drawing", 0, notifications)
        main.idle()
        assertEquals("the first presentation reports ready once", 1, notifications)
        view.draw(canvas)
        main.idle()
        assertEquals("later presentations do not repeat readiness", 1, notifications)
        if (page != null) assertEquals(Color.GREEN, bitmap.getPixel(32, 32))
        if (bars != null) {
            val layer = root.getChildAt(0) as FrameLayout
            assertEquals(View.VISIBLE, layer.getChildAt(0).visibility)
            assertEquals(View.VISIBLE, layer.getChildAt(1).visibility)
        }
        bitmap.recycle()
    }
}
