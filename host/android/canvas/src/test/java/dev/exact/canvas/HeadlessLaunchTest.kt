package dev.exact.canvas

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.HandlerThread
import android.os.Looper
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
import java.nio.ByteBuffer
import java.util.concurrent.CopyOnWriteArrayList

/** The first activity made over a host [Headless] booted (LLP 1133.017 D7): its URL or share is the launch location. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [HeadlessLaunchTest.Host::class, HeadlessLaunchTest.Environment::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HeadlessLaunchTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            /** What the host was told of its boot, its locations and its frames, in order. */
            val told = CopyOnWriteArrayList<String>()
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
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun start(width: Int, height: Int, scale: Float, dir: String, store: String, live: Boolean): Boolean {
                told += "start"
                turns += "start"
                return true
            }
            @JvmStatic @Implementation fun hidden(hidden: Boolean, eligible: Boolean) { turns += "hidden $hidden $eligible" }
            @JvmStatic @Implementation fun launch(href: String) { told += "launch $href" }
            @JvmStatic @Implementation fun launchShare(request: String) { told += "launchShare $request" }
            @JvmStatic @Implementation fun open(href: String) { told += "open $href" }
            @JvmStatic @Implementation fun share(request: String) { told += "share $request" }
            @JvmStatic @Implementation fun frame(vsync: Boolean): ByteBuffer? { told += "frame"; return null }
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
        Host.told.clear(); Host.turns.clear(); Host.environment.clear(); Environment.values.clear()
        Exact.owner = null; Exact.reader = null; Exact.live = false; Exact.unshown = false
    }

    @After fun finish() {
        made.forEach { it.pause().stop().destroy() }
        Exact.owner?.let { owner ->
            val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }.get(owner) as HandlerThread
            thread.quitSafely(); thread.join(5000)
        }
        Exact.owner = null; Exact.reader = null; Exact.live = false; Exact.unshown = false
        Exact.hidden(false)
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

    private fun activity(intent: Intent): Reader {
        val controller = Robolectric.buildActivity(Reader::class.java, intent.putExtra("splashfade", "1"))
        made += controller
        controller.create().start().resume()
        settle()
        return controller.get()
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

    @Test fun aDriverLaunchTransfersItsEnvironmentBeforeBoot() {
        val app = RuntimeEnvironment.getApplication()
        app.applicationInfo.metaData = Bundle().apply { putBoolean("dev.exact.development", true) }
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
        val app = RuntimeEnvironment.getApplication()
        app.applicationInfo.metaData = Bundle().apply { putBoolean("dev.exact.development", false) }
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
