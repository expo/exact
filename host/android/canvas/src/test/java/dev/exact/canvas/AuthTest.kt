package dev.exact.canvas

import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ActivityInfo
import android.content.pm.ApplicationInfo
import android.content.pm.ResolveInfo
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Bundle
import android.os.Looper
import android.os.SystemClock
import org.junit.After
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
import org.robolectric.shadows.ShadowSystemClock
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [AuthTest.Host::class, AuthTest.Environment::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AuthTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            val urls = ArrayList<String>()
            val answers = ArrayList<Pair<Int, String?>>()
            val opened = ArrayList<Int>()
            val pending = java.util.concurrent.atomic.AtomicReference<IntArray?>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun open(url: String) { urls += url }
            @JvmStatic @Implementation fun answer(id: Int, answer: String?) {
                if (answer == "opened") opened += id else answers += id to answer
            }
            @JvmStatic @Implementation fun requests(): IntArray? = pending.getAndSet(null)
        }
    }

    @Implements(android.system.Os::class)
    class Environment {
        companion object {
            val values = HashMap<String, String?>()
            @JvmStatic @Implementation fun setenv(name: String, value: String?, overwrite: Boolean) {
                if (overwrite || name !in values) values[name] = value
            }
        }
    }

    // Robolectric's manifest-free Activity has no display context until its window is attached.
    class Reader : ExactActivity() {
        val attempts = ArrayList<String?>()
        val missing = HashSet<String>()
        override fun startActivity(intent: Intent, options: Bundle?) {
            attempts += intent.`package`
            if (intent.`package` in missing) throw ActivityNotFoundException()
            super.startActivity(intent, options)
        }
        override fun getDisplay(): android.view.Display =
            getSystemService(android.hardware.display.DisplayManager::class.java).getDisplay(0)
    }

    private fun intent(url: String? = null) = Intent(Intent.ACTION_VIEW, url?.let(Uri::parse))
        .putExtra("owner", "0").putExtra("asyncboot", "0").putExtra("splashfade", "1")
    private var controller = Robolectric.buildActivity(Reader::class.java, intent())
    private val activity get() = controller.get()

    @Before fun setup() {
        Host.urls.clear(); Host.answers.clear(); Host.opened.clear(); Host.pending.set(null); Environment.values.clear()
        Exact.owner = null; Exact.reader = null; Exact.live = true
        callbacks("[\"app.test:/cb\"]")
        controller.create().start().resume()
    }

    private fun callbacks(json: String) {
        val info = activity.packageManager.getApplicationInfo(activity.packageName, 0)
        info.metaData = Bundle().apply { putString("dev.exact.auth.callbacks", json) }
        shadowOf(activity.packageManager).addPackage(android.content.pm.PackageInfo().apply {
            packageName = activity.packageName; applicationInfo = info
        })
    }

    @After fun finish() {
        (1..4).forEach { Auth.end(it) }
        controller.pause().stop().destroy()
        Exact.live = false
    }

    private fun browser(tabs: Boolean, pkg: String = "test.browser", priority: Int = 0, allWeb: Boolean = true, separateFilters: Boolean = false) {
        val manager = shadowOf(activity.packageManager)
        val info = ResolveInfo().apply {
            this.priority = priority
            isDefault = true
            filter = IntentFilter(Intent.ACTION_VIEW).apply {
                addCategory(Intent.CATEGORY_BROWSABLE)
                addCategory(Intent.CATEGORY_DEFAULT)
                addDataScheme("https")
                if (allWeb && !separateFilters) addDataScheme("http")
            }
            activityInfo = ActivityInfo().apply {
                packageName = pkg; name = "Browser"
                applicationInfo = ApplicationInfo().apply { packageName = pkg }
            }
        }
        manager.addResolveInfoForIntent(Intent(Intent.ACTION_VIEW, Uri.parse("https://")), info)
        if (allWeb) manager.addResolveInfoForIntent(Intent(Intent.ACTION_VIEW, Uri.parse("http://")), ResolveInfo(info).apply {
            if (separateFilters) filter = IntentFilter(Intent.ACTION_VIEW).apply {
                addCategory(Intent.CATEGORY_BROWSABLE)
                addCategory(Intent.CATEGORY_DEFAULT)
                addDataScheme("http")
            }
        })
        if (tabs) manager.addResolveInfoForIntent(Intent(SystemBrowser.SERVICE).setPackage(pkg), ResolveInfo().apply {
            serviceInfo = ServiceInfo().apply { packageName = pkg; name = "Tabs" }
        })
    }

    @Test fun separateHttpAndHttpsFiltersQualifyTheDefaultBrowser() {
        browser(false, "test.default", priority = 10, separateFilters = true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        assertTrue(Host.answers.isEmpty())
        assertEquals("test.default", shadowOf(activity).nextStartedActivity.`package`)
        browser(true, "test.other")
        Auth.open(activity, 2, "https://login.test/authorize", "app.test:/cb", "s1")
        assertEquals("test.default", shadowOf(activity).nextStartedActivity.`package`)
    }

    @Test fun aPlainDefaultBrowserWinsOverAnotherBrowsersCustomTabs() {
        browser(true, "test.tabs")
        browser(false, "test.default", priority = 10)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        val opened = shadowOf(activity).nextStartedActivity
        assertEquals("test.default", opened.`package`)
        assertFalse(opened.hasExtra(SystemBrowser.SESSION))
    }

    @Test fun noDefaultAsksThePersonToChooseBetweenBrowsers() {
        shadowOf(activity.packageManager).setShouldShowActivityChooser(true)
        browser(true, "test.tabs")
        browser(false, "test.plain")
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        val opened = shadowOf(activity).nextStartedActivity
        assertEquals(Intent.ACTION_CHOOSER, opened.action)
        val first = opened.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)!!
        val rest = opened.getParcelableArrayExtra(Intent.EXTRA_INITIAL_INTENTS)!!.map { it as Intent }
        val choices = listOf(first) + rest
        assertEquals(setOf("test.tabs", "test.plain"), choices.map { it.`package` }.toSet())
        assertTrue(choices.all { it.dataString == "https://login.test/authorize" })
        assertTrue(choices.single { it.`package` == "test.tabs" }.hasExtra(SystemBrowser.SESSION))
        assertFalse(choices.single { it.`package` == "test.plain" }.hasExtra(SystemBrowser.SESSION))
    }

    @Test fun aCustomTabsServiceDoesNotMakeALinkHandlerABrowser() {
        browser(true, "test.link", priority = 10, allWeb = false)
        browser(false, "test.browser")
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        assertEquals("test.browser", shadowOf(activity).nextStartedActivity.`package`)
    }

    @Test fun aMissingDefaultTriesTheOtherBrowserBeforeReportingUnavailability() {
        browser(true, "test.default", priority = 10)
        browser(false, "test.other")
        activity.missing += "test.default"
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        assertEquals(listOf("test.default", "test.other"), activity.attempts)
        assertEquals("test.other", shadowOf(activity).nextStartedActivity.`package`)
        assertTrue(Host.answers.isEmpty())
    }

    @Test fun customTabReturnsThroughTheActivityIntentAndDoesNotBecomeItsSavedLocation() {
        browser(true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        val opened = shadowOf(activity).nextStartedActivity
        assertEquals("test.browser", opened.`package`)
        assertTrue(opened.hasExtra(SystemBrowser.SESSION))
        assertEquals("https://login.test/authorize", opened.dataString)
        val url = "app.test:/cb?code=secret&state=s1"
        controller.newIntent(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        assertEquals(listOf(url), Host.urls)
        assertNull(activity.intent.data)
        assertTrue(Host.answers.isEmpty())
        Auth.end(1)
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(11))
        assertTrue(Host.answers.isEmpty())
    }

    private fun authRequest(id: Int, state: String = "s1"): IntArray {
        val body = ArrayList<Int>()
        for (text in listOf("https://login.test/authorize", "app.test:/cb", state)) {
            val bytes = text.toByteArray(Charsets.UTF_8)
            body += bytes.size
            bytes.toList().chunked(4).forEach { chunk ->
                body += chunk.foldIndexed(0) { i, word, byte -> word or ((byte.toInt() and 255) shl (i * 8)) }
            }
        }
        return intArrayOf(17, id, body.size) + body.toIntArray()
    }

    @Test fun filesDeliveredByTheOwnerAfterDestructionAreCancelled() {
        val owner = Owner(false, false, 0) { "0" }
        val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }
            .get(owner) as android.os.HandlerThread
        Exact.owner = owner
        owner.running(false)
        owner.asked = { Requests(activity).perform(it) }
        try {
            controller.pause().stop().destroy()
            assertNull(Exact.reader)
            owner.call { Host.pending.set(intArrayOf(18, 377, 3, 42, 0, 0)) }
            owner.ask { }
            shadowOf(Looper.getMainLooper()).idle()
            owner.ask { }
            assertEquals(listOf(377 to "file picker: activity unavailable"), Host.answers)
            assertNull(shadowOf(activity).nextStartedActivityForResult)
        } finally {
            Exact.owner = null
            thread.quitSafely(); thread.join(5000)
            assertFalse(thread.isAlive)
            controller = Robolectric.buildActivity(Reader::class.java, intent())
            controller.create().start().resume()
        }
    }

    @Test fun authDeliveredByTheOwnerAfterDestructionIsCancelled() {
        browser(true)
        Auth.open(activity, 2, "https://login.test/authorize", "app.test:/cb", "s1")
        shadowOf(activity).nextStartedActivity
        assertEquals(listOf(2), Host.opened)
        Host.opened.clear()
        val owner = Owner(false, false, 0) { "0" }
        val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }
            .get(owner) as android.os.HandlerThread
        Exact.owner = owner
        owner.running(false)
        owner.asked = { Requests(activity).perform(it) }
        try {
            controller.pause().stop().destroy()
            assertNull(Exact.reader)
            owner.call { Host.pending.set(intArrayOf(19, 3, 1, 2) + authRequest(1)) }
            owner.ask { } // Request delivery is now queued on main.
            shadowOf(Looper.getMainLooper()).idle()
            owner.ask { } // The cancellation has reached the owner.
            assertEquals(listOf(1 to "cancelled"), Host.answers)
            assertTrue(Host.opened.isEmpty())
            assertNull(shadowOf(activity).nextStartedActivity)
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(11))
            owner.ask { }
            assertEquals(listOf(1 to "cancelled"), Host.answers)
        } finally {
            Exact.owner = null
            thread.quitSafely(); thread.join(5000)
            assertFalse(thread.isAlive)
            controller = Robolectric.buildActivity(Reader::class.java, intent())
            controller.create().start().resume()
        }
    }

    @Test fun aHostlessCustomSchemeCallbackIsClaimedAndDelivered() {
        browser(true)
        val requests = Requests(activity)
        requests.perform(authRequest(1))
        assertEquals(listOf(1), Host.opened)
        assertEquals("https://login.test/authorize", shadowOf(activity).nextStartedActivity.dataString)
        val uri = Uri.parse("app.test:/cb?code=secret&state=s1")
        assertNull(uri.encodedAuthority)
        assertTrue(Auth.claims(activity, uri))
        val callback = intent(uri.toString())
        assertTrue(Auth.receive(activity, callback))
        assertEquals(listOf(uri.toString()), Host.urls)
        assertNull(callback.data)
        requests.perform(intArrayOf(19, 2, 1, 1))
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(11))
        assertTrue(Host.answers.isEmpty())
    }

    @Test fun aDeclaredEndpointWithoutALiveWaitDoesNotSuppressReturnCancellation() {
        browser(true)
        callbacks("[\"app.test:/cb\",\"app.test:/other\"]")
        assertTrue(Auth.claims(activity, Uri.parse("app.test:/other")))
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        controller.pause().stop()
        controller.newIntent(intent("app.test:/other?code=secret&state=s1")).start().resume()
        assertTrue(Host.urls.isEmpty())
        assertNull(activity.intent.data)
        assertEquals(listOf(1 to "cancelled"), Host.answers)
    }

    @Test fun userinfoOnTheLiveEndpointIsDeliveredForTheRunnerToReject() {
        browser(true)
        callbacks("[\"app.test://host/cb\"]")
        Auth.open(activity, 1, "https://login.test/authorize", "app.test://host/cb", "s1")
        controller.pause().stop()
        val url = "app.test://secret@host/cb?state=s1"
        controller.newIntent(intent(url)).start().resume()
        assertEquals(listOf(url), Host.urls)
        assertNull(activity.intent.data)
        assertTrue(Host.answers.isEmpty())
        // The runner ends the wait after settling the callback with 502.
        Auth.end(1)
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(11))
        assertTrue(Host.answers.isEmpty())
    }

    private val links = listOf("app.test:/inbox", "app.test://host/cb", "app.test:/cb/", "app.test:/%63b")

    @Test fun sameSchemeOrdinaryLinksReachWarmNavigation() {
        for (url in links) {
            controller.newIntent(intent(url))
            assertEquals(url, Host.urls.lastOrNull())
            assertEquals(url, activity.intent.dataString)
        }
    }

    @Test fun sameSchemeOrdinaryLinksReachColdNavigation() {
        for (url in links) {
            controller.pause().stop().destroy()
            Exact.live = false
            controller = Robolectric.buildActivity(Reader::class.java, intent(url))
            controller.create().start().resume()
            assertEquals(url, Environment.values["EXACT_LAUNCH_URL"])
        }
    }

    @Test fun aCallbackRecreatesTheActivityWithSavedStateInTheLiveProcess() {
        browser(true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        val saved = Bundle()
        controller.pause().saveInstanceState(saved).stop().destroy()
        val url = "app.test:/cb?code=secret&state=s1"
        controller = Robolectric.buildActivity(Reader::class.java, intent(url))
        controller.create(saved).start().resume()
        assertEquals(listOf(url), Host.urls)
        assertNull(activity.intent.data)
        assertTrue(Host.answers.isEmpty())
    }

    @Test fun coldAndHistoryCallbacksAreDroppedThroughOnCreate() {
        for (cold in listOf(true, false)) {
            controller.pause().stop().destroy()
            Exact.live = !cold
            val launch = intent("app.test:/cb?code=secret&state=s1")
            if (!cold) launch.addFlags(Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY)
            controller = Robolectric.buildActivity(Reader::class.java, launch)
            controller.create().start().resume()
            assertTrue(Host.urls.isEmpty())
            assertNull(activity.intent.data)
            assertNotEquals("app.test:/cb?code=secret&state=s1", Environment.values["EXACT_LAUNCH_URL"])
        }
    }

    @Test fun aStopThenResumeWithoutACallbackCancelsImmediately() {
        browser(true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        controller.pause().stop().start().resume()
        assertEquals(listOf(1 to "cancelled"), Host.answers)
    }

    @Test fun aNonCallbackViewIntentCancelsTheWaitingSignInOnReturn() {
        browser(true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        controller.pause().stop()
        val url = "app.test:/inbox"
        controller.newIntent(intent(url)).start().resume()
        assertEquals(listOf(url), Host.urls)
        assertEquals(url, activity.intent.dataString)
        assertEquals(listOf(1 to "cancelled"), Host.answers)
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(11))
        assertEquals(listOf(1 to "cancelled"), Host.answers)
    }

    @Test fun aPauseWithoutAStopKeepsTheWait() {
        browser(true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        // A permission dialog or a multi-window transition can pause without stopping the activity.
        controller.pause().resume()
        assertTrue(Host.answers.isEmpty())
        val url = "app.test:/cb?code=secret&state=s1"
        controller.newIntent(intent(url))
        assertEquals(listOf(url), Host.urls)
    }

    @Test fun recreationWithoutAResponseCancelsTheProcessWait() {
        browser(true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        val saved = Bundle()
        controller.pause().saveInstanceState(saved).stop().destroy()
        controller = Robolectric.buildActivity(Reader::class.java, intent())
        controller.create(saved).start().resume()
        assertEquals(listOf(1 to "cancelled"), Host.answers)
    }

    @Test fun aResponseBeforeResumeLeavesRejectionToTheRunner() {
        browser(true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        controller.pause().stop()
        val url = "app.test:/cb?code=x&state=s1&state=s1"
        controller.newIntent(intent(url)).start().resume()
        assertEquals(listOf(url), Host.urls)
        assertTrue(Host.answers.isEmpty())
    }

    @Test fun aStaleTabCannotEndTheRetryOrTurnItsResumeIntoCancellation() {
        browser(true)
        val requests = Requests(activity)
        requests.perform(authRequest(1))
        controller.pause().stop().start().resume()
        assertEquals(listOf(1 to "cancelled"), Host.answers)
        Host.answers.clear()
        requests.perform(authRequest(2, "s2"))
        controller.pause().stop()
        controller.newIntent(intent("app.test:/cb?code=old&state=s1")).start().resume()
        assertTrue(Host.urls.isEmpty())
        assertTrue(Host.answers.isEmpty())
        assertNull(activity.intent.data)
        controller.pause().stop()
        controller.newIntent(intent("app.test:/cb?code=missing-state")).start().resume()
        assertTrue(Host.urls.isEmpty())
        assertTrue(Host.answers.isEmpty())
        val url = "app.test:/cb?code=new&st%61te=%73%32"
        controller.newIntent(intent(url))
        assertEquals(listOf(url), Host.urls)
        assertTrue(Host.answers.isEmpty())
    }

    @Test fun sameEndpointWaitsReceiveOnlyTheirOwnState() {
        browser(true)
        val requests = Requests(activity)
        requests.perform(authRequest(1))
        requests.perform(authRequest(2, "s2"))
        controller.pause().stop()
        val second = "app.test:/cb?state=s2"
        controller.newIntent(intent(second)).start().resume()
        assertEquals(listOf(second), Host.urls)
        assertTrue(Host.answers.isEmpty())
        Auth.end(2)
        val first = "app.test:/cb?state=s1"
        controller.newIntent(intent(first))
        assertEquals(listOf(second, first), Host.urls)
        assertTrue(Host.answers.isEmpty())
        // A later ordinary return still cancels: callback suppression lasts one resume.
        controller.pause().stop().start().resume()
        assertEquals(listOf(1 to "cancelled"), Host.answers)
    }

    @Test fun defaultBrowserWaitsUntilTheBoundAndForgettingCancelsOnlyTheWait() {
        browser(false)
        Auth.open(activity, 2, "https://login.test/authorize", "app.test:/cb", "s1")
        assertEquals("test.browser", shadowOf(activity).nextStartedActivity.`package`)
        // With no return to the activity, the wait still has a bound.
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(9))
        assertTrue(Host.answers.isEmpty())
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(1))
        assertEquals(listOf(2 to "cancelled"), Host.answers)
        Auth.open(activity, 3, "https://login.test/authorize", "app.test:/cb", "s1")
        Auth.end(3)
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(11))
        assertEquals(1, Host.answers.size)
    }

    @Test fun aCallbackAfterDeepSleepExpiresTheWaitWithoutForwardingTheUrl() {
        browser(true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        controller.pause().stop()
        val uptime = SystemClock.uptimeMillis()
        val elapsed = SystemClock.elapsedRealtime()
        ShadowSystemClock.simulateDeepSleep(Duration.ofMinutes(11))
        assertEquals(uptime, SystemClock.uptimeMillis())
        assertEquals(elapsed + Duration.ofMinutes(11).toMillis(), SystemClock.elapsedRealtime())
        assertTrue(Host.answers.isEmpty())
        val callback = intent("app.test:/cb?code=secret&state=s1")
        assertTrue(Auth.receive(activity, callback))
        assertNull(callback.data)
        assertTrue(Host.urls.isEmpty())
        assertEquals(listOf(1 to "cancelled"), Host.answers)
        controller.start().resume()
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(11))
        assertEquals(listOf(1 to "cancelled"), Host.answers)
    }

    @Test fun resumeAfterDeepSleepEmitsCancellationAfterTheDeadline() {
        browser(true)
        Auth.open(activity, 1, "https://login.test/authorize", "app.test:/cb", "s1")
        controller.pause().stop()
        controller.newIntent(intent("app.test:/cb?code=x&state=s1"))
        assertEquals(1, Host.urls.size)
        val uptime = SystemClock.uptimeMillis()
        ShadowSystemClock.simulateDeepSleep(Duration.ofMinutes(11))
        assertEquals(uptime, SystemClock.uptimeMillis())
        assertTrue(Host.answers.isEmpty())
        controller.start().resume()
        assertEquals(listOf(1 to "cancelled"), Host.answers)
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(11))
        assertEquals(listOf(1 to "cancelled"), Host.answers)
    }

    @Test fun noBrowserReportsUnavailabilityWithoutTheAuthorizationUrl() {
        Auth.open(activity, 4, "https://login.test/authorize?secret=value", "app.test:/cb", "s1")
        assertEquals(listOf(4 to "unavailable"), Host.answers)
        assertNull(shadowOf(activity).nextStartedActivity)
    }
}
