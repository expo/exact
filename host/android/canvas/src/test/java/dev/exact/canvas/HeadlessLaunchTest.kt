package dev.exact.canvas

import android.app.Activity
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Rect
import android.net.Uri
import android.os.Bundle
import android.os.HandlerThread
import android.os.Looper
import android.view.View
import android.widget.FrameLayout
import androidx.window.WindowSdkExtensions
import androidx.window.WindowSdkExtensionsDecorator
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowInfoTracker
import androidx.window.layout.WindowInfoTrackerDecorator
import androidx.window.layout.WindowLayoutInfo
import kotlinx.coroutines.flow.MutableSharedFlow
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
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
import org.robolectric.shadows.ShadowChoreographer
import org.robolectric.util.ReflectionHelpers
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [HeadlessLaunchTest.Host::class, HeadlessLaunchTest.Environment::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HeadlessLaunchTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            /** What the host was told of its boot, its locations and its frames, in order. */
            val told = CopyOnWriteArrayList<String>()
            val windowFolds = CopyOnWriteArrayList<List<Int>>()
            var windowGeometry = emptyList<Int>()
            val environment = HashMap<String, String>()
            @JvmStatic @Implementation fun environment(bytes: ByteArray) {
                String(bytes, Charsets.UTF_8).split('\u0000').filter { '=' in it }.forEach {
                    environment[it.substringBefore('=')] = it.substringAfter('=')
                }
                turns += "environment"
            }
            /** The host's boot, what it was told of the page, its turns with no reader and its data module's
             * activation, in the order its thread made them. */
            val turns = CopyOnWriteArrayList<String>()
            var pageHidden = false
            var bootResult = true
            var bootEntered: CountDownLatch? = null
            var releaseBoot: CountDownLatch? = null
            var paint: ByteBuffer? = null
            var chrome: IntArray? = null

            fun rectangle(color: Int, top: Float = 8f): ByteBuffer {
                val words = intArrayOf(2, color, 8f.toBits(), top.toBits(), 16f.toBits(), 8f.toBits()) + IntArray(8)
                return ByteBuffer.allocate(words.size * 4).order(ByteOrder.LITTLE_ENDIAN)
                    .apply { asIntBuffer().put(words) }
            }

            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun start(width: Int, height: Int, scale: Float, dir: String, store: String, live: Boolean): Boolean {
                told += "start"
                turns += "start"
                bootEntered?.countDown()
                releaseBoot?.let { check(it.await(5, TimeUnit.SECONDS)) { "boot was not released" } }
                return bootResult
            }
            @JvmStatic @Implementation fun hidden(hidden: Boolean, eligible: Boolean) {
                pageHidden = hidden
                turns += "hidden $hidden $eligible"
            }
            @JvmStatic @Implementation fun agent(line: ByteArray): ByteArray {
                val prefer = JSONObject(String(line, Charsets.UTF_8))
                pageHidden = prefer.getJSONObject("page").getString("visibility-state") == "hidden"
                return "{}".toByteArray()
            }
            @JvmStatic @Implementation fun launch(href: String) { told += "launch $href" }
            @JvmStatic @Implementation fun launchShare(request: String) { told += "launchShare $request" }
            @JvmStatic @Implementation fun open(href: String) { told += "open $href" }
            @JvmStatic @Implementation fun share(request: String) { told += "share $request" }
            @JvmStatic @Implementation fun window(width: Int, height: Int, scale: Float, insets: IntArray) {
                windowFolds += insets.drop(16)
                windowGeometry = listOf(width, height, insets[0])
                if (insets[0] == 16) {
                    // The software canvas uses the stream's coordinates inside the root's clip.
                    paint = rectangle(Color.BLUE, top = 24f)
                    chrome = intArrayOf(0, 16, width, height - 16, Color.TRANSPARENT, 0)
                }
            }
            @JvmStatic @Implementation fun frame(vsync: Boolean): ByteBuffer? {
                told += "frame"
                return paint.also { paint = null }
            }
            @JvmStatic @Implementation fun pendingPictures() = IntArray(0)
            @JvmStatic @Implementation fun chrome(): IntArray? = chrome.also { chrome = null }
            @JvmStatic @Implementation fun drew(): Boolean { turns += "drew"; return false }
            @JvmStatic @Implementation fun dataActive(unshown: Boolean): Int { turns += "dataActive $unshown"; return 1 }
            @JvmStatic @Implementation fun fds() = IntArray(0)
            @JvmStatic @Implementation fun background(applying: Boolean): Double { turns += "background"; return -1.0 }
            @JvmStatic @Implementation fun nextDue() = -1.0
        }
    }

    @Implements(android.system.Os::class)
    class Environment {
        companion object {
            val values = HashMap<String, String?>()
            @JvmStatic @Implementation fun environ(): Array<String> = values.map { "${it.key}=${it.value}" }.toTypedArray()
            @JvmStatic @Implementation fun setenv(name: String, value: String?, overwrite: Boolean) {
                if (overwrite || name !in values) values[name] = value
            }
        }
    }

    // Robolectric's manifest-free Activity has no display context until its window is attached.
    class Reader : ExactActivity() {
        override fun getDisplay(): android.view.Display =
            getSystemService(android.hardware.display.DisplayManager::class.java).getDisplay(0)
    }

    private val main = shadowOf(Looper.getMainLooper())
    private val made = ArrayList<org.robolectric.android.controller.ActivityController<Reader>>()

    @Before fun setup() {
        Host.told.clear(); Host.turns.clear(); Host.windowFolds.clear(); Host.environment.clear(); Environment.values.clear()
        Host.pageHidden = false; Host.bootResult = true; Host.bootEntered = null; Host.releaseBoot = null
        Host.paint = null; Host.chrome = null
        Exact.owner = null; Exact.reader = null; Exact.live = false; Exact.unshown = false
        ReflectionHelpers.setStaticField(Exact::class.java, "pageHidden", null)
    }

    @After fun finish() {
        made.forEach { it.pause().stop().destroy() }
        Exact.owner?.let { owner ->
            val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }.get(owner) as HandlerThread
            thread.quitSafely(); thread.join(5000)
        }
        Exact.owner = null; Exact.reader = null; Exact.live = false; Exact.unshown = false
        ReflectionHelpers.setStaticField(Exact::class.java, "pageHidden", null)
    }

    /** The host's thread has done what it was asked so far, and the main thread what that posted. */
    private fun settle() = repeat(3) { main.idle(); Exact.owner?.ask { }; main.idle() }

    /** A process a platform callback started: its host boots with no activity, and its session is ready. */
    private fun headless() {
        var ready: Boolean? = null
        Headless.session(RuntimeEnvironment.getApplication()) { ready = it }
        settle()
        assertEquals(true, ready)
        assertEquals(listOf("start"), Host.told)
        assertNull(Environment.values["EXACT_LAUNCH_URL"])
    }

    private fun start(intent: Intent): org.robolectric.android.controller.ActivityController<Reader> {
        // warmlaunch=0: the GPU warm-up draws through Robolectric's graphics, which run the test JVM out of heap.
        val controller = Robolectric.buildActivity(Reader::class.java, intent.putExtra("splashfade", "1").putExtra("warmlaunch", "0"))
        made += controller
        return controller.create().start().resume()
    }

    private fun activity(intent: Intent) {
        start(intent)
        settle()
    }

    private fun visibilityReports() = Host.turns.filter { it.startsWith("hidden ") }

    @Test fun layoutUsesTheRootSizeWhenMetricsStillDescribeAnotherSize() {
        val controller = start(Intent(Intent.ACTION_MAIN))
        val root = ReflectionHelpers.getField<Root>(controller.get(), "root")
        root.layout(0, 0, 357, 113)
        Exact.owner?.ask { }
        assertEquals(listOf(357, 113), Host.windowGeometry.take(2))
    }

    @Test @Config(sdk = [29]) fun theFirstReaderKeepsDispatchedInsetsThroughAnUnattachedLayout() {
        val intent = Intent(Intent.ACTION_MAIN).putExtra("warmlaunch", "0")
        val controller = Robolectric.buildActivity(Reader::class.java, intent).create()
        val root = ReflectionHelpers.getField<Root>(controller.get(), "root")
        try {
            root.dispatchApplyWindowInsets(android.view.WindowInsets.Builder()
                .setSystemWindowInsets(android.graphics.Insets.of(0, 17, 0, 0)).build())
            root.layout(0, 0, 200, 300)
            Exact.owner?.ask { }
            assertEquals(listOf(200, 300, 17), Host.windowGeometry)
        } finally { made += controller.start().resume() }
    }

    @Test fun aStreamOnlyFoldSurvivesPauseAndTracksTheWindowWhileStopped() = resumeFold(5)
    @Test fun resumeReadsTheCurrentFoldEvenWithoutAFlowEmission() = resumeFold(9)

    private fun resumeFold(extension: Int) {
        val empty = WindowLayoutInfo(emptyList())
        var current = empty
        val events = MutableSharedFlow<WindowLayoutInfo>(extraBufferCapacity = 1)
        WindowSdkExtensions.overrideDecorator(object : WindowSdkExtensionsDecorator {
            override fun decorate(windowSdkExtensions: WindowSdkExtensions) = object : WindowSdkExtensions() {
                override val extensionVersion = extension
            }
        })
        WindowInfoTracker.overrideDecorator(object : WindowInfoTrackerDecorator {
            override fun decorate(tracker: WindowInfoTracker) = object : WindowInfoTracker by tracker {
                override fun windowLayoutInfo(activity: Activity) = events
                override fun getCurrentWindowLayoutInfo(context: android.content.Context) = current
            }
        })
        try {
            val app = RuntimeEnvironment.getApplication()
            shadowOf(app.packageManager).setSystemFeature(android.content.pm.PackageManager.FEATURE_SENSOR_HINGE_ANGLE, true)
            val controller = start(Intent(Intent.ACTION_MAIN))
            settle()
            val bounds = controller.get().windowManager.currentWindowMetrics.bounds
            val x = bounds.width() / 2
            val book = WindowLayoutInfo(listOf(object : FoldingFeature {
                override val bounds = Rect(x, 0, x, bounds.height())
                override val orientation = FoldingFeature.Orientation.VERTICAL
                override val state = FoldingFeature.State.HALF_OPENED
                override val isSeparating = true
                override val occlusionType = FoldingFeature.OcclusionType.NONE
            }))
            current = book
            assertTrue(events.tryEmit(book))
            settle()
            assertEquals(listOf(2, 1, x, x), Host.windowFolds.last())
            controller.pause()
            if (extension >= 9) current = empty
            controller.resume()
            settle()
            assertEquals(if (extension >= 9) listOf(1, 0, 0, 0) else listOf(2, 1, x, x), Host.windowFolds.last())
            controller.pause().stop()
            val before = Host.windowFolds.size
            current = if (extension >= 9) book else empty
            if (extension < 9) assertTrue(events.tryEmit(current))
            settle()
            assertEquals(before, Host.windowFolds.size)
            controller.restart().resume()
            settle()
            assertEquals(if (extension >= 9) listOf(2, 1, x, x) else listOf(1, 0, 0, 0), Host.windowFolds.last())
        } finally {
            WindowInfoTracker.reset()
            WindowSdkExtensions.reset()
        }
    }

    @Test fun aColdActivitySendsNoVisibilityNotification() {
        activity(Intent(Intent.ACTION_MAIN))
        assertEquals(emptyList<String>(), visibilityReports())
    }

    @Test fun anActivityMakesAHeadlessHostVisible() {
        headless()
        activity(Intent(Intent.ACTION_MAIN))
        assertEquals(listOf("hidden true false", "hidden false true"), visibilityReports())
    }

    @Test fun aReplacementActivityMakesThePageVisibleAfterAnAgentHidIt() {
        activity(Intent(Intent.ACTION_MAIN))
        val owner = checkNotNull(Exact.owner)
        owner.sync { Native.agent("""{"op":"prefer","page":{"visibility-state":"hidden"}}""".toByteArray()) }
        assertTrue(owner.ask { Host.pageHidden })

        activity(Intent(Intent.ACTION_MAIN))

        assertEquals(listOf("hidden false true"), visibilityReports())
        assertFalse(owner.ask { Host.pageHidden })
    }

    @Test fun aColdCompletionStillBelongsToItsReaderAfterRestart() {
        val controller = start(Intent(Intent.ACTION_MAIN))
        val owner = checkNotNull(Exact.owner)
        fun reader() = ReflectionHelpers.getField<ExactView>(controller.get(), "view")
        fun started(view: ExactView) = ReflectionHelpers.getField<Boolean>(view, "started")
        val cold = reader()
        assertTrue(cold.booting)
        withMainSyncBarrier {
            controller.pause().stop().restart().resume()
            val resumed = reader()
            assertTrue(cold !== resumed)
            assertFalse("restart does not hold drawing", resumed.booting)
            owner.ask { }
            main.idle()
            assertTrue("the asynchronous completion reaches the cold reader", started(cold))
            assertFalse("the restarted reader waits for its own completion", started(resumed))
        }
        assertTrue(started(reader()))
    }

    @Test fun chromeAfterTheFirstDrawStaysInOrderEvenWhenBootSentNone() {
        val controller = start(Intent(Intent.ACTION_MAIN).putExtra("warm", "0"))
        settle()
        val view = ReflectionHelpers.getField<ExactView>(controller.get(), "view")
        val bitmap = Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888)
        try {
            view.layout(0, 0, 64, 64)
            view.draw(Canvas(bitmap))
            main.idle()
            assertTrue("the reader drew", "drew" in Host.turns)
            val owner = checkNotNull(Exact.owner)
            withMainSyncBarrier {
                owner.sync { Host.chrome = intArrayOf(0, 4, 64, 60, Color.TRANSPARENT, 0) }
                main.idle()
                assertEquals("later chrome waits for the traversal", 0,
                    (view.layoutParams as FrameLayout.LayoutParams).topMargin)
            }
            assertEquals(4, (view.layoutParams as FrameLayout.LayoutParams).topMargin)
        } finally {
            Exact.owner?.ask { Host.chrome = null }
            bitmap.recycle()
        }
    }

    @Test fun theWindowWaitsForASuccessfulBoot() = drawsAfterBoot(ok = true)

    @Test fun aBootInsetCorrectionIsAppliedOnTheFirstDraw() = drawsAfterBoot(ok = true, insetCorrection = true)

    @Test @Config(sdk = [29]) fun anInsetLearnedDuringLayoutIsAppliedBeforeTheFirstDraw() =
        drawsAfterBoot(ok = true, insetCorrection = true, insetDuringLayout = true)

    @Test fun theWindowDrawsAfterAFailedBoot() = drawsAfterBoot(ok = false)

    private fun drawsAfterBoot(ok: Boolean, insetCorrection: Boolean = false, insetDuringLayout: Boolean = false) {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        Host.bootEntered = entered
        Host.releaseBoot = release
        Host.bootResult = ok
        if (ok) Host.paint = Host.rectangle(Color.RED)
        if (insetCorrection) Host.chrome = intArrayOf(0, 0, 64, 64, Color.TRANSPARENT, 0)
        val controller = Robolectric.buildActivity(Activity::class.java).setup()
        val bitmap = Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888)
        val wasPaused = ShadowChoreographer.isPaused()
        ShadowChoreographer.setPaused(true)
        try {
            val activity = controller.get()
            activity.intent.putExtra("warm", "0")
            val owner = Owner(false, false, 0) { "0" }.also { Exact.owner = it }
            val view = ExactView(activity, false) {}.apply { this.owner = owner }
            fun tellInset() = owner.window(64, 64, 1f, IntArray(16).apply { this[0] = 16 })
            var reportInset = false
            val root = object : FrameLayout(activity) {
                override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
                    super.onLayout(changed, left, top, right, bottom)
                    if (reportInset) { reportInset = false; tellInset() }
                }
            }
            root.addView(view, FrameLayout.LayoutParams(64, 64))
            owner.chrome = { c ->
                view.layoutParams = FrameLayout.LayoutParams(c[2], c[3]).apply { topMargin = c[1] }
            }
            owner.window(64, 64, 1f, IntArray(16))
            view.booting = owner.boot(64, 64, 1f, "", "", false, done = view::ownerStarted)
            assertTrue("native startup entered", entered.await(5, TimeUnit.SECONDS))
            activity.setContentView(root)
            assertTrue(view.isAttachedToWindow)
            fun traverse(): Boolean {
                val size = View.MeasureSpec.makeMeasureSpec(64, View.MeasureSpec.EXACTLY)
                root.measure(size, size)
                root.layout(0, 0, 64, 64)
                if (root.viewTreeObserver.dispatchOnPreDraw()) return false
                root.draw(Canvas(bitmap))
                return true
            }
            assertFalse("the window waits for the owner", traverse())

            withMainSyncBarrier {
                if (insetCorrection && !insetDuringLayout) tellInset()
                release.countDown()
                owner.ask { }
                main.idle()
                if (insetDuringLayout) {
                    assertFalse("boot completed before layout", view.booting)
                    reportInset = true
                    assertFalse("layout learned a new inset, so the window waits for its chrome", traverse())
                    assertFalse("layout told the owner about the inset", reportInset)
                    owner.ask { }
                    main.idle()
                }
                assertTrue("boot completion lets the window draw through the barrier", traverse())
                assertEquals("the first frame uses the current page margin", if (insetCorrection) 16 else 0, view.top)
                val color = if (insetCorrection) Color.BLUE else if (ok) Color.RED else Color.TRANSPARENT
                assertEquals("the first frame replays the owner's paint", color, bitmap.getPixel(12, view.top + 12))
                assertEquals(Color.TRANSPARENT, bitmap.getPixel(12, view.top + 20))
            }
            assertEquals("the host draws content only after a successful boot", ok, "drew" in Host.turns)
        } finally {
            release.countDown()
            Exact.owner?.ask { Host.paint = null; Host.chrome = null }
            controller.pause().stop().destroy()
            ShadowChoreographer.setPaused(wasPaused)
            bitmap.recycle()
        }
    }

    /** What the host was told once the activity's reader adopted it ([Native.start] again), up to its first frame. */
    private fun beforeTheFirstFrame(): List<String> {
        assertEquals(listOf("start", "start"), Host.told.take(2))
        return Host.told.drop(2).takeWhile { it != "frame" }
    }

    @Test fun theNativeEnvironmentIsTransferredBeforeAHeadlessBoot() {
        headless()
        assertEquals(Environment.values["XDG_CACHE_HOME"], Host.environment["XDG_CACHE_HOME"])
        assertEquals(Environment.values["TMPDIR"], Host.environment["TMPDIR"])
        assertTrue(Host.turns.indexOf("environment") in 0 until Host.turns.indexOf("start"))
    }

    @Test fun aLoopSelectionBelongsToOneInstallAndDrivesCanOverrideIt() {
        val app = RuntimeEnvironment.getApplication()
        val saved = "http://127.0.0.1:8765/"
        val other = "http://127.0.0.1:9123/"
        val select = Intent().setClassName(app, "dev.exact.app.DevLoop").putExtra("x_dev_plan", saved)
        assertEquals(saved, DevLoop.atLaunch(app, select))
        assertEquals(saved, DevLoop.atLaunch(app, null))
        assertEquals(saved, DevLoop.atLaunch(app, Intent(Intent.ACTION_MAIN)))
        assertEquals(other, DevLoop.atLaunch(app, Intent().putExtra("x_dev_plan", other)))
        assertNull(DevLoop.atLaunch(app, Intent().putExtra("x_agent", "1")))
        assertNull(DevLoop.atLaunch(app, Intent().putExtra("x_plan", "/fixture")))
        assertEquals(saved, DevLoop.atLaunch(app, Intent().setClassName(app, "dev.exact.app.Notify")))
        val prefs = app.getSharedPreferences("exact.devLoop", android.content.Context.MODE_PRIVATE)
        prefs.edit().putLong("installed", -2).commit()
        assertNull(DevLoop.atLaunch(app, Intent(Intent.ACTION_MAIN)))
    }

    private fun development(on: Boolean) {
        val app = RuntimeEnvironment.getApplication()
        // With Android resources, ApplicationInfo is a copy; Setup reads the package manager's manifest metadata.
        val info = shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).applicationInfo!!
        info.metaData = Bundle().apply { putBoolean("dev.exact.development", on) }
    }

    @Test fun aDriverLaunchTransfersItsEnvironmentBeforeBoot() {
        development(true)
        val stored = """[["pace","\"2:15\""]]"""
        activity(Intent(Intent.ACTION_MAIN).putExtra("x_agent", "1")
            .putExtra("x_agent_storage", "a=b é").putExtra("x_agent_stored", stored)
            .putExtra("x_agent_epoch", "1234"))
        assertEquals("1", Host.environment["EXACT_AGENT"])
        assertEquals("a=b é", Host.environment["EXACT_AGENT_STORAGE"])
        assertEquals(stored, Host.environment["EXACT_AGENT_STORED"])
        assertEquals("1234", Host.environment["EXACT_AGENT_EPOCH"])
        assertTrue(Host.turns.indexOf("environment") in 0 until Host.turns.indexOf("start"))
    }

    @Test fun aProductionLaunchDoesNotTransferDriverExtras() {
        development(false)
        activity(Intent(Intent.ACTION_MAIN).putExtra("x_agent", "1").putExtra("x_agent_storage", "private"))
        assertFalse(Host.environment.containsKey("EXACT_AGENT"))
        assertFalse(Host.environment.containsKey("EXACT_AGENT_STORAGE"))
        assertEquals(Environment.values["XDG_CACHE_HOME"], Host.environment["XDG_CACHE_HOME"])
    }

    @Test fun aHostBootedWithNoActivityIsHiddenBeforeItsFirstTurnAndItsDataModule() {
        headless()
        // What the thread asked before its host existed, no host heard.
        val booted = Host.turns.dropWhile { it != "start" }
        // A resource that reads the page as the module activates, or a task of the first turn, finds it hidden and
        // not eligible to execute.
        assertEquals(listOf("start", "hidden true false"), booted.take(2))
        assertEquals(setOf("background", "dataActive true"), booted.drop(2).toSet())
    }

    @Test fun theFirstActivitysUrlIsTheLaunchLocation() {
        headless()
        val url = "https://app.test/item/42"
        activity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        assertEquals("the URL is the launch location before a reader takes a frame", listOf("launch $url"), beforeTheFirstFrame())
        assertFalse(Host.told.any { it.startsWith("open ") })
        assertNull("the environment is read once, at the boot", Environment.values["EXACT_LAUNCH_URL"])
        // An activity made again is given its URL as the running app's.
        val next = "https://app.test/item/43"
        activity(Intent(Intent.ACTION_VIEW, Uri.parse(next)))
        assertEquals(listOf("launch $url", "open $next"), Host.told.filter { it.startsWith("launch") || it.startsWith("open") })
    }

    private fun push(vararg data: Pair<String, String>) = Intent(Intent.ACTION_MAIN).apply {
        putExtra("google.message_id", "expo-42")
        data.forEach { (key, value) -> putExtra(key, value) }
    }

    @Test fun anExpoPushTapSetsTheColdLaunchLocation() {
        activity(push("experienceId" to "@owner/app", "body" to """{"navigate":"/urgencies"}"""))
        assertEquals("/urgencies", Host.environment["EXACT_LAUNCH_URL"])
        assertFalse(Host.told.any { it.startsWith("open ") })
    }

    @Test fun anExpoPushTapLaunchesOverAHeadlessHost() {
        headless()
        activity(push("scopeKey" to "@owner/app", "body" to """{"navigate":"/urgencies"}"""))
        assertEquals(listOf("launch /urgencies"), beforeTheFirstFrame())
    }

    @Test fun aWarmPushTapReadsOnlyTheSendersNavigate() {
        activity(Intent(Intent.ACTION_MAIN))
        val body = """{"id":"alert-42","navigate":"/urgencies"}"""
        val cases = listOf(
            push("experienceId" to "@owner/app", "body" to body) to "/urgencies",
            push("scopeKey" to "@owner/app", "body" to body) to "/urgencies",
            push("navigate" to "/top", "scopeKey" to "@owner/app", "body" to body) to "/top",
            push("navigate" to "/direct") to "/direct",
            push("body" to body) to null,
            push("projectId" to "project", "body" to body) to null,
            push("scopeKey" to "@owner/app") to null,
            push("navigate" to "", "scopeKey" to "@owner/app", "body" to body) to null,
            push("navigate" to " ", "scopeKey" to "@owner/app", "body" to body) to null,
            push("scopeKey" to "@owner/app", "body" to body).also { it.removeExtra("google.message_id") } to null,
        ) + listOf("not JSON", "[]", "{}", """{"navigate":42}""", """{"navigate":null}""",
            """{"navigate":""}""", """{"navigate":"  "}""").map {
            push("experienceId" to "@owner/app", "body" to it) to null
        }
        for ((intent, location) in cases) {
            Host.told.clear()
            made.single().newIntent(intent)
            settle()
            assertEquals(intent.extras.toString(), listOfNotNull(location?.let { "open $it" }),
                Host.told.filter { it.startsWith("open ") })
        }
    }

    @Test fun theFirstActivitysShareIsTheLaunchLocation() {
        headless()
        val target = "{\"action\":\"/share\",\"params\":{\"text\":\"text\"}}"
        val app = RuntimeEnvironment.getApplication()
        shadowOf(app.packageManager).addOrUpdateActivity(android.content.pm.ActivityInfo().apply {
            packageName = app.packageName; name = Reader::class.java.name
            metaData = Bundle().apply { putString("dev.exact.share_target", target) }
        })
        activity(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, "a note"))
        val given = beforeTheFirstFrame().single()
        assertTrue(given, given.startsWith("launchShare "))
        val request = JSONObject(given.removePrefix("launchShare "))
        assertEquals("/share", request.getJSONObject("target").getString("action"))
        assertEquals("a note", request.getJSONObject("share").getString("text"))
        assertFalse(Host.told.any { it.startsWith("share ") })
    }

    @Test fun aLauncherStartOverAHeadlessHostGivesNoLocation() {
        headless()
        activity(Intent(Intent.ACTION_MAIN))
        assertTrue(beforeTheFirstFrame().isEmpty())
        assertTrue(Host.told.none { it.startsWith("launch") || it.startsWith("open") || it.startsWith("share") })
    }
}
