package dev.exact.canvas

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.os.HandlerThread
import android.view.View
import android.widget.EditText
import android.widget.FrameLayout
import androidx.activity.OnBackPressedCallback
import androidx.activity.compose.BackHandler
import androidx.activity.findViewTreeOnBackPressedDispatcherOwner
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.findViewTreeLifecycleOwner
import androidx.lifecycle.findViewTreeViewModelStoreOwner
import androidx.savedstate.findViewTreeSavedStateRegistryOwner
import dev.exact.app.materialPickers
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.util.ReflectionHelpers
import java.nio.IntBuffer

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [HeadlessLaunchTest.Host::class, AuthTest.Environment::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ComposeModuleTest {
    @get:Rule val compose = createEmptyComposeRule()
    class Module : NativeModule {
        lateinit var view: ComposeView
        var shown = ""
        var change: (String) -> Unit = {}
        var disposed = false
        lateinit var remembered: Any
        override fun create(context: Context, props: JSONObject): View = ComposeView(context).also {
            disposed = false
            view = it
            it.id = 1234 // Stable across Activity recreation, as rememberSaveable requires.
            it.setContent {
                remembered = remember { Any() }
                val text = rememberSaveable { mutableStateOf("hello") }
                SideEffect { shown = text.value; change = { text.value = it } }
                DisposableEffect(Unit) { onDispose { disposed = true } }
                BasicText(text.value)
            }
        }
        override fun update(view: View, props: JSONObject) {}
        override fun destroy(view: View) { (view as ComposeView).disposeComposition() }
    }

    class PlainView(context: Context) : View(context) {
        var value = ""
        var returnedParent: android.view.ViewParent? = null
        override fun onFinishTemporaryDetach() { super.onFinishTemporaryDetach(); returnedParent = parent }
    }

    class PlainModule : NativeModule {
        val destroyed = mutableListOf<View>()
        val events = mutableListOf<Pair<View, ViewLifecycle>>()
        val states = mutableListOf<Pair<ViewLifecycle, String?>>()
        override fun lifecycle(view: View, event: ViewLifecycle, state: Bundle?) {
            events += view to event
            val value = (view as PlainView).value
            if (event == ViewLifecycle.SAVE_INSTANCE_STATE) state!!.putString("value", value)
            states += event to (if (event == ViewLifecycle.CREATE) state?.getString("value") else value)
        }
        override fun create(context: Context, props: JSONObject) = PlainView(context)
        override fun update(view: View, props: JSONObject) { (view as PlainView).value = props.optString("value") }
        override fun destroy(view: View) { destroyed += view }
    }

    class Reader : ExactActivity() {
        var platform: android.window.OnBackInvokedDispatcher? = null
        @android.annotation.TargetApi(33)
        override fun getOnBackInvokedDispatcher() = platform ?: super.getOnBackInvokedDispatcher()

        companion object { internal var publication: Owner? = null }
        val module = Module()
        val plain = PlainModule()
        lateinit var nestedMemory: Any
        val nested = object : NativeModule {
            override fun create(context: Context, props: JSONObject) = FrameLayout(context).apply {
                addView(EditText(context))
                addView(EditText(context))
                addView(ComposeView(context).apply {
                    setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnDetachedFromWindow)
                    setContent { nestedMemory = remember { Any() } }
                })
            }
            override fun update(view: View, props: JSONObject) {}
        }
        override val modules = mapOf("hello-compose" to module, "plain" to plain, "nested" to nested)
        internal lateinit var content: Root
        internal lateinit var hosted: Natives
        override fun getDisplay(): android.view.Display =
            getSystemService(android.hardware.display.DisplayManager::class.java).getDisplay(0)
        override fun onCreate(state: Bundle?) {
            super.onCreate(state)
            content = findViewById<FrameLayout>(android.R.id.content).getChildAt(0) as Root
        }
        override fun setContentView(view: View) {
            publication?.let { owner ->
                content = view as Root
                assertFalse(content.isAttachedToWindow)
                val reader = (0 until content.childCount).map { content.getChildAt(it) }.filterIsInstance<ExactView>().single()
                reader.owner = owner
                reader.ownerStarted(true)
                val bytes = "{\"module\":\"hello-compose\",\"slot\":2,\"generation\":3}".toByteArray()
                val words = IntArray((bytes.size + 3) / 4)
                for (i in bytes.indices) words[i / 4] = words[i / 4] or ((bytes[i].toInt() and 255) shl (8 * (i % 4)))
                val ops = intArrayOf(28, 7, 2, 0, 0, 360f.toBits(), 48f.toBits()) + IntArray(8) + intArrayOf(bytes.size) + words
                owner.queue.add(Owner.Pub(1, IntBuffer.wrap(ops), false, 0, 0, null, null, null, null))
                reader.onPublished()
                assertFalse(owner.queue.isEmpty())
            }
            super.setContentView(view)
        }
        fun mount() {
            hosted = content.getChildAt(0) as Natives
            hosted.ensure(7, 2, 360, 48, "{\"module\":\"hello-compose\",\"slot\":2,\"generation\":3}", 0)
            hosted.everywhere()
        }
    }

    class Model : ViewModel() {
        var cleared = false
        override fun onCleared() { cleared = true }
    }

    private val controller = Robolectric.buildActivity(Reader::class.java, Intent()
        .putExtra("owner", "0").putExtra("asyncboot", "0").putExtra("splashfade", "1").putExtra("warmlaunch", "0"))
    private val activity get() = controller.get()
    private fun Reader.state() = content.findViewTreeLifecycleOwner()!!.lifecycle.currentState
    private var destroyed = false
    private fun settle() { compose.waitForIdle() }

    @Before fun start() {
        Exact.owner = null; Exact.reader = null; Exact.live = true; Exact.unshown = false
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 1, 1)
        AuthTest.Environment.values.clear()
        controller.create().start().resume().visible()
    }

    @After fun finish() {
        Reader.publication?.let { owner ->
            val thread = ReflectionHelpers.getField<HandlerThread>(owner, "thread")
            thread.quitSafely(); thread.join(5000)
            assertFalse(thread.isAlive)
        }
        Reader.publication = null
        if (!destroyed) controller.pause().stop().destroy()
        settle()
        Exact.owner = null; Exact.reader = null; Exact.live = false; Exact.unshown = false
    }

    @Test fun noModuleOrHatchViewLeavesTheRootWithoutAndroidXOwners() {
        val root = activity.content
        assertEquals(android.app.Activity::class.java, ExactActivity::class.java.superclass)
        assertNull(root.findViewTreeLifecycleOwner())
        assertNull(root.findViewTreeSavedStateRegistryOwner())
        assertNull(root.findViewTreeViewModelStoreOwner())
        assertNull(root.findViewTreeOnBackPressedDispatcherOwner())
        controller.pause().stop().start().resume()
        assertNull(root.findViewTreeLifecycleOwner())
    }

    @Test fun aHatchCanComposeBeforeTheFirstModuleView() {
        val root = activity.content
        val hosted = root.getChildAt(0) as Natives
        var composed = false
        hosted.hatched = { _, context -> ComposeView(context).apply {
            setContent { BasicText("dial"); SideEffect { composed = true } }
        } }
        hosted.ensure(6, 5, 48, 48, "{}", 0)
        settle()
        assertTrue(composed)
        val owner = root.findViewTreeLifecycleOwner()
        assertNotNull(owner)
        assertSame(owner, root.findViewTreeSavedStateRegistryOwner())
        assertSame(owner, root.findViewTreeViewModelStoreOwner())
        assertSame(owner, root.findViewTreeOnBackPressedDispatcherOwner())
        activity.mount()
        settle()
        val view = activity.module.view
        assertSame(owner, view.findViewTreeLifecycleOwner())
        assertSame(owner, view.findViewTreeSavedStateRegistryOwner())
        assertSame(owner, view.findViewTreeViewModelStoreOwner())
    }

    @Suppress("DEPRECATION")
    @Test fun systemBackClosesAnExactSheetThenVisitsBothComposeLevelsBeforePopping() {
        val requests = ReflectionHelpers.getField<Requests>(activity, "requests")
        requests.perform(intArrayOf(5, 0, 1, 1))
        activity.mount()
        val level = mutableStateOf(2)
        activity.module.view.setContent {
            BackHandler(enabled = level.value > 0) { level.value-- }
            BasicText("level ${level.value}")
        }
        settle()
        val turns = HeadlessLaunchTest.Host.turns
        turns.clear()
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 2, 42, 2, 42)
        activity.onBackPressed()
        assertEquals(listOf("back 2 42"), turns.filter { it.startsWith("back ") })
        assertEquals(2, level.value)
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 1, 1)
        for (expected in listOf(1, 0)) {
            activity.onBackPressed()
            settle()
            assertEquals(expected, level.value)
        }
        activity.onBackPressed()
        assertEquals(listOf("back 2 42", "back 1 1"), turns.filter { it.startsWith("back ") })
        HeadlessLaunchTest.Host.backTargets = intArrayOf(0, 0, 0)
        assertFalse(activity.module.view.findViewTreeOnBackPressedDispatcherOwner()!!.onBackPressedDispatcher.hasEnabledCallbacks())
    }

    @Suppress("DEPRECATION")
    @Test fun aSheetAndADirectHandlerFollowAndroidXRegistrationOrderAcrossStop() {
        activity.mount()
        val owners = activity.content.findViewTreeOnBackPressedDispatcherOwner()!!
        val turns = HeadlessLaunchTest.Host.turns
        turns.clear()
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 2, 42, 2, 42)
        val direct = object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() { turns += "back direct" }
        }
        owners.onBackPressedDispatcher.addCallback(direct)
        activity.onBackPressed()
        controller.pause().stop().start().resume()
        activity.onBackPressed()
        assertEquals(listOf("back direct", "back direct"), turns.filter { it.startsWith("back ") })
        direct.remove()
        activity.onBackPressed()
        assertEquals("back 2 42", turns.last { it.startsWith("back ") })
    }

    @Suppress("DEPRECATION")
    @Test fun aMenuKeepsBackWhenAnExactSheetOpensBeneathIt() {
        activity.mount()
        val turns = HeadlessLaunchTest.Host.turns
        turns.clear()
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 4, 30, 4, 30)
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 4, 30, 2, 42, 4, 30)
        activity.onBackPressed()
        assertEquals(listOf("back 4 30"), turns.filter { it.startsWith("back ") })
    }

    @Suppress("DEPRECATION")
    @Test fun aClosedSelectOnTheFirstRouteHandsItsPendingBackToTheHostedHandler() {
        activity.mount()
        var received = 0
        activity.module.view.setContent { BackHandler { received++ } }
        settle()
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 4, 30, 4, 30)
        HeadlessLaunchTest.Host.backTargets = intArrayOf(0, 0, 0)
        activity.onBackPressed()
        assertEquals(1, received)
        assertFalse(activity.isFinishing)
    }

    @Config(sdk = [33, 34])
    @Test fun theWindowRegistrationFollowsEnabledHandlersAndForwardsPredictiveEvents() {
        var registered: android.window.OnBackInvokedCallback? = null
        activity.platform = object : android.window.OnBackInvokedDispatcher {
            override fun registerOnBackInvokedCallback(priority: Int, callback: android.window.OnBackInvokedCallback) {
                assertEquals(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT, priority)
                registered = callback
            }
            override fun unregisterOnBackInvokedCallback(callback: android.window.OnBackInvokedCallback) {
                assertSame(registered, callback)
                registered = null
            }
        }
        activity.mount()
        val turns = HeadlessLaunchTest.Host.turns
        turns.clear()
        if (android.os.Build.VERSION.SDK_INT >= 34) {
            val animation = registered as android.window.OnBackAnimationCallback
            val event = android.window.BackEvent(0f, 180f, 0.5f, android.window.BackEvent.EDGE_LEFT)
            HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 2, 42, 2, 42)
            animation.onBackStarted(event)
            animation.onBackProgressed(event)
            animation.onBackCancelled()
            assertEquals(listOf("preview 2 42 0", "preview 2 42 1", "preview 2 42 2"),
                turns.filter { it.startsWith("preview ") })
        }
        registered!!.onBackInvoked()
        val expected = if (android.os.Build.VERSION.SDK_INT >= 34) "back 2 42" else "back 1 1"
        assertTrue(expected in turns)
        HeadlessLaunchTest.Host.backTargets = intArrayOf(0, 0, 0)
        assertNull(registered)
    }

    @Suppress("DEPRECATION")
    @Test fun aPendingSheetShouldCloseBeforeTheMountedHandler() {
        activity.mount()
        var level = 2
        activity.module.view.setContent { BackHandler { level-- } }
        settle()
        val presenter = HandlerThread("sheet request").apply { start() }
        try {
            Exact.backHandoff!!.awaitOwner(android.os.Handler(presenter.looper)) {
                HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 2, 42, 2, 42)
            }
        } finally {
            presenter.quitSafely()
            presenter.join(5000)
            assertFalse(presenter.isAlive)
        }
        // Back arrives before the reader draws the requested sheet.
        activity.onBackPressed()
        assertEquals("the sheet should take Back before the hosted handler", 2, level)
        assertEquals("back 2 42", HeadlessLaunchTest.Host.turns.last { it.startsWith("back ") })
    }

    @Suppress("DEPRECATION")
    @Test fun lateReportsShouldKeepTheNewerHandlerInsideTheSheet() {
        activity.mount()
        val requests = ReflectionHelpers.getField<Requests>(activity, "requests")
        val sheet = intArrayOf(1, 2, 42, 2, 42)
        HeadlessLaunchTest.Host.backTargets = sheet

        var level = 2
        activity.module.view.setContent { BackHandler { level-- } }
        settle()
        // Availability reports queued before the owner subscribed can still reach the activity.
        requests.perform(intArrayOf(5, 0, 1, 0))
        requests.perform(intArrayOf(5, 0, 1, 1))
        activity.onBackPressed()
        assertEquals("the later hosted callback should keep its place", 1, level)
    }

    @Suppress("DEPRECATION")
    @Test fun aPendingWindowShouldKeepAnEnabledCallbackOnAPushedRoute() {
        activity.mount()
        val requests = ReflectionHelpers.getField<Requests>(activity, "requests")
        assertTrue(activity.moduleOwners!!.onBackPressedDispatcher.hasEnabledCallbacks())
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 5, 8, 5, 8)
        requests.perform(intArrayOf(5, 0, 1, 0))
        assertTrue("Back should remain registered until the native window arrives",
            activity.moduleOwners!!.onBackPressedDispatcher.hasEnabledCallbacks())
        activity.onBackPressed()
        assertEquals("back 5 8", HeadlessLaunchTest.Host.turns.last { it.startsWith("back ") })
    }

    @Config(sdk = [33])
    @Test fun closingASheetKeepsANewerPlatformHandlerAheadOfTheRoute() {
        val callbacks = mutableListOf<android.window.OnBackInvokedCallback>()
        val window = object : android.window.OnBackInvokedDispatcher {
            override fun registerOnBackInvokedCallback(priority: Int, callback: android.window.OnBackInvokedCallback) {
                assertEquals(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT, priority)
                callbacks.add(callback)
            }
            override fun unregisterOnBackInvokedCallback(callback: android.window.OnBackInvokedCallback) {
                assertTrue(callbacks.remove(callback))
            }
        }
        activity.platform = window
        val requests = ReflectionHelpers.getField<Requests>(activity, "requests")
        requests.perform(intArrayOf(5, 0, 1, 1))
        var calls = 0
        window.registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT) { calls++ }
        activity.mount()
        callbacks.last().onBackInvoked()
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 2, 42, 2, 42)
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 1, 1)
        callbacks.last().onBackInvoked()
        assertEquals(2, calls)
    }

    @Suppress("DEPRECATION")
    @Test fun aRetiredDialogStillReceivingBackCannotPopItsRoute() {
        activity.mount()
        val surfaces = ReflectionHelpers.getField<Requests>(activity, "requests").surfaces!!
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 5, 8, 5, 8)
        surfaces.told(intArrayOf(8, Surfaces.ALERT, 1, 0, 0, 120, 80, 0, 0, 0), 0)
        val dialog = org.robolectric.shadows.ShadowDialog.getLatestDialog()
        assertTrue(dialog.isShowing)
        HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 1, 1)
        val turns = HeadlessLaunchTest.Host.turns
        turns.clear()
        dialog.onBackPressed()
        assertFalse("Back from the retired window reached the route", "back 1 1" in turns)
        surfaces.told(intArrayOf(8, Surfaces.CLOSED, 0, 0, 0, 0, 0, 0, 0, 0), 0)
        activity.onBackPressed()
        assertTrue("back 1 1" in turns)
    }

    @Suppress("DEPRECATION")
    @Test fun aPressQueuedBehindPresentationReachesTheNewLayer() {
        activity.mount()
        val turns = HeadlessLaunchTest.Host.turns
        turns.clear()
        HeadlessLaunchTest.Host.beforeBack = {
            HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 4, 30, 4, 30)
        }
        activity.onBackPressed()
        assertEquals("back 4 30", turns.last { it.startsWith("back ") })
    }

    @Test fun anInputTurnCanRegisterASheetWhileTheMainThreadWaitsForIt() {
        activity.mount()
        val owner = Owner(false, false, 0) { "0" }
        try {
            owner.running(false)
            owner.sync(publish = false) {}
            assertTrue(owner.answer(2000) {
                HeadlessLaunchTest.Host.backTargets = intArrayOf(1, 2, 42, 2, 42)
            })
        } finally {
            Exact.backHandoff!!.flush()
            val thread = ReflectionHelpers.getField<HandlerThread>(owner, "thread")
            thread.quitSafely()
            thread.join(5000)
            assertFalse(thread.isAlive)
        }
    }

    @Test fun aPlainComposeModuleKeepsItsViewAndCompositionAcrossAStop() {
        activity.mount()
        settle()
        val root = activity.content
        val view = activity.module.view
        val owner = root.findViewTreeLifecycleOwner()
        assertNotSame(activity, owner)
        for (child in listOf(root, view)) {
            assertSame(owner, child.findViewTreeLifecycleOwner())
            assertSame(owner, child.findViewTreeSavedStateRegistryOwner())
            assertSame(owner, child.findViewTreeViewModelStoreOwner())
        }
        val model = ViewModelProvider(root.findViewTreeViewModelStoreOwner()!!)[Model::class.java]
        val remembered = activity.module.remembered
        val originalParent = view.parent
        activity.module.change("temporary")
        settle()
        assertEquals("temporary", activity.module.shown)
        assertEquals(Lifecycle.State.RESUMED, activity.state())
        controller.pause()
        assertEquals(Lifecycle.State.STARTED, activity.state())
        controller.stop()
        assertEquals(Lifecycle.State.CREATED, activity.state())
        controller.start().resume()
        activity.mount()
        settle()
        assertNotSame(originalParent, view.parent)
        assertSame(activity.hosted, view.parent)
        assertSame(view, activity.module.view)
        assertSame(remembered, activity.module.remembered)
        assertFalse(activity.module.disposed)
        assertEquals("temporary", activity.module.shown)
        assertSame(model, ViewModelProvider(root.findViewTreeViewModelStoreOwner()!!)[Model::class.java])
        activity.module.change("back again")
        settle()
        assertEquals("back again", activity.module.shown)
        activity.hosted.destroyAll()
        assertTrue(activity.module.disposed)
    }

    @Test fun aPlainViewKeepsItsFieldAndAnAbsentNodeIsDestroyedOnTheFirstPaintAfterAStop() {
        fun layer() = activity.content.getChildAt(0) as Natives
        fun mount(id: Int, row: Int) {
            layer().ensure(id, 2, 40, 40, "{\"module\":\"plain\"}", row)
        }
        layer().everywhere()
        mount(10, 0)
        mount(11, 3)
        val kept = layer().taker(10, 0, 0, 0) as PlainView
        val gone = layer().taker(11, 0, 0, 0)!!
        kept.value = "temporary"
        controller.pause().stop().start().resume()
        mount(10, 0)
        layer().unrowed(setOf(10))
        assertSame(layer(), kept.parent)
        assertSame(kept.parent, kept.returnedParent)
        assertSame(kept, layer().taker(10, 0, 0, 0))
        assertEquals("temporary", kept.value)
        assertNull(gone.parent)
        assertEquals(listOf(gone), activity.plain.destroyed)
        assertEquals(listOf(ViewLifecycle.CREATE, ViewLifecycle.START, ViewLifecycle.RESUME, ViewLifecycle.PAUSE,
            ViewLifecycle.STOP, ViewLifecycle.DESTROY), activity.plain.events.filter { it.first === gone }.map { it.second })
    }

    @Test fun theReturningRowResumesWithCurrentPropsAndAParkedRowReceivesItsBundle() {
        fun layer() = activity.content.getChildAt(0) as Natives
        fun mount(id: Int, value: String) {
            val props = JSONObject().put("module", "plain").put("props", JSONObject().put("value", value).toString())
            layer().ensure(id, 2, 40, 40, props.toString(), id)
        }
        mount(10, "old")
        mount(11, "keep")
        (layer().taker(11, 0, 0, 0) as PlainView).value = "edited"
        controller.pause().stop().start().resume()
        layer().lazy = true
        mount(10, "new")
        mount(11, "keep")
        layer().near(setOf(10), setOf(10), setOf(10, 11))
        assertEquals(ViewLifecycle.RESUME to "new", activity.plain.states.last { it.first == ViewLifecycle.RESUME })
        assertNull(layer().taker(11, 0, 0, 0))
        layer().near(setOf(11), setOf(10, 11), setOf(10, 11))
        assertEquals(ViewLifecycle.CREATE to "edited", activity.plain.states.last { it.first == ViewLifecycle.CREATE })
    }

    @Test fun aNestedCompositionAndTheFocusedChildStayAttachedToTheWindow() {
        fun mount(): FrameLayout {
            val layer = activity.content.getChildAt(0) as Natives
            layer.ensure(8, 2, 100, 100, "{\"module\":\"nested\"}", 0)
            return layer.taker(8, 0, 0, 0) as FrameLayout
        }
        val module = mount()
        settle()
        val memory = activity.nestedMemory
        val field = module.getChildAt(1)
        assertTrue(field.requestFocus())
        controller.pause().stop().start().resume()
        assertSame(module, mount())
        settle()
        assertSame(memory, activity.nestedMemory)
        assertSame(field, activity.content.findFocus())
    }

    @Test fun recreationRestoresComposeStateAndRetainsTheViewModelUntilTheActivityEnds() {
        activity.mount()
        settle()
        val old = activity
        val model = ViewModelProvider(old.content.findViewTreeViewModelStoreOwner()!!)[Model::class.java]
        old.module.change("kept")
        settle()
        assertEquals("kept", old.module.shown)
        Reader.publication = Owner(false, false, 0) { "0" }
        controller.recreate()
        settle()
        assertEquals(360, activity.module.view.measuredWidth)
        assertEquals(48, activity.module.view.measuredHeight)
        assertNotSame(old, activity)
        assertEquals(Lifecycle.State.DESTROYED, old.state())
        assertTrue(old.module.disposed)
        assertFalse(model.cleared)
        assertSame(model, ViewModelProvider(activity.content.findViewTreeViewModelStoreOwner()!!)[Model::class.java])
        assertEquals("kept", activity.module.shown)
        activity.finish()
        controller.pause().stop().destroy()
        destroyed = true
        assertTrue(model.cleared)
    }

    @Test fun aRecreatedActivityThatNeverMountsAModuleStillClearsItsRetainedStore() {
        activity.mount()
        settle()
        val model = ViewModelProvider(activity.content.findViewTreeViewModelStoreOwner()!!)[Model::class.java]
        controller.recreate()
        assertNull(activity.content.findViewTreeViewModelStoreOwner())
        assertFalse(model.cleared)
        activity.finish()
        controller.pause().stop().destroy()
        destroyed = true
        assertTrue(model.cleared)
    }

    @Test fun closingAMaterialDateOrTimePickerLeavesTheModuleComposed() {
        activity.mount()
        settle()
        val content = activity.findViewById<FrameLayout>(android.R.id.content)
        for ((kind, value) in listOf("date" to "2026-11-24", "time" to "14:05")) {
            val answers = ArrayList<String?>()
            val close = materialPickers!!.show(activity, ExactPicker(kind, value, "", "", ""), {}) { answers += it }
            settle()
            val picker = content.getChildAt(1) as ComposeView
            val owner = picker.findViewTreeLifecycleOwner()!!
            assertNotSame(activity, owner)
            assertEquals(Lifecycle.State.RESUMED, owner.lifecycle.currentState)
            assertTrue(picker.hasComposition)
            close()
            close()
            settle()
            assertEquals(listOf<String?>(null), answers)
            assertEquals(Lifecycle.State.DESTROYED, owner.lifecycle.currentState)
            assertFalse(picker.hasComposition)
            assertEquals(1, content.childCount)
            assertFalse(activity.module.disposed)
            assertEquals("hello", activity.module.shown)
        }
    }
}
