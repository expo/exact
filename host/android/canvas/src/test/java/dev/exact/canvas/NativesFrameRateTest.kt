package dev.exact.canvas

import android.app.Activity
import android.graphics.Bitmap
import android.hardware.HardwareBuffer
import android.os.Looper
import android.os.SystemClock
import android.view.Choreographer
import android.view.Surface
import android.view.SurfaceView
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.widget.FrameLayout
import java.io.File
import java.time.Duration
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
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadow.api.Shadow
import org.robolectric.shadows.ShadowNativeBitmap
import org.robolectric.shadows.ShadowNativeSurface
import org.robolectric.shadows.ShadowSystemClock
import org.robolectric.shadows.ShadowViewRootImpl
import org.robolectric.util.ReflectionHelpers

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [NativesFrameRateTest.Votes::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativesFrameRateTest {
    @Implements(Surface::class)
    class Votes : ShadowNativeSurface() {
        val votes = ArrayList<Pair<Float, Int>>()
        var valid = true
        @Implementation fun isValid() = valid
        @Implementation fun setFrameRate(fps: Float, compatibility: Int) { votes.add(fps to compatibility) }
    }

    private fun natives() = Natives(RuntimeEnvironment.getApplication(), { File(".") }, emptyMap()).apply {
        ensure(7, 3, 0, 0, "{}", 0)
    }

    /** Supply a compositor surface without starting the Rust GPU host. */
    private fun surface(natives: Natives): Votes {
        val elements = Natives::class.java.getDeclaredField("elements").apply { isAccessible = true }
            .get(natives) as Map<*, *>
        val e = elements[7]!!
        val surface = Surface(android.graphics.SurfaceTexture(0))
        e.javaClass.getDeclaredField("view").apply { isAccessible = true }
            .set(e, SurfaceView(natives.context))
        e.javaClass.getDeclaredField("surface").apply { isAccessible = true }.set(e, surface)
        return Shadow.extract(surface)
    }

    @Test fun aCanvasCapVotesDefaultToSuppressGlobalTouchBoostAndZeroClearsIt() {
        val natives = natives()
        val surface = surface(natives)
        // AOSP's ExplicitDefault vote suppresses global touch boost; ExactOrMultiple does not (#574).
        for (fps in listOf(24, 30, 60, 90, 120, 0)) {
            natives.frameRates(intArrayOf(7, fps))
            assertEquals(fps.toFloat() to Surface.FRAME_RATE_COMPATIBILITY_DEFAULT, surface.votes.last())
        }
        assertEquals(6, surface.votes.size)
        surface.valid = false
        natives.frameRates(intArrayOf(7, 60))
        assertEquals("a destroyed surface gets no vote", 6, surface.votes.size)
    }
}

/**
 * A view's requested rate reaches its window when the view is recorded after it was invalidated
 * ([View.setRequestedFrameRate]): the view root sends the window's layer the highest rate and category its views
 * voted since its last frame, and keeps the rate until 100 ms pass in which no view voted one (`ViewRootImpl`'s
 * `FRAME_RATE_SETTING_REEVALUATE_TIME`, Android 15 and 16). A canvas the reader draws from its buffers has a view
 * that draws nothing, so the reader records it with each canvas frame (#755).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], manifest = Config.NONE,
    shadows = [NativesWindowFrameRateTest.Host::class, NativesWindowFrameRateTest.Wrapped::class, NativesWindowFrameRateTest.Root::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativesWindowFrameRateTest {
    /** The host's buffered canvases: [frames] numbers each one's newest frame ([Native.canvasFrame]). */
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            val frames = HashMap<Int, Long>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun attachBuffers(view: Int, width: Int, height: Int) {}
            @JvmStatic @Implementation fun canvasFrame(view: Int) = frames[view] ?: -1L
            @JvmStatic @Implementation fun canvasBuffer(view: Int): HardwareBuffer? =
                HardwareBuffer.create(8, 8, HardwareBuffer.RGBA_8888, 1, HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE)
        }
    }

    /** Robolectric wraps no hardware buffer: a bitmap of the buffer's size stands for it. */
    @Implements(Bitmap::class, isInAndroidSdk = false, callNativeMethodsByDefault = true)
    class Wrapped : ShadowNativeBitmap() {
        companion object {
            @JvmStatic @Implementation fun wrapHardwareBuffer(buffer: HardwareBuffer, space: android.graphics.ColorSpace?): Bitmap =
                Bitmap.createBitmap(buffer.width, buffer.height, Bitmap.Config.ARGB_8888)
        }
    }

    /** The votes the window's views cast, and whether the window was told its drawing changed. */
    @Implements(className = "android.view.ViewRootImpl", isInAndroidSdk = false)
    class Root : ShadowViewRootImpl() {
        companion object {
            val rates = ArrayList<Pair<Float, Int>>()
            val categories = ArrayList<Pair<View?, Int>>()
            var asked = false
        }
        @Implementation fun votePreferredFrameRate(rate: Float, compatibility: Int) { rates.add(rate to compatibility) }
        @Implementation fun votePreferredFrameRateCategory(category: Int, reason: Int, view: View?) { categories.add(view to category) }
        @Implementation fun onDescendantInvalidated(child: View, descendant: View) { asked = true }
    }

    // Robolectric's Android 15 has the views' votes switched off; a phone whose windows vote has them on.
    private val voting = "sToolkitFrameRateViewEnablingReadOnlyFlagValue"
    private val none = 1 // FRAME_RATE_CATEGORY_NO_PREFERENCE
    private val fixed = Surface.FRAME_RATE_COMPATIBILITY_FIXED_SOURCE
    private lateinit var natives: Natives
    private lateinit var ticker: View
    private var ticks = 0

    /** The reader's window as [ExactActivity] makes it: the hosted elements' layout and the 1×1 view its frames are
     * drawn through, with canvases 7 and 8 drawn from their buffers. */
    @Before fun reader() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        ReflectionHelpers.setStaticField(View::class.java, voting, true)
        natives = Natives(activity, { File(".") }, emptyMap())
        ticker = View(activity)
        natives.frame = { ticks++; ticker.invalidate() }
        activity.setContentView(FrameLayout(activity).apply {
            addView(natives, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
            addView(ticker, FrameLayout.LayoutParams(1, 1))
        })
        for (id in 7..8) natives.ensure(id, 3, 400, 300, "{}", 0)
        frame()
    }

    @After fun finish() {
        Host.frames.clear()
        ReflectionHelpers.setStaticField(View::class.java, voting, false)
    }

    /** One display frame, in which each of [canvases] has a new frame: the reader looks for them, then the window's
     * views are recorded as a traversal records them (Robolectric's window draws no frame itself). The votes are
     * those cast in it. */
    private fun frame(vararg canvases: Int) {
        Root.rates.clear(); Root.categories.clear(); Root.asked = false; ticks = 0
        for (id in canvases) Host.frames[id] = (Host.frames[id] ?: 0L) + 1
        var ran = false
        Choreographer.getInstance().postFrameCallback { ran = true }
        while (!ran) shadowOf(Looper.getMainLooper()).runOneTask()
        View::class.java.getMethod("updateDisplayListIfDirty").invoke(natives.rootView)
    }

    /** The categories the reader's own views voted in the last frame. */
    private fun own() = Root.categories.filter { it.first === natives || it.first === ticker || it.first?.parent === natives }

    @Test fun aCappedCanvasInTheReaderWindowVotesItsRateInEachFrameItDraws() {
        val canvas = natives.getChildAt(0)
        natives.frameRates(intArrayOf(7, 60))
        assertEquals(60f, canvas.requestedFrameRate, 0f)
        for (n in 1..3) {
            frame(7)
            assertEquals("frame $n", listOf(60f to fixed), Root.rates)
            // The window draws the frame with only the canvas's view recorded for it, which casts no category: beside
            // one, SurfaceFlinger drops the rate on a panel of fixed modes (`FrameRate::isVoteValidForMrr`). The
            // ticker would cast one, and the window-sized layout recorded again would damage the whole window.
            assertTrue("frame $n", Root.asked)
            assertEquals("frame $n", 0, ticks)
            assertEquals("frame $n", listOf(canvas to none), own())
            // A display frame the cap skips draws nothing, so the view root keeps the rate it has.
            frame()
            assertEquals("after frame $n", emptyList<Pair<Float, Int>>(), Root.rates)
            assertEquals("after frame $n", emptyList<Pair<View?, Int>>(), own())
            assertFalse("after frame $n", Root.asked)
            assertEquals("after frame $n", 0, ticks)
        }
        // An uncapped canvas asks for nothing: its view is not recorded, and its frame is drawn through the ticker.
        natives.frameRates(intArrayOf(7, 0))
        assertEquals(View.REQUESTED_FRAME_RATE_CATEGORY_DEFAULT, canvas.requestedFrameRate, 0f)
        frame(7)
        assertEquals(emptyList<Pair<Float, Int>>(), Root.rates)
        assertEquals(listOf(ticker), own().map { it.first })
        assertEquals(1, ticks)
    }

    @Test fun motionBesideACappedCanvasKeepsTheVoteItHasWithoutOne() {
        // What the ticker votes for a move in a window with no capped canvas: a category, by its size.
        natives.frame()
        frame()
        val usual = own()
        assertEquals(listOf(ticker), usual.map { it.first })
        assertTrue("$usual", usual[0].second > none)
        natives.frameRates(intArrayOf(7, 60))
        // A move (a scroll step, a playing layer) in a display frame the capped canvas draws, and in one it skips.
        natives.frame()
        frame(7)
        assertEquals(listOf(60f to fixed), Root.rates)
        assertEquals(usual, own().filter { it.first === ticker })
        natives.frame()
        frame()
        assertEquals(usual, own())
        // An uncapped canvas's frame beside the capped one's.
        frame(7, 8)
        assertEquals(listOf(60f to fixed), Root.rates)
        assertEquals(1, ticks)
        assertEquals(usual, own().filter { it.first === ticker })
    }

    @Test fun cappedCanvasesVoteTheHighestCapOfThoseStillDrawing() {
        // Of 24 and 60 neither divides the other: voted both in one frame, the view root sends the window's layer
        // neither (`ViewRootImpl.votePreferredFrameRate` sets `mIsFrameRateConflicted`).
        natives.frameRates(intArrayOf(7, 24, 8, 60))
        frame(7, 8)
        assertEquals(listOf(60f to fixed, 60f to fixed), Root.rates)
        frame(7)
        assertEquals(listOf(60f to fixed), Root.rates)
        // Each view keeps its own cap, which the vote stands on.
        assertEquals(listOf(24f, 60f), (0..1).map { natives.getChildAt(it).requestedFrameRate })
        natives.frameRates(intArrayOf(8, 0))
        frame(7)
        assertEquals(listOf(24f to fixed), Root.rates)
        // A canvas that rests leaves the vote an interval of its cap and 100 ms after its last frame, about when the
        // view root would have dropped its rate had it drawn alone. Its next frame votes its cap again.
        natives.frameRates(intArrayOf(7, 60, 8, 120))
        frame(7, 8)
        assertEquals(listOf(120f to fixed, 120f to fixed), Root.rates)
        val rested = SystemClock.uptimeMillis()
        ShadowSystemClock.advanceBy(Duration.ofMillis(90))
        frame(7)
        assertEquals("${SystemClock.uptimeMillis() - rested} ms after its last frame", listOf(120f to fixed), Root.rates)
        frame(7)
        assertEquals("${SystemClock.uptimeMillis() - rested} ms after its last frame", listOf(60f to fixed), Root.rates)
        frame(8)
        assertEquals(listOf(120f to fixed), Root.rates)
        frame(7)
        assertEquals(listOf(120f to fixed), Root.rates)
    }
}
