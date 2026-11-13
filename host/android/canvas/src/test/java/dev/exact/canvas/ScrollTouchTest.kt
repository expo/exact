package dev.exact.canvas

import android.app.Activity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.widget.FrameLayout
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.LooperMode
import org.robolectric.shadow.api.Shadow

/**
 * A finger on the reader's view, through [ExactView.onTouchEvent] as the platform delivers it: [Scrolling], the
 * hosted fields' dispatcher ([Fields]) and what reaches exact2, with exact2's answer for the touch's down
 * ([Native.under]) given when the test says, as a busy exact2 thread gives it late. Each gesture is played twice,
 * answered at once and answered after the finger has done all it does, and must scroll the same.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ScrollTouchTest.Thread::class, ScrollTouchTest.Host::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@LooperMode(LooperMode.Mode.PAUSED)
class ScrollTouchTest {
    /** exact2's thread: what the reader sends it, and the answers it owes. */
    @Implements(Owner::class, isInAndroidSdk = false)
    class Thread {
        /** Each touch exact2 heard: 0 down, 1 up, 2 move, 3 cancel. */
        val touches = ArrayList<Int>()
        /** How far the reader moved each scroller, by id. */
        val moved = HashMap<Int, Float>()
        val answers = ArrayDeque<(IntArray) -> Unit>()
        @Implementation fun __constructor__(hwbuf: Boolean, fast: Boolean, priority: Int, intent: (String) -> String?) {}
        @Implementation fun under(x: Float, y: Float, done: (IntArray) -> Unit) { answers.addLast(done) }
        @Implementation fun touch(action: Int, x: Float, y: Float, tool: Int, waitMs: Long): Boolean { touches.add(action); return false }
        @Implementation fun finger(action: Int, id: Int, x: Float, y: Float, waitMs: Long) = false
        @Implementation fun scrolled(id: Int, dx: Float, dy: Float, rev: Int, seq: Int) {
            moved[id] = (moved[id] ?: 0f) + dx + dy
            Host.told.add("move $id")
        }
        @Implementation fun answer(waitMs: Long, f: () -> Unit) = false
        @Implementation fun call(f: () -> Unit) = f()
        @Implementation fun input() {}
        @Implementation fun held(on: Boolean) {}
        @Implementation fun running(on: Boolean) {}
        @Implementation fun timerReader(view: Any) {}
        @Implementation fun timerReport(view: Any, scrolling: Boolean, sampling: Boolean, applied: Long, began: Long, ended: Long) {}
    }

    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            val fieldInputs = ArrayList<Int>()
            @JvmStatic @Implementation fun fieldInput(id: Int, generation: Int, kind: Int) { fieldInputs.add(kind) }
            @JvmStatic @Implementation fun fieldAction(id: Int, generation: Int, action: Int) {}
            @JvmStatic @Implementation fun fieldEdited(id: Int, generation: Int, seq: Int, value: String, start: Int, end: Int) {}
            @JvmStatic @Implementation fun fieldFocused(id: Int, generation: Int, focused: Boolean) {}
            @JvmStatic @Implementation fun fieldSelected(id: Int, generation: Int, start: Int, end: Int) {}
            /** What exact2's thread was told of scrolling, in order: each move, and each drag's start and end. */
            val told = ArrayList<String>()
            val ends = ArrayList<FloatArray>()
            @JvmStatic @Implementation fun scrollDrag(id: Int, ended: Boolean, vx: Float, vy: Float, rest: Boolean, restX: Float, restY: Float, flings: Boolean) {
                told.add(if (!ended) "start $id" else if (flings) "end $id flings" else "end $id")
                if (ended) ends.add(floatArrayOf(vx, vy, restX, restY))
            }
            @JvmStatic @Implementation fun flingEnded(id: Int) { told.add("rest $id") }
            @JvmStatic @Implementation fun refresh(id: Int) { told.add("refresh $id") }
        }
    }

    private lateinit var activity: Activity
    private lateinit var view: ExactView
    private lateinit var thread: Thread
    private val slop get() = ViewConfiguration.get(activity).scaledTouchSlop
    private var time = 0L

    @Before fun setUp() {
        activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        activity.intent.putExtra("warm", "0")
        view = ExactView(activity, false) {}
        activity.setContentView(FrameLayout(activity).also { it.addView(view, FrameLayout.LayoutParams(400, 800)) })
        val owner = Owner(false, false, 0) { null }
        thread = Shadow.extract(owner)
        Host.told.clear(); Host.ends.clear()
        Host.fieldInputs.clear()
        view.owner = owner
        view.ownerStarted(true)
    }

    /** A vertical scroller [id] at [offset] of [max], its port the view's top 400 by 800. */
    private fun scroller(id: Int, offset: Float, max: Float = 500f) =
        intArrayOf(id, 2, 0, 0, offset.toBits(), 0, max.toBits(), 0, 0, 400f.toBits(), 800f.toBits())

    /** exact2's answer: no claims, these scrollers innermost first, [field] under the touch. */
    private fun reply(vararg scrollers: IntArray, field: Int = 0) =
        intArrayOf(0, scrollers.size, field) + scrollers.fold(IntArray(0)) { a, s -> a + s }

    private fun answer(words: IntArray) = thread.answers.removeFirst()(words)

    private fun touch(action: Int, vararg fingers: Pair<Float, Float>, index: Int = 0) {
        val props = Array(fingers.size) { k -> MotionEvent.PointerProperties().apply { id = k; toolType = MotionEvent.TOOL_TYPE_FINGER } }
        val coords = Array(fingers.size) { k -> MotionEvent.PointerCoords().apply { x = fingers[k].first; y = fingers[k].second } }
        val e = MotionEvent.obtain(0, time, action or (index shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), fingers.size,
            props, coords, 0, 0, 1f, 1f, 0, 0, android.view.InputDevice.SOURCE_TOUCHSCREEN, 0)
        time += 10
        view.dispatchTouchEvent(e)
        e.recycle()
    }
    private fun down(y: Float) = touch(MotionEvent.ACTION_DOWN, 100f to y)
    private fun move(y: Float) = touch(MotionEvent.ACTION_MOVE, 100f to y)
    private fun up(y: Float) = touch(MotionEvent.ACTION_UP, 100f to y)

    /** [gesture] from a down at y 400, its answer [words] given at once and then given after it: what each scrolled,
     * by scroller, and what exact2 heard. */
    private fun both(words: IntArray, gesture: () -> Unit): List<Pair<Map<Int, Float>, List<Int>>> = listOf(true, false).map { soon ->
        setUp()
        down(400f)
        if (soon) answer(words)
        gesture()
        if (!soon) answer(words)
        HashMap(thread.moved) to ArrayList(thread.touches)
    }

    @Test fun fieldInputSeparatesPointerHardwareAndImeModality() {
        field(keeps = false)
        down(400f)
        answer(reply(field = 1))
        up(400f)
        assertEquals(listOf(1), Host.fieldInputs)
        val text = view.fields!!.text(1)!!
        Host.fieldInputs.clear()
        text.dispatchKeyEvent(android.view.KeyEvent(android.view.KeyEvent.ACTION_DOWN, android.view.KeyEvent.KEYCODE_DPAD_LEFT))
        assertEquals(listOf(2), Host.fieldInputs)
        Host.fieldInputs.clear()
        text.dispatchKeyEvent(android.view.KeyEvent(0, 0, android.view.KeyEvent.ACTION_DOWN,
            android.view.KeyEvent.KEYCODE_A, 0, android.view.KeyEvent.META_CTRL_ON))
        assertEquals(listOf(0), Host.fieldInputs)
        for (flags in listOf(android.view.KeyEvent.FLAG_SOFT_KEYBOARD, android.view.KeyEvent.FLAG_KEEP_TOUCH_MODE,
                android.view.KeyEvent.FLAG_SOFT_KEYBOARD or android.view.KeyEvent.FLAG_KEEP_TOUCH_MODE)) {
            for (code in listOf(android.view.KeyEvent.KEYCODE_DEL, android.view.KeyEvent.KEYCODE_ENTER, android.view.KeyEvent.KEYCODE_DPAD_LEFT)) {
                Host.fieldInputs.clear()
                text.dispatchKeyEvent(android.view.KeyEvent(0, 0, android.view.KeyEvent.ACTION_DOWN, code, 0, 0,
                    android.view.KeyCharacterMap.VIRTUAL_KEYBOARD, 0, flags))
                assertEquals("IME flags $flags, key $code", listOf(0), Host.fieldInputs)
            }
        }
        Host.fieldInputs.clear()
        text.onCreateInputConnection(android.view.inputmethod.EditorInfo())!!.deleteSurroundingText(1, 0)
        assertTrue(Host.fieldInputs.isNotEmpty())
        assertTrue(Host.fieldInputs.all { it == 0 })
    }

    @Test fun aLateAnswerTakesTheWayTheFingerWentPastTheSlop() {
        val (soon, late) = both(reply(scroller(1, 0f))) { move(390f); move(370f); move(350f) }
        assertEquals(50f - slop, soon.first[1]!!, 0.01f)
        assertEquals(soon.first, late.first)
        assertTrue(slop in 11..29)
        assertEquals("answered at once, exact2 hears the move within the slop", listOf(0, 2, 3), soon.second)
        assertEquals("answered late, it heard the moves until the scroll took the touch", listOf(0, 2, 2, 2, 3), late.second)
    }

    @Test fun aFingerThatLiftedBeforeTheAnswerScrollsAndFlings() {
        val (soon, late) = both(reply(scroller(1, 0f))) { move(380f); move(340f); move(300f); up(300f) }
        assertEquals(100f - slop, late.first[1]!!, 0.01f)
        assertEquals(soon.first, late.first)
        assertTrue("the lift flings", view.scrolling().timerActive)
        // exact2 heard the lift before the touch was known to be a scroll: it has nothing to cancel.
        assertEquals(listOf(0, 2, 2, 2, 1), late.second)
    }

    @Test fun theUpsOwnSampleMovesNothing() {
        // The platform's lists take a lift's position into its velocity, never into the drag.
        val (soon, late) = both(reply(scroller(1, 0f))) { move(380f); move(350f); up(320f) }
        assertEquals(50f - slop, late.first[1]!!, 0.01f)
        assertEquals(soon.first, late.first)
    }

    @Test fun aTurnAtTheEndIsNotLostToTheNetWay() {
        // Down past the top first, which nothing takes, then up past where the finger began: the scroll is the way
        // up from the turn, as it is live, not the way up from the down.
        val (soon, late) = both(reply(scroller(1, 0f))) { move(420f); move(440f); move(400f); move(360f) }
        assertEquals(80f, late.first[1]!!, 0.01f)
        assertEquals(soon.first, late.first)
    }

    @Test fun aTurnHandsOffBetweenNestedScrollersInOrder() {
        // The inner list is at its top and the outer one 100 down: down goes to the outer one, up to the inner.
        val words = reply(scroller(1, 0f), scroller(2, 100f))
        val (soon, late) = both(words) { move(420f); move(440f); move(400f); move(360f) }
        assertEquals(-(40f - slop), late.first[2]!!, 0.01f)
        assertEquals(80f, late.first[1]!!, 0.01f)
        assertEquals(soon.first, late.first)
    }

    @Test fun aSecondFingerTakesOverFromAFirstThatLiftedBeforeTheAnswer() {
        val (soon, late) = both(reply(scroller(1, 0f))) {
            touch(MotionEvent.ACTION_POINTER_DOWN, 100f to 400f, 200f to 400f, index = 1)
            touch(MotionEvent.ACTION_POINTER_UP, 100f to 400f, 200f to 400f, index = 0)
            // The second finger is all that is down: it is pointer 1, at index 0.
            secondFinger(MotionEvent.ACTION_MOVE, 360f)
            secondFinger(MotionEvent.ACTION_MOVE, 340f)
        }
        assertEquals(60f - slop, late.first[1]!!, 0.01f)
        assertEquals(soon.first, late.first)
        // The drag has a finger: it goes on moving the list, and its lift ends it.
        secondFinger(MotionEvent.ACTION_MOVE, 300f)
        assertEquals(100f - slop, thread.moved[1]!!, 0.01f)
        secondFinger(MotionEvent.ACTION_UP, 300f)
        assertFalse(view.scrolling().dragging())
    }

    private fun secondFinger(action: Int, y: Float) {
        val props = arrayOf(MotionEvent.PointerProperties().apply { id = 1; toolType = MotionEvent.TOOL_TYPE_FINGER })
        val coords = arrayOf(MotionEvent.PointerCoords().apply { x = 200f; this.y = y })
        val e = MotionEvent.obtain(0, time, action, 1, props, coords, 0, 0, 1f, 1f, 0, 0, android.view.InputDevice.SOURCE_TOUCHSCREEN, 0)
        time += 10
        view.dispatchTouchEvent(e)
        e.recycle()
    }

    @Test fun anAnswerAfterTheActivityStoppedMovesNothing() {
        down(400f); move(380f); move(340f); move(300f); up(300f)
        view.running(false)
        answer(reply(scroller(1, 0f)))
        assertNull(thread.moved[1])
        assertFalse("no fling in a stopped window", view.scrolling().timerActive)
    }

    @Test fun anAnswerAfterTheViewLeftItsWindowMovesNothing() {
        down(400f); move(380f); move(340f); move(300f); up(300f)
        (view.parent as FrameLayout).removeView(view)
        answer(reply(scroller(1, 0f)))
        assertNull(thread.moved[1])
        assertFalse(view.scrolling().timerActive)
    }

    // A hosted text field under the finger ([Fields]): the touch is held for it until the answer.

    private fun field(keeps: Boolean): ArrayList<Int> {
        val fields = Fields(activity)
        (view.parent as FrameLayout).addView(fields)
        view.fields = fields
        fields.view = view
        fields.told(FieldsTest.statement(x = 50f, y = 380f, w = 100f, h = 40f))
        val got = ArrayList<Int>()
        fields.text(1)!!.setOnTouchListener { v, e ->
            got.add(e.actionMasked)
            // A field that drags its selection asks the views around it to leave it the touch.
            if (keeps && e.actionMasked == MotionEvent.ACTION_MOVE) v.parent.requestDisallowInterceptTouchEvent(true)
            false
        }
        return got
    }

    @Test fun aFieldGetsWhatItHeldUntilTheScrollTakesIt() {
        for (soon in listOf(true, false)) {
            setUp()
            val got = field(keeps = false)
            down(400f)
            if (soon) answer(reply(scroller(1, 0f), field = 1))
            move(396f); move(380f); move(360f); up(360f)
            if (!soon) answer(reply(scroller(1, 0f), field = 1))
            assertEquals("answered at once: $soon", listOf(MotionEvent.ACTION_DOWN, MotionEvent.ACTION_MOVE, MotionEvent.ACTION_CANCEL), got)
            assertEquals(40f - slop, thread.moved[1]!!, 0.01f)
            assertEquals("exact2 heard nothing of the field's touch", emptyList<Int>(), thread.touches)
        }
    }

    @Test fun aFieldThatKeepsItsTouchKeepsItWhenTheAnswerIsLate() {
        for (soon in listOf(true, false)) {
            setUp()
            val got = field(keeps = true)
            down(400f)
            if (soon) answer(reply(scroller(1, 0f), field = 1))
            move(396f); move(380f); move(360f); up(360f)
            if (!soon) answer(reply(scroller(1, 0f), field = 1))
            assertEquals("answered at once: $soon", listOf(MotionEvent.ACTION_DOWN, MotionEvent.ACTION_MOVE, MotionEvent.ACTION_MOVE,
                MotionEvent.ACTION_MOVE, MotionEvent.ACTION_UP), got)
            assertNull(thread.moved[1])
        }
    }

    @Test fun aTouchInAFieldTheAnswerDoesNotGiveItGoesToExact2UntilTheScrollTakesIt() {
        val got = field(keeps = false)
        down(400f); move(396f); move(380f); move(360f); up(360f)
        answer(reply(scroller(1, 0f), field = 0))
        assertEquals(emptyList<Int>(), got)
        assertEquals(listOf(0, 2, 3), thread.touches)
        assertEquals(40f - slop, thread.moved[1]!!, 0.01f)
    }

    // What exact2 is told of the drag itself: its `scrolldragstart` and `scrolldragend` ([Native.scrollDrag]).

    @Test fun aDragIsToldOnceBeforeItsFirstMoveAndItsEndWithTheFlingsRest() {
        down(400f); answer(reply(scroller(1, 0f), scroller(2, 0f)))
        move(380f); move(340f); move(300f); up(300f)
        // The innermost the drag moves is the one dragged; its moves follow its start, the end follows them.
        assertEquals(listOf("start 1", "move 1", "move 1", "move 1", "end 1 flings"), Host.told)
        val (vx, vy, restX, restY) = Host.ends.single().toList()
        assertEquals(0f, vx)
        assertTrue("a finger moving up scrolls toward the end: positive, px/s", vy > 1000f)
        assertTrue("the fling's rest is past where the lift left it", restY > 100f - slop)
        assertEquals(0f, restX)
        assertTrue(view.scrolling().timerActive)
    }

    @Test fun aFlingsEndIsToldWhenItStopsOrAFingerCatchesIt() {
        down(400f); answer(reply(scroller(1, 0f, max = 100000f)))
        move(380f); move(340f); move(300f); up(300f)
        org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idleFor(java.time.Duration.ofSeconds(10))
        assertFalse(view.scrolling().timerActive)
        // The fling's moves come between its drag's end and its own.
        val end = Host.told.indexOf("end 1 flings")
        assertTrue(end > 0 && Host.told.subList(end + 1, Host.told.size - 1).all { it == "move 1" } && end + 1 < Host.told.size - 1)
        assertEquals("rest 1", Host.told.last())
        Host.told.clear()
        down(400f); answer(reply(scroller(1, 0f, max = 100000f)))
        move(380f); move(340f); move(300f); up(300f)
        // A finger on the list while it flings stops it and has it: the fling is over.
        down(400f)
        assertEquals(listOf("end 1 flings", "rest 1"), Host.told.takeLast(2))
        // It is dragged once the finger moves it.
        move(380f)
        assertEquals(listOf("start 1", "move 1"), Host.told.takeLast(2))
    }

    @Test fun aDragLetGoOfStillRestsWhereItIs() {
        down(400f); answer(reply(scroller(1, 0f)))
        move(380f); move(340f)
        time += 500
        move(340f); up(340f)
        val end = Host.ends.single()
        assertTrue(kotlin.math.abs(end[1]) < ViewConfiguration.get(activity).scaledMinimumFlingVelocity)
        assertEquals(60f - slop, end[3], 0.01f)
        assertFalse(view.scrolling().timerActive)
    }

    @Test fun aDragTheSystemTakesOrAStopEndsIsToldItsEnd() {
        down(400f); answer(reply(scroller(1, 0f)))
        move(380f); move(340f)
        touch(MotionEvent.ACTION_CANCEL, 100f to 340f)
        assertEquals(listOf("start 1", "move 1", "move 1", "end 1"), Host.told)
        assertEquals(0f, Host.ends.single()[1])
        Host.told.clear()
        down(400f); answer(reply(scroller(1, 0f)))
        move(380f)
        view.running(false)
        assertEquals(listOf("start 1", "move 1", "end 1"), Host.told)
    }

    @Test fun aTouchThatNeverScrollsTellsNoDrag() {
        down(400f); answer(reply(scroller(1, 0f)))
        move(398f); up(398f)
        assertEquals(emptyList<String>(), Host.told)
    }

    // What a drag does to the keyboard: its scroller's `keyboardDismissMode` (bits 6 and 7 of its flags).

    private fun focusedField(): android.widget.EditText {
        field(keeps = false)
        view.isFocusableInTouchMode = true
        val text = view.fields!!.text(1)!!
        assertTrue(text.requestFocus())
        return text
    }

    private fun dragWith(dismiss: Int) {
        down(600f); answer(reply(scroller(1, 0f).also { it[1] = 2 or (dismiss shl 6) }))
        move(580f); move(540f)
    }

    @Test fun onDragPutsTheKeyboardAwayAsTheDragStarts() {
        val text = focusedField()
        dragWith(Dismissal.ON_DRAG)
        assertFalse("the field let go of the focus, as the person's own blur", text.isFocused)
    }

    @Test fun unsaidOrNoneOrInteractiveLeavesTheFieldFocusedAtTheStart() {
        for (dismiss in listOf(0, Dismissal.INTERACTIVE)) {
            setUp()
            val text = focusedField()
            dragWith(dismiss)
            up(540f)
            assertTrue("dismiss $dismiss", text.isFocused)
        }
    }

    // Pull to refresh: a scroller that hears `refresh` (flag bit 4), refreshing while bit 5 is set.

    private val main get() = org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper())
    private fun pull(): Refresh? = (view.parent as FrameLayout).let { p -> (0 until p.childCount).map { p.getChildAt(it) } }.filterIsInstance<Refresh>().singleOrNull()
    private fun refreshable(refreshing: Boolean = false) = scroller(1, 0f).also { it[1] = 2 or 16 or (if (refreshing) 32 else 0) }
    private fun statement(refreshing: Boolean) =
        view.scrolling().told(intArrayOf(1, 2 or 16 or (if (refreshing) 32 else 0), 0, 0, 0, 0, 0, 0, 500f.toBits()))

    @Test fun aPullPastTheThresholdSendsRefreshAndTurnsUntilRefreshingEnds() {
        down(300f); answer(reply(refreshable()))
        move(320f); move(400f); move(500f)
        assertNull("the list does not move", thread.moved[1])
        val r = pull()!!
        assertTrue(r.fraction > 1f)
        up(500f)
        assertEquals("refresh 1", Host.told.last())
        assertTrue(r.spinning)
        statement(refreshing = true)
        main.idleFor(java.time.Duration.ofSeconds(2))
        assertTrue("refreshing keeps it turning past the wait for an app that starts nothing", r.spinning && pull() === r)
        assertEquals("it rests at the threshold", 1f, r.fraction, 0.001f)
        statement(refreshing = false)
        main.idleFor(java.time.Duration.ofSeconds(1))
        assertNull("it went back up and away", pull())
    }

    @Test fun aRefreshTheAppNeverStartsEndsAfterItsWait() {
        down(300f); answer(reply(refreshable()))
        move(320f); move(400f); move(500f); up(500f)
        assertEquals("refresh 1", Host.told.last())
        assertTrue(pull()!!.spinning)
        main.idleFor(java.time.Duration.ofSeconds(2))
        assertNull(pull())
    }

    @Test fun aPullShortOfTheThresholdSendsNothing() {
        down(300f); answer(reply(refreshable()))
        move(320f); move(360f)
        assertTrue(pull()!!.fraction in 0.01f..0.99f)
        up(360f)
        main.idleFor(java.time.Duration.ofSeconds(1))
        assertFalse(Host.told.contains("refresh 1"))
        assertNull(pull())
    }

    @Test fun aPullTakesTheFingerBackBeforeTheListScrolls() {
        down(300f); answer(reply(refreshable()))
        move(320f); move(400f)
        move(250f)
        assertEquals(150f - (100f - slop), thread.moved[1]!!, 0.01f)
        assertEquals(0f, pull()!!.fraction)
    }

    @Test fun aListThatIsRefreshingStretchesInstead() {
        down(300f); answer(reply(refreshable(refreshing = true)))
        move(320f); move(400f); move(500f); up(500f)
        assertNull(pull())
        assertFalse(Host.told.contains("refresh 1"))
    }

    @Test fun aListThatHearsNoRefreshIsNeverPulled() {
        down(300f); answer(reply(scroller(1, 0f)))
        move(320f); move(400f); move(500f); up(500f)
        assertNull(pull())
    }

    // What review of the first version found.

    @Test fun aTouchThatEndedBeforeItsAnswerScrollsThoughALaterTouchBegan() {
        down(400f); move(380f); move(340f); move(300f); up(300f)
        // The next touch comes down before exact2 has answered for the first.
        down(400f)
        answer(reply(scroller(1, 0f)))
        assertEquals(100f - slop, thread.moved[1]!!, 0.01f)
        assertEquals("its fling stopped where the next touch came down",
            listOf("start 1", "move 1", "move 1", "move 1", "end 1 flings", "rest 1"), Host.told)
        assertEquals("exact2 is told to cancel nothing: the first ended, the second is its own", listOf(0, 2, 2, 2, 1, 0), thread.touches)
        answer(reply(scroller(1, 0f)))
        move(380f); move(340f)
        assertEquals("the second scrolls on from there", 100f - slop + 60f - slop, thread.moved[1]!!, 0.01f)
    }

    @Test fun aNestedDragTellsAndHoldsTheScrollerThatMoves() {
        // The inner list at its top, the outer one 100 down: a drag down moves only the outer one.
        down(400f); answer(reply(scroller(1, 0f), scroller(2, 100f)))
        move(420f); move(440f)
        assertEquals(listOf("start 2", "move 2", "move 2"), Host.told)
        // Back up: the way goes to the inner one first, which is dragged from then.
        move(300f)
        assertEquals(listOf("start 2", "move 2", "move 2", "start 1", "move 1"), Host.told)
        up(300f)
        assertTrue(Host.told.containsAll(listOf("end 2", "end 1")) || Host.told.containsAll(listOf("end 2 flings", "end 1 flings")))
    }

    @Test fun aNestedDragReadsTheKeyboardOfTheScrollerThatMoves() {
        for ((inner, outer) in listOf(Dismissal.ON_DRAG to 0, 0 to Dismissal.ON_DRAG)) {
            setUp()
            val text = focusedField()
            down(600f)
            answer(reply(scroller(1, 0f).also { it[1] = 2 or (inner shl 6) }, scroller(2, 100f).also { it[1] = 2 or (outer shl 6) }))
            move(620f); move(640f)
            assertEquals("the outer list moved; inner $inner, outer $outer", outer == 0, text.isFocused)
        }
    }

    @Test fun aPullTakenAllTheWayBackScrollsAndFlingsAsAnyDrag() {
        down(300f); answer(reply(refreshable()))
        move(320f); move(400f)
        move(250f); move(150f); move(50f); up(50f)
        assertTrue(thread.moved[1]!! > 0f)
        assertTrue("it flings", view.scrolling().timerActive)
        assertFalse(Host.told.contains("refresh 1"))
    }

    @Test fun aRefreshingListThatGoesStopsItsIndicator() {
        down(300f); answer(reply(refreshable()))
        move(320f); move(400f); move(500f); up(500f)
        statement(refreshing = true)
        val r = pull()!!
        assertTrue(r.turning)
        // The app takes the list away while it refreshes: the statement no longer has it.
        view.scrolling().told(intArrayOf(2, 2, 0, 0, 0, 0, 0, 0, 500f.toBits()))
        assertNull(pull())
        assertFalse("its animator does not run on, holding the view", r.turning)
    }

    private fun ExactView.scrolling(): Scrolling =
        ExactView::class.java.getDeclaredField("scrolling").apply { isAccessible = true }.get(this) as Scrolling

    private fun Scrolling.dragging(): Boolean =
        Scrolling::class.java.getDeclaredField("mode").apply { isAccessible = true }.getInt(this) == 3
}
