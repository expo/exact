package exact.backgroundtransfer

import android.app.ActivityManager
import android.content.Context
import android.os.Looper
import android.os.HandlerThread
import dev.exact.canvas.Exact
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Extensions
import dev.exact.canvas.Native
import dev.exact.canvas.Owner
import exact.app.ExactExtensions
import exact.backgroundtransfer.Lifecycle.Conditions
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.JobStarted
import exact.backgroundtransfer.Lifecycle.JobStopped
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Limits
import exact.backgroundtransfer.Lifecycle.Run
import exact.backgroundtransfer.Lifecycle.Stop
import exact.backgroundtransfer.Lifecycle.Write
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.LooperMode
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/** The module over [Transfers], with [Sessions] as the host: the package's calls, its session's life and its wakes. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], manifest = Config.NONE, shadows = [BackgroundTransferTest.Session::class, BackgroundTransferTest.Transport::class, BackgroundTransferTest.Environment::class])
@LooperMode(LooperMode.Mode.PAUSED)
class BackgroundTransferTest {
    @Implements(android.system.Os::class)
    class Environment {
        companion object {
            var fresh = false
            @JvmStatic @Implementation fun environ(): Array<String> = emptyArray()
            @JvmStatic @Implementation fun getenv(name: String): String? = if (name == "EXACT_AGENT_STORAGE_FRESH" && fresh) "1" else null
        }
    }

    @Implements(Native::class, isInAndroidSdk = false)
    class Session {
        companion object {
            lateinit var root: File
            /** Whether a drive runs the session: its implementation told so ([agent], off under `--bind`), and its
             * storage the drive's ([scratch], whatever it binds). */
            var agent = false
            var scratch = false
            /** Whether the library has the entry point that says so. */
            var linked = true
            val wakes = CopyOnWriteArrayList<JSONObject>()
            val expired = CopyOnWriteArrayList<String>()
            /** What the host answered the runner, by request. */
            val answers = ConcurrentHashMap<Int, String>()
            /** The hosts booted, and what each was last told of the page: hidden, then whether it may execute. */
            val boots = AtomicInteger()
            @Volatile var page: Pair<Boolean, Boolean>? = null
            /** Whether the store's writes reach the disk when the host waits for them, and what happened in order. */
            @Volatile var durable = true
            val order = CopyOnWriteArrayList<String>()
            @JvmStatic @Implementation fun start(width: Int, height: Int, scale: Float, dir: String, store: String, live: Boolean): Boolean {
                boots.incrementAndGet()
                return true
            }
            @JvmStatic @Implementation fun fds() = IntArray(0)
            /** No timer is due. */
            @JvmStatic @Implementation fun background(applying: Boolean) = -1.0
            @JvmStatic @Implementation fun hidden(hidden: Boolean, eligible: Boolean) { page = hidden to eligible }
            @JvmStatic @Implementation fun storeDrain(ms: Int): Boolean { order += "store"; return durable }
            /** The data module: 1 active, 0 while its image loads, -1 failed to activate. And whether each asking
             * was of a host no reader takes a frame from. */
            @Volatile var data = 1
            val unshown = CopyOnWriteArrayList<Boolean>()
            @JvmStatic @Implementation fun dataActive(unshown: Boolean): Int { this.unshown += unshown; return data }
            @JvmStatic @Implementation fun trust(context: Context) {}
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = agent
            @JvmStatic @Implementation fun scratchStorage(): Boolean = if (linked) scratch else throw UnsatisfiedLinkError("scratchStorage")
            @JvmStatic @Implementation fun appRoot(index: Int) = File(root, listOf("data", "cache", "tmp")[index]).path
            @JvmStatic @Implementation fun answer(id: Int, answer: String?) { answers[id] = answer ?: "" }
            @JvmStatic @Implementation fun changed(topic: String) {
                if (topic.startsWith("exact-wake:")) wakes += JSONObject(topic.removePrefix("exact-wake:"))
                if (topic.startsWith("exact-wake-expired:")) expired += JSONObject(topic.removePrefix("exact-wake-expired:")).getString("id")
            }
        }
    }
    @Implements(NativeTransfer::class, isInAndroidSdk = false)
    class Transport {
        companion object { @JvmStatic @Implementation fun __staticInitializer__() {} }
    }
    private class Store(root: File) : Registrations(root) {
        var failing = false
        @Synchronized override fun write(write: Write, admission: Admission?, response: Response?) {
            if (failing) { failing = false; throw IOException("disk full") }
            super.write(write, admission, response)
        }
    }

    private lateinit var base: File
    private lateinit var store: Store
    private lateinit var transport: FakeTransport
    private lateinit var transfers: Transfers
    private lateinit var context: ExtensionContext
    private var module: BackgroundTransferModule? = null
    private var requests = 0
    private val platform = FakePlatform()
    private companion object {
        const val NAME = "background-transfer"
        const val AUTHORITY = "net.fetch https://example.test\nfs.write app:/data"
    }
    private val main get() = shadowOf(Looper.getMainLooper())
    private val app: Context get() = RuntimeEnvironment.getApplication()

    @Before fun setup() {
        base = Files.createTempDirectory("transfer-module").toFile()
        Session.root = base
        Session.agent = false
        Environment.fresh = false
        Session.scratch = false
        Session.linked = true
        Session.wakes.clear()
        Session.expired.clear()
        Session.answers.clear()
        Session.boots.set(0)
        Session.page = null
        Session.durable = true
        Session.data = 1
        Session.unshown.clear()
        Session.order.clear()
        platform.finish = { Session.order += "finish" }
        listOf("data", "cache", "tmp").forEach { File(base, it).mkdirs() }
        transport = FakeTransport(base)
        process()
    }
    @After fun cleanup() {
        close()
        Extensions.stopped(false)
        Jobs.visible = null
        Sessions.application = null
        Exact.owner?.let { owner ->
            val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }.get(owner) as HandlerThread
            thread.quitSafely(); thread.join(5000)
        }
        Exact.owner = null
        Exact.live = false
        Exact.hidden(false)
        base.deleteRecursively()
    }

    /** A process: its transfers over what the last one left. Every effect and operation runs where it is asked for. */
    private fun process(limits: Limits = Limits()) {
        store = Store(File(base, "private"))
        transfers = Transfers(store, transport::exchange, platform, Sessions, Fixture.inline, Fixture.inline { transfers }, limits)
    }
    /**
     * The session starts, with an activity that shows it when [visible]. The host makes the module from the app's
     * roster at its first frame, as the interface's `"start": "load"` asks.
     */
    private fun open(visible: Boolean = true) {
        if (visible) Extensions.started() else Extensions.stopped(false)
        ExactExtensions.process(mapOf(NAME to { made ->
            context = made
            BackgroundTransferModule(made, { transfers }, transport::exchange, Executor { it.run() }).also { module = it }
        }), atLoad = listOf(NAME))
        Extensions.bind(app)
        Extensions.firstFrame()
        context.authority = AUTHORITY
        main.idle()
    }
    private fun close() {
        module?.destroy {}
        module = null
        ExactExtensions.process()
        main.idle()
    }
    /**
     * The process died with its session, and Android started another for a job: no activity, no host, the app's
     * roster, and the transfers over what the last one left.
     */
    private fun jobProcess(limits: Limits = Limits()) {
        close()
        process(limits)
        ExactExtensions.process(mapOf(NAME to { made ->
            context = made
            BackgroundTransferModule(made, { transfers }, transport::exchange, Executor { it.run() }).also { module = it }
        }), atLoad = listOf(NAME))
        Sessions.application = app
        // The job's entry, as every entrant's.
        transfers.enter(Lifecycle.Namespace("app"))
    }
    /** What the main thread, the host's thread and the sessions' events owe one another has run. */
    private fun settle() {
        repeat(4) {
            main.idle()
            Exact.owner?.ask { }
            val reached = CountDownLatch(1)
            Sessions.events.execute { reached.countDown() }
            assertTrue(reached.await(5, TimeUnit.SECONDS))
        }
        main.idle()
    }
    /** A request of the runner's, through the host's seam ([Extensions.call]); what the host answered it. */
    private fun host(body: JSONObject): String? {
        val id = ++requests
        Extensions.call(id, body.put("extension", NAME).toString())
        main.idle()
        return Session.answers.remove(id)
    }
    /**
     * A start as the runner forwards it to the host: marked [activated] when it comes inside a person's activation
     * window, and unmarked for a timer's or a resource's call and for one that comes after the window.
     */
    private fun forwarded(id: String, activated: Boolean): JSONObject {
        val args = named(id).put("requests", JSONArray().put(JSONObject().put("url", "https://example.test/$id")))
        val body = JSONObject().put("send", "backgroundFetch").put("args", args).put("authority", AUTHORITY)
        return JSONObject(host(if (activated) body.put("userActivation", true) else body)!!)
    }
    private fun call(name: String, args: JSONObject = JSONObject()): JSONObject {
        var answer: JSONObject? = null
        module!!.later(name, args) { answer = it }
        main.idle()
        return answer!!
    }
    private fun named(id: String = "visitor") = JSONObject().put("id", id)
    private fun start(id: String = "visitor", url: String = "https://example.test/a%2Fb?token=private#hidden", saveTo: String? = null) =
        call("backgroundFetch", named(id).put("title", JSONObject.NULL).put("requests", JSONArray().put(JSONObject().put("url", url)
            .put("method", JSONObject.NULL).put("saveTo", saveTo ?: JSONObject.NULL)
            .put("headers", JSONArray().put(JSONObject().put("name", "Authorization").put("value", "secret"))))))
    private fun key(id: String = "visitor"): Key = transfers.namespaces().firstNotNullOf { transfers.find(it, id) }
    private fun record(key: Key) = store.durable().records[key]!!
    private fun run(key: Key, id: Int) { transfers.feed(JobStarted(key, Run(id, Job.Work), transfers.conditions)); main.idle() }
    /** The runner tells the wake's end. */
    private fun end(wake: JSONObject, outcome: String) { host(JSONObject().put("woke", wake.getString("id")).put("outcome", outcome)) }
    private fun shown(api: Int, visible: Boolean = true) { platform.conditions = Conditions(api, visible) }

    @Test fun aStartCapturesItsAuthorityAndKeepsCredentialsOffDisk() {
        open()
        val answer = start()
        assertEquals("visitor", answer.getString("id"))
        assertEquals("", answer.getString("result"))
        val key = key()
        val kept = store.get(key)!!
        assertEquals(context.authority, kept.getString("source"))
        assertEquals("app", kept.getString("origin"))
        assertEquals("visitor", kept.getString("title"))
        assertEquals(base.path + "/data", kept.getString("data"))
        assertEquals("GET", kept.getJSONArray("requests").getJSONObject(0).getString("method"))
        assertEquals("https://example.test/a%2Fb", kept.getJSONArray("requests").getJSONObject(0).getString("url"))
        assertFalse(kept.toString().contains("token=private") || kept.toString().contains("secret"))
        assertEquals("secret", transport.secrets.values.single().getJSONArray("headers").getJSONObject(0).getString("value"))
        assertEquals(context.authority, transport.secrets.values.single().getString("authority"))
        // Nothing marked the call a person's: a start is a program's, though the app shows.
        assertEquals(Lifecycle.Intent.Ordinary, record(key).intent)
        assertEquals(setOf(key to Job.Work), platform.jobs.toSet())
        assertEquals("visitor", call("backgroundFetchRegistration", named()).getJSONObject("exact:answer").getString("id"))
        assertEquals("[\"visitor\"]", call("backgroundFetchIds").getJSONArray("exact:answer").toString())
    }

    @Test fun aStartInsideAPersonsActivationWindowHoldsAUserInitiatedJob() {
        shown(api = 34)
        open()
        assertEquals("press", forwarded("press", activated = true).getString("id"))
        assertEquals(Lifecycle.Intent.Person, record(key("press")).intent)
        assertEquals(setOf(key("press") to Job.UserJob), platform.jobs.toSet())
        // The mark is each call's own: a timer's, a resource's, or a call after the window carries none.
        assertEquals("timer", forwarded("timer", activated = false).getString("id"))
        assertEquals(Lifecycle.Intent.Ordinary, record(key("timer")).intent)
        assertEquals(Job.Work, record(key("timer")).job)
    }

    @Test fun aPersonsStartBelowApi34IsWorkThatRunsInTheForegroundUnderAView() {
        shown(api = 33)
        open()
        forwarded("press", activated = true)
        forwarded("timer", activated = false)
        assertEquals(Lifecycle.Intent.Person to Job.Work, record(key("press")).let { it.intent to it.job })
        // The person's work run starts while the activity shows; a program's is never asked.
        run(key("press"), 1)
        run(key("timer"), 2)
        assertEquals(listOf(1), platform.promoted.toList())
    }

    @Test fun aPersonsWorkThatStartedWithNoViewIsPromotedWhenTheViewShows() {
        shown(api = 33)
        // The exchange never answers, so the run is still sending when the view comes back.
        transfers = Transfers(store, transport::exchange, platform, Sessions, Fixture.inline,
            { effect, _, body -> if (effect !is Lifecycle.Transmit) body()?.let { transfers.feed(it) } })
        open()
        forwarded("press", activated = true)
        module!!.foreground(false)
        shown(api = 33, visible = false)
        run(key("press"), 1)
        assertTrue("a foreground service does not start from the background", platform.promoted.isEmpty())
        shown(api = 33)
        module!!.foreground(true)
        assertEquals(listOf(1), platform.promoted.toList())
    }

    @Test fun anActivatedStartWithNoViewIsAPrograms() {
        shown(api = 34, visible = false)
        open(visible = false)
        forwarded("hidden", activated = true)
        assertEquals(Lifecycle.Intent.Ordinary, record(key("hidden")).intent)
        assertEquals(setOf(key("hidden") to Job.Work), platform.jobs.toSet())
    }

    @Test fun anAbortEndsTheTransferAndARetirementRemovesIt() {
        open()
        start()
        val key = key()
        assertTrue(call("abortBackgroundFetch", named()).getBoolean("exact:answer"))
        // The view is up, so the ending is delivered at once.
        val wake = Session.wakes.single()
        assertEquals("backgroundfetchabort", wake.getString("event"))
        assertEquals("transfer.${key.generation}.1", wake.getString("id"))
        assertEquals("aborted", wake.getJSONArray("args").getJSONObject(0).getString("failureReason"))
        end(wake, "completed")
        assertFalse(call("abortBackgroundFetch", named()).getBoolean("exact:answer"))
        assertFalse(call("retireBackgroundFetch", named()).has("error"))
        assertTrue(transport.secrets.isEmpty())
        assertTrue(platform.jobs.isEmpty())
        assertEquals(0, call("backgroundFetchIds").getJSONArray("exact:answer").length())
        assertEquals(JSONObject.NULL, call("backgroundFetchRegistration", named()).get("exact:answer"))
        assertFalse(call("abortBackgroundFetch", named()).getBoolean("exact:answer"))
        assertEquals("the id is free again", "visitor", start().getString("id"))
    }

    @Test fun aRefusedStartAnswersItsErrorAndKeepsNothing() {
        open()
        assertEquals("NotAllowedError", start(url = "https://denied.test/a").getString("error"))
        assertEquals(0, call("backgroundFetchIds").getJSONArray("exact:answer").length())
        assertTrue(transport.secrets.isEmpty())
        assertTrue(platform.jobs.isEmpty())
        assertEquals("visitor", start().getString("id"))
        assertEquals("an id is registered once", "TypeError", start().getString("error"))
        assertEquals(1, transport.secrets.size)
        for (bad in listOf(named(""), named("none").put("requests", JSONArray()), named("x".repeat(65)),
            JSONObject("{\"id\":\"total\",\"downloadTotal\":-1,\"requests\":[{\"url\":\"https://example.test/a\"}]}"))) {
            assertEquals(bad.toString(), "TypeError", call("backgroundFetch", bad).getString("error"))
        }
        assertEquals("NotSupportedError", call("pauseBackgroundFetch", named()).getString("error"))
    }

    @Test fun anAppRestrictedInTheBackgroundIsDenied() {
        open()
        assertEquals("granted", call("backgroundFetchStatus").getString("state"))
        assertTrue(call("backgroundFetchStatus").getBoolean("available"))
        shadowOf(app.getSystemService(ActivityManager::class.java)).setBackgroundRestricted(true)
        assertEquals("denied", call("backgroundFetchStatus").getString("state"))
        assertEquals("NotAllowedError", start().getString("error"))
        assertTrue(transfers.ids(Fixture.app).isEmpty())
    }

    @Test fun admissionUsesTheSharedParsersPublicUrl() {
        open()
        for ((url, public) in listOf("https://example.test/file?secret=1#part" to "https://example.test/file",
            "http://upload_server.example/file" to "http://upload_server.example/file")) {
            val answer = start(url = url)
            assertFalse(answer.toString(), answer.has("error"))
            assertEquals(public, store.get(key())!!.getJSONArray("requests").getJSONObject(0).getString("url"))
            assertEquals(url, transport.seen.last { it.getString("op") == "keep" }.getString("url"))
            call("retireBackgroundFetch", named())
        }
    }

    @Test fun aLaunchDeliversAnEndingNoSessionHeard() {
        open(visible = false)
        start()
        val key = key()
        close()
        // With no session, and no application to boot one for, the host's seam reports each wake expired.
        for (id in 1..3) run(key, id)
        assertTrue(Session.wakes.isEmpty())
        assertEquals(listOf(1 to true, 2 to true, 3 to false), platform.finished.toList())
        assertTrue(platform.jobs.isEmpty())
        process()
        open()
        val wake = Session.wakes.single()
        assertEquals("backgroundfetchsuccess", wake.getString("event"))
        assertEquals("transfer.${key.generation}.4", wake.getString("id"))
        val path = wake.getJSONArray("args").getJSONObject(0).getJSONArray("records").getJSONObject(0).getString("path")
        assertEquals("response", File(base, path.removePrefix("app:/")).readText())
        end(wake, "completed")
        // An unretired ending is delivered again at the next launch.
        close()
        open()
        assertEquals("transfer.${key.generation}.5", Session.wakes.last().getString("id"))
        call("retireBackgroundFetch", named())
        assertFalse(File(base, path.removePrefix("app:/")).exists())
        assertFalse(File(base, "private/app.app/${key.generation}").exists())
    }

    @Test fun anEndingWithNoSessionRunsItsHandlerInOneBootedWithNoView() {
        open(visible = false)
        start()
        val key = key()
        jobProcess()
        run(key, 1)
        settle()
        assertEquals("the job's process boots one host", 1, Session.boots.get())
        assertNull("no activity is made", Exact.reader?.get())
        assertEquals("the page is hidden and may not execute, as a stopped activity's", true to false, Session.page)
        val wake = Session.wakes.single()
        assertEquals("backgroundfetchsuccess", wake.getString("event"))
        assertEquals("transfer.${key.generation}.1", wake.getString("id"))
        assertEquals("visitor", wake.getJSONArray("args").getJSONObject(0).getString("id"))
        assertTrue("the run is held through the handler", platform.finished.isEmpty())
        assertTrue(Session.order.isEmpty())
        end(wake, "completed")
        settle()
        assertEquals("the handler's writes are on disk before the job is given back", listOf("store", "finish"), Session.order.toList())
        assertEquals(listOf(1 to true), platform.finished.toList())
        // The next run's ending is raised in the session that runs: nothing boots again.
        run(key, 2)
        settle()
        assertEquals(1, Session.boots.get())
        assertEquals("transfer.${key.generation}.2", Session.wakes.last().getString("id"))
    }

    @Test fun aSessionWithNoViewIsReadyOnceItsDataModuleIsActive() {
        open(visible = false)
        start()
        val key = key()
        jobProcess()
        // The module's image is still loading when the host has booted.
        Session.data = 0
        run(key, 1)
        settle()
        assertEquals(1, Session.boots.get())
        assertTrue("no pixel waits for the module: it is activated with no frame", Session.unshown.isNotEmpty() && Session.unshown.all { it })
        assertNull("the package's module is not made before the data module answers", module)
        assertTrue("and no handler runs", Session.wakes.isEmpty())
        assertTrue(platform.finished.isEmpty())
        // The image has loaded, and the host's next turn finds the module active.
        Session.data = 1
        Exact.owner!!.call { }
        settle()
        assertEquals("transfer.${key.generation}.1", Session.wakes.single().getString("id"))
    }

    @Test fun aSessionWhoseDataModuleDoesNotActivateExpiresItsWake() {
        open(visible = false)
        start()
        val key = key()
        jobProcess()
        Session.data = -1
        run(key, 1)
        settle()
        assertEquals(1, Session.boots.get())
        assertTrue(Session.wakes.isEmpty())
        assertEquals("the run is to run again: its ending was not delivered", listOf(1 to true), platform.finished.toList())
    }

    @Test fun anEndingWithNoViewIsDeliveredThreeTimesThenWaitsForALaunch() {
        open(visible = false)
        start()
        val key = key()
        jobProcess()
        for (id in 1..3) {
            run(key, id)
            settle()
            assertEquals("transfer.${key.generation}.$id", Session.wakes.last().getString("id"))
            end(Session.wakes.last(), "completed")
            settle()
        }
        assertEquals(listOf(1 to true, 2 to true, 3 to false), platform.finished.toList())
        assertTrue("no job is held for a fourth", platform.jobs.isEmpty())
        assertEquals(3, Session.wakes.size)
        // A launch with a view delivers it again.
        Extensions.started()
        host(JSONObject().put("foreground", true))
        settle()
        assertEquals("transfer.${key.generation}.4", Session.wakes.last().getString("id"))
    }

    @Test fun aWakeWhoseWritesDidNotReachTheDiskExpires() {
        process(Limits(completedWakes = 1))
        open(visible = false)
        start()
        val key = key()
        jobProcess(Limits(completedWakes = 1))
        run(key, 1)
        settle()
        Session.durable = false
        end(Session.wakes.single(), "completed")
        settle()
        assertEquals("the run is to run again: its ending was not delivered", listOf(1 to true), platform.finished.toList())
        Session.durable = true
        run(key, 2)
        settle()
        end(Session.wakes.last(), "completed")
        settle()
        assertEquals(listOf(1 to true, 2 to false), platform.finished.toList())
    }

    @Test fun aRunStoppedWhileItsSessionBootsExpiresItsWake() {
        open(visible = false)
        start()
        val key = key()
        jobProcess()
        // The session is not ready yet: its data module's image is still loading.
        Session.data = 0
        run(key, 1)
        settle()
        transfers.feed(JobStopped(key, 1, Stop.System))
        settle()
        assertTrue(transfers.state().first.waking.isEmpty())
        Session.data = 1
        Exact.owner!!.call { }
        settle()
        assertTrue("the ending is not raised in the session that was ready afterwards", Session.wakes.isEmpty())
        assertEquals(Job.Work, record(key).job)
    }

    @Test fun aWakesOutcomeReachesTheLifecycle() {
        process(Limits(completedWakes = 1))
        open(visible = false)
        start("expired")
        start("completed")
        run(key("expired"), 1)
        run(key("completed"), 2)
        assertEquals(listOf("backgroundfetchsuccess", "backgroundfetchsuccess"), Session.wakes.map { it.getString("event") })
        assertTrue("each run is held through its wake", platform.finished.isEmpty())
        end(Session.wakes[0], "expired")
        assertEquals(listOf(1 to true), platform.finished.toList())
        end(Session.wakes[1], "completed")
        assertEquals(listOf(1 to true, 2 to false), platform.finished.toList())
        assertEquals(Job.Work to null, record(key("expired")).job to record(key("completed")).job)
    }

    @Test fun aStoppedRunExpiresItsWake() {
        open(visible = false)
        start()
        val key = key()
        run(key, 1)
        val wake = Session.wakes.single()
        transfers.feed(JobStopped(key, 1, Stop.System))
        main.idle()
        assertEquals(listOf(wake.getString("id")), Session.expired)
        end(wake, "expired")
        assertEquals(Job.Work, record(key).job)
    }

    @Test fun aSessionsEndSupersedesItsWakes() {
        open(visible = false)
        start()
        val key = key()
        run(key, 1)
        val wake = Session.wakes.single().getString("id")
        close()
        assertEquals(listOf(wake), Session.expired)
        val (process, _) = transfers.state()
        assertNull(process.session)
        assertTrue(process.waking.isEmpty())
        assertEquals(listOf(1 to true), platform.finished.toList())
        // The outcome the runner tells afterwards is no one's.
        context.ended.remove(wake)?.invoke("completed")
        open()
        assertEquals("transfer.${key.generation}.2", Session.wakes.last().getString("id"))
    }

    @Test fun theSessionsViewFollowsItsActivity() {
        open(visible = false)
        assertEquals(false, transfers.state().first.session!!.view)
        Extensions.started()
        module!!.resumed()
        assertEquals(true, transfers.state().first.session!!.view)
        module!!.foreground(false)
        assertEquals(false, transfers.state().first.session!!.view)
        module!!.foreground(true)
        module!!.foreground(true)
        assertEquals(true, transfers.state().first.session!!.view)
        val incarnation = transfers.state().first.session!!.incarnation
        close()
        open()
        assertNotEquals(incarnation, transfers.state().first.session!!.incarnation)
    }

    @Test fun formFieldsReachTheSecretStoreAndOnlyTheirKeyReachesTheSnapshot() {
        open()
        val form = JSONArray().put(JSONObject().put("name", "policy").put("value", "private policy"))
        val request = JSONObject().put("url", "https://example.test/upload").put("form", form)
        val args = named().put("requests", JSONArray().put(request))
        request.put("bodyFrom", "app:/data/conflict")
        assertEquals("TypeError", call("backgroundFetch", args).getString("error"))
        request.remove("bodyFrom")
        assertEquals("visitor", call("backgroundFetch", args).getString("id"))
        val kept = transport.seen.single { it.getString("op") == "keep" }
        assertEquals("POST", kept.getString("method"))
        assertEquals(form.toString(), kept.getJSONArray("form").toString())
        val copy = transport.seen.single { it.getString("op") == "snapshot" }
        assertEquals(kept.getString("key"), copy.getString("key"))
        assertTrue(copy.isNull("path"))
        assertFalse(store.get(key())!!.toString().contains("private policy"))
    }

    @Test fun aSaveToIsPlacedOnceAndARetirementLeavesIt() {
        open()
        start(saveTo = "app:/data/saved")
        assertEquals("app:/data/saved", transport.seen.single { it.getString("op") == "checkSave" }.getString("path"))
        val key = key()
        run(key, 1)
        val path = Session.wakes.single().getJSONArray("args").getJSONObject(0).getJSONArray("records").getJSONObject(0).getString("path")
        assertEquals("app:/data/saved", path)
        File(base, "data/saved").writeText("the app's now")
        end(Session.wakes.single(), "completed")
        close()
        process()
        open()
        assertEquals(1, transport.ops().count { it == "place" })
        call("retireBackgroundFetch", named())
        assertEquals("the app's now", File(base, "data/saved").readText())
        assertFalse("removePlaced" in transport.ops())
    }

    @Test fun aRetirementAnswersWhenItsFilesAreGoneAndTheIdIsFreeFromThen() {
        // The retirement's cleanup waits for the test to run it.
        val cleanups = ArrayList<() -> Lifecycle.Event?>()
        transfers = Transfers(store, transport::exchange, platform, Sessions, Fixture.inline,
            { effect, _, body -> if (effect is Lifecycle.Clean) cleanups += body else body()?.let { transfers.feed(it) } })
        open()
        start()
        run(key(), 1)
        assertTrue(File(base, "data/transfers/visitor/0").isFile)
        var retired: JSONObject? = null
        module!!.later("retireBackgroundFetch", named()) { retired = it }
        main.idle()
        assertNull(retired)
        assertEquals(JSONObject.NULL, call("backgroundFetchRegistration", named()).get("exact:answer"))
        // The first transfer's files are still under the id.
        assertEquals("InvalidStateError", start().getString("error"))
        transfers.feed(cleanups.single()()!!)
        main.idle()
        assertFalse(retired!!.has("error"))
        assertFalse(File(base, "data/transfers/visitor").exists())
        assertEquals("visitor", start().getString("id"))
    }

    @Test fun aLibraryThatCannotSayWhoseStorageItIsMakesNoModule() {
        Session.linked = false
        val made = ExtensionContext(app, NAME)
        // The app's namespace is not the answer to a question the library cannot be asked.
        assertThrows(UnsatisfiedLinkError::class.java) {
            BackgroundTransferModule(made, { transfers }, transport::exchange, Executor { it.run() })
        }
    }

    @Test fun anAgentDriveHasItsOwnNamespace() {
        open()
        start()
        close()
        Session.agent = true
        Session.scratch = true
        Session.root = File(base, "drive").also { File(it, "data").mkdirs() }
        open()
        assertEquals(0, call("backgroundFetchIds").getJSONArray("exact:answer").length())
        assertEquals("visitor", start().getString("id"))
        val drive = transfers.namespaces().single { it.agent }
        assertTrue(Regex("[0-9a-f]{16}").matches(drive.name))
        assertTrue(File(base, "private/agent.${drive.name}").isDirectory)
        assertEquals(listOf("visitor"), transfers.ids(drive))
        // The same store at a later launch is the same namespace, and another drive's store is not.
        close()
        open()
        assertEquals(1, call("backgroundFetchIds").getJSONArray("exact:answer").length())
        close()
        Session.root = File(base, "other").also { File(it, "data").mkdirs() }
        open()
        assertEquals(0, call("backgroundFetchIds").getJSONArray("exact:answer").length())
        assertEquals("visitor", start().getString("id"))
        assertEquals(2, transfers.namespaces().count { it.agent })
    }

    @Test fun aFreshDriveCancelsOnlyItsOwnTransfersAndCanReuseTheirIds() {
        open()
        start()
        close()
        Session.scratch = true // --bind selects the package while the drive owns its storage.
        Session.root = File(base, "other")
        open()
        start()
        close()
        val keep = platform.jobs.toSet()
        Session.root = File(base, "drive")
        open()
        start()
        val previous = (platform.jobs - keep).single().first
        close()
        process()
        // A worker entered first. A fresh drive still retires this generation.
        transfers.enter(previous.namespace)
        Environment.fresh = true
        open()
        assertEquals(0, call("backgroundFetchIds").getJSONArray("exact:answer").length())
        assertEquals(keep, platform.jobs)
        assertNull(store.get(previous))
        assertEquals("visitor", start().getString("id"))
        val replacement = (platform.jobs - keep).single().first
        assertNotEquals(previous, replacement)
        close()
        open()
        assertEquals(listOf("visitor"), transfers.ids(previous.namespace))
        assertNotNull(store.get(replacement))
        close()
        Session.scratch = false
        Session.root = base
        open()
        assertEquals(1, call("backgroundFetchIds").getJSONArray("exact:answer").length())
    }

    @Test fun aRetirementWhoseCleanupFailsSaysSoAndIsTriedAgain() {
        open()
        start()
        run(key(), 1)
        val key = key()
        val response = File(base, "data/transfers/visitor/0")
        assertTrue(response.isFile)
        // The secret store is locked: the credentials cannot be removed.
        transport.failing = setOf("forget")
        assertEquals("UnknownError", call("retireBackgroundFetch", named()).getString("error"))
        assertEquals(Lifecycle.Phase.Retired, record(key).phase)
        assertFalse(transport.secrets.isEmpty())
        // The registration is retired, and its id is taken while what it kept remains.
        assertEquals(JSONObject.NULL, call("backgroundFetchRegistration", named()).get("exact:answer"))
        assertEquals("InvalidStateError", start().getString("error"))
        assertEquals("UnknownError", call("retireBackgroundFetch", named()).getString("error"))
        // The store opens: the same call removes it, and answers once the record is gone.
        transport.failing = emptySet()
        var gone: Boolean? = null
        module!!.later("retireBackgroundFetch", named()) { gone = !it.has("error") && store.durable().records[key] == null }
        main.idle()
        assertEquals(true, gone)
        assertTrue(transport.secrets.isEmpty())
        assertFalse(response.exists())
        assertEquals("visitor", start().getString("id"))
        // Nothing of that id is left to retire.
        assertFalse(call("retireBackgroundFetch", named("absent")).has("error"))
    }

    @Test fun aRetirementCutShortByAFailedWriteSaysSo() {
        // The retirement's cleanup waits for the test to run it.
        val cleanups = ArrayList<() -> Lifecycle.Event?>()
        transfers = Transfers(store, transport::exchange, platform, Sessions, Fixture.inline,
            { effect, _, body -> if (effect is Lifecycle.Clean) cleanups += body else body()?.let { transfers.feed(it) } })
        open()
        start()
        start("other")
        var retired: JSONObject? = null
        module!!.later("retireBackgroundFetch", named()) { retired = it }
        main.idle()
        assertNull(retired)
        // Another call's write fails: the process goes on as its successor, and the retirement's removal is unknown.
        store.failing = true
        assertEquals("UnknownError", call("abortBackgroundFetch", named("other")).getString("error"))
        assertEquals("UnknownError", retired!!.getString("error"))
    }

    @Test fun aDriveThatBindsThePackageCannotReachTheAppsTransfers() {
        val app = Lifecycle.Namespace("app")
        // The app: a transfer that ended with a kept response, and one waiting with its credentials.
        open()
        start()
        run(key(), 1)
        start("waiting")
        val mine = setOf(key(), key("waiting"))
        val response = File(base, "data/transfers/visitor/0")
        assertTrue(response.isFile)
        val secrets = transport.secrets.keys.toSet()
        assertTrue(secrets.isNotEmpty())
        close()
        // A drive that binds the package as in production: told it is not under the agent, on the drive's store.
        Session.scratch = true
        Session.root = File(base, "drive").also { root -> listOf("data", "cache", "tmp").forEach { File(root, it).mkdirs() } }
        process()
        open()
        assertEquals(0, call("backgroundFetchIds").getJSONArray("exact:answer").length())
        for (id in listOf("visitor", "waiting")) {
            assertEquals(JSONObject.NULL, call("backgroundFetchRegistration", named(id)).get("exact:answer"))
            assertFalse(call("abortBackgroundFetch", named(id)).getBoolean("exact:answer"))
            call("retireBackgroundFetch", named(id))
        }
        assertEquals("the app's records stand", mine, store.durable().records.filterValues { it.phase != Lifecycle.Phase.Retired }.keys)
        assertTrue("with the response", response.isFile)
        assertEquals("and the credentials", secrets, transport.secrets.keys.toSet())
        // The same id is free in the drive, whose retirement removes only its own.
        assertEquals("visitor", start().getString("id"))
        val drive = transfers.namespaces().single { it.agent }
        val first = transfers.find(drive, "visitor")!!
        assertTrue(call("abortBackgroundFetch", named()).getBoolean("exact:answer"))
        assertFalse(call("retireBackgroundFetch", named()).has("error"))
        assertNull(store.durable().records[first])
        assertEquals(mine, store.durable().records.keys)
        assertTrue(response.isFile)
        assertEquals(secrets, transport.secrets.keys.toSet())
        assertEquals("visitor", start().getString("id"))
        val theirs = transfers.find(drive, "visitor")!!
        close()
        // A later process recovers each in its own namespace.
        Session.scratch = false
        Session.root = base
        process()
        open()
        val ids = { call("backgroundFetchIds").getJSONArray("exact:answer").let { a -> (0 until a.length()).map(a::getString).toSet() } }
        assertEquals(setOf("visitor", "waiting"), ids())
        assertEquals(mine, setOf(transfers.find(app, "visitor"), transfers.find(app, "waiting")))
        assertEquals(mine + theirs, store.durable().records.keys)
        close()
        Session.scratch = true
        Session.root = File(base, "drive")
        process()
        open()
        assertEquals(setOf("visitor"), ids())
        assertEquals(theirs, transfers.find(drive, "visitor"))
        assertEquals(mine + theirs, store.durable().records.keys)
    }

    @Test fun aCallAfterAFailedWriteRecoversTheSession() {
        open()
        start()
        store.failing = true
        assertEquals("UnknownError", call("abortBackgroundFetch", named()).getString("error"))
        assertNull(transfers.state().first.session)
        assertEquals("", call("backgroundFetchRegistration", named()).getJSONObject("exact:answer").getString("result"))
        assertEquals(true, transfers.state().first.session!!.view)
        assertTrue(call("abortBackgroundFetch", named()).getBoolean("exact:answer"))
        assertEquals("backgroundfetchabort", Session.wakes.single().getString("event"))
    }
}
