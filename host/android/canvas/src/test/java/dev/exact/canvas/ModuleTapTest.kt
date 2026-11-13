package dev.exact.canvas

import android.app.Activity
import android.content.Context
import android.os.Looper
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Before
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
import org.robolectric.shadow.api.Shadow

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ScrollTouchTest.Thread::class, ModuleTapTest.Host::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@LooperMode(LooperMode.Mode.PAUSED)
class ModuleTapTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            val events = ArrayList<Pair<Long, String>>()
            @JvmStatic @Implementation fun moduleEvent(id: Int, slot: Int, generation: Int, session: Long, kind: String, payload: String) {
                events.add(session to payload)
            }
        }
    }

    private class Button : NativeModule {
        var clicks = 0
        val bindings = ArrayList<(String, String) -> Unit>()
        override fun create(context: Context, props: JSONObject) = View(context).apply {
            setOnClickListener { clicks++ }
        }
        override fun update(view: View, props: JSONObject) {}
        override fun events(view: View, emit: (String, String) -> Unit) { bindings.add(emit) }
    }

    private lateinit var view: ExactView
    private lateinit var thread: ScrollTouchTest.Thread
    private lateinit var button: Button
    private var time = 0L

    @Before fun setUp() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        activity.intent.putExtra("warm", "0")
        view = ExactView(activity, false) {}
        val natives = Natives(activity, activity.filesDir, mapOf("test-button" to Button().also { button = it }))
        activity.setContentView(FrameLayout(activity).also {
            it.addView(natives)
            it.addView(view, FrameLayout.LayoutParams(400, 800))
        })
        view.natives = natives
        val owner = Owner(false, false, 0) { null }
        thread = Shadow.extract(owner)
        view.owner = owner
        view.ownerStarted(true)
        Exact.owner = null
        Host.events.clear()
        ensure(1)
    }

    private fun ensure(session: Long) = view.natives!!.ensure(1, 2, 100, 40,
        JSONObject().put("module", "test-button").put("slot", 3).put("generation", 4).put("session", session).toString(), 0)

    private fun reply(session: Long = 1, longPresses: Int = 0) = intArrayOf(
        0, 0, 0, 1, 0, 0, 100f.toBits(), 40f.toBits(), 3, 4, longPresses, session.toInt(), (session ushr 32).toInt(),
    )

    private fun answer(session: Long = 1, longPresses: Int = 0) = thread.answers.removeFirst()(reply(session, longPresses))

    private fun touch(action: Int, x: Float = 20f) {
        time = maxOf(time, android.os.SystemClock.uptimeMillis())
        val e = MotionEvent.obtain(0, time, action, x, 20f, 0)
        time += 10
        view.dispatchTouchEvent(e)
        e.recycle()
    }

    @Test fun rapidTapsKeepTheirOwnAnswersAndReleaseBeforeTheNextDown() {
        repeat(3) { touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_UP) }
        assertEquals(listOf(0), thread.touches)
        repeat(3) { i ->
            answer()
            assertEquals(i + 1, button.clicks)
            assertEquals(List(i + 1) { listOf(0, 3) }.flatten() + if (i < 2) listOf(0) else emptyList(), thread.touches)
        }
    }

    @Test fun aTapClicksOnceWhetherItsAnswerPrecedesOrFollowsTheRelease() {
        for (soon in listOf(true, false)) {
            touch(MotionEvent.ACTION_DOWN)
            if (soon) answer()
            touch(MotionEvent.ACTION_UP)
            if (!soon) answer()
        }
        assertEquals(2, button.clicks)
        assertEquals(listOf(0, 3, 0, 3), thread.touches)
    }

    @Test fun reloadRebindsAnOtherwiseIdenticalModuleAndRetiresQueuedEvents() {
        val old = button.bindings.single()
        old("change", "queued before reload")
        ensure(2)
        assertEquals(2, button.bindings.size)
        old("change", "old popup")
        button.bindings.last()("change", "new popup")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(2L to "new popup"), Host.events)
    }

    @Test fun aReplyFromBeforeReloadCannotClickTheNewControl() {
        touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_UP)
        ensure(2)
        answer(1)
        assertEquals(0, button.clicks)
        assertEquals(listOf(0, 1), thread.touches)
    }

    @Test fun theWholeSessionTokenReachesClicksAndEvents() {
        val session = 0x12345678fedcba98L
        ensure(session)
        touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_UP)
        answer(session)
        assertEquals(1, button.clicks)
        button.bindings.last()("change", "current")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(session to "current"), Host.events)
    }

    @Test fun cancellationAndMovementRetireOnlyTheirOwnAnswer() {
        for (cancel in listOf(true, false)) {
            touch(MotionEvent.ACTION_DOWN)
            touch(if (cancel) MotionEvent.ACTION_CANCEL else MotionEvent.ACTION_MOVE, 100f)
            if (!cancel) touch(MotionEvent.ACTION_UP, 100f)
            touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_UP)
            answer()
            assertEquals(if (cancel) 0 else 1, button.clicks)
            answer()
            assertEquals(if (cancel) 1 else 2, button.clicks)
        }
    }

    @Test fun cancellingAQueuedContactPreservesTheCompletedTapBeforeIt() {
        touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_UP)
        touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_CANCEL)
        answer()
        assertEquals(1, button.clicks)
        answer()
        assertEquals(1, button.clicks)
        assertEquals(listOf(0, 3, 0, 3), thread.touches)
    }

    @Test fun stoppingDiscardsCompletedAndQueuedContactsBeforeTheirAnswers() {
        touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_UP)
        touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_UP)
        view.running(false)
        answer()
        assertEquals(0, button.clicks)
        assertTrue(thread.answers.isEmpty())
        assertEquals(listOf(0, 3), thread.touches)
    }

    @Test fun aTakenLongPressDoesNotClickAndTheFollowingTapStillDoes() {
        touch(MotionEvent.ACTION_DOWN)
        shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(
            android.view.ViewConfiguration.getLongPressTimeout().toLong()))
        touch(MotionEvent.ACTION_UP)
        touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_UP)
        answer(longPresses = 0)
        assertEquals(0, button.clicks)
        answer(longPresses = 1)
        assertEquals(0, button.clicks)
        answer(longPresses = 1)
        assertEquals(1, button.clicks)
        assertEquals(listOf(0, 4, 1, 0, 3), thread.touches)
    }

    @Test fun aCompletedQueuedLongPressKeepsTheTimeItWasHeld() {
        touch(MotionEvent.ACTION_DOWN); touch(MotionEvent.ACTION_UP)
        touch(MotionEvent.ACTION_DOWN)
        shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(
            2L * android.view.ViewConfiguration.getLongPressTimeout()))
        touch(MotionEvent.ACTION_UP)
        answer()
        assertEquals(1, button.clicks)
        assertEquals(listOf(0, 3, 0, 4), thread.touches)
        answer(longPresses = 0)
        answer(longPresses = 1)
        assertEquals(1, button.clicks)
        assertEquals(listOf(0, 3, 0, 4, 1), thread.touches)
    }
}
