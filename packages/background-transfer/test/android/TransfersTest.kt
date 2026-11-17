package exact.backgroundtransfer

import exact.backgroundtransfer.Lifecycle.Abort
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.JobStarted
import exact.backgroundtransfer.Lifecycle.JobStopped
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Outcome
import exact.backgroundtransfer.Lifecycle.Phase
import exact.backgroundtransfer.Lifecycle.Process
import exact.backgroundtransfer.Lifecycle.Result
import exact.backgroundtransfer.Lifecycle.Retire
import exact.backgroundtransfer.Lifecycle.Run
import exact.backgroundtransfer.Lifecycle.SessionEnded
import exact.backgroundtransfer.Lifecycle.SessionStarted
import exact.backgroundtransfer.Lifecycle.Stop
import exact.backgroundtransfer.Lifecycle.Write
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.util.concurrent.CountDownLatch
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/** [Transfers] over a store on disk, with the platform, the host and the transport stood in for. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], manifest = Config.NONE)
class TransfersTest {
    private class Store(root: File) : Registrations(root) {
        var failing = false
        /** The writes that land before the failing one. */
        var landing = 0
        @Synchronized override fun write(write: Write, admission: Admission?, response: Response?) {
            if (failing && landing-- <= 0) { failing = false; landing = 0; throw IOException("disk full") }
            super.write(write, admission, response)
        }
    }

    private lateinit var root: File
    private lateinit var store: Store
    private lateinit var transport: FakeTransport
    private lateinit var transfers: Transfers
    private val platform = FakePlatform()
    private val host = FakeHost()
    private var control: ExecutorService? = null

    @Before fun setup() {
        root = Files.createTempDirectory("transfers-").toFile()
        store = Store(File(root, "private"))
        transport = FakeTransport(root)
    }
    @After fun cleanup() {
        control?.shutdownNow()
        root.deleteRecursively()
    }

    /** Every effect and operation runs where it is asked for, unless [threads]: then as in the app. */
    private fun make(host: Host = this.host, threads: Boolean = false) {
        transfers = if (threads) Transfers(store, transport::exchange, platform, host, Executors.newSingleThreadExecutor().also { control = it })
            else Transfers(store, transport::exchange, platform, host, Fixture.inline, Fixture.inline { transfers })
        transfers.enter(Fixture.app)
        transfers.feed(SessionStarted(Fixture.app, 1))
    }
    private fun record(key: Key) = store.durable().records[key]
    private fun dir(key: Key) = File(root, "private/app.app/${key.generation}")

    @Test fun aTransferRunsFromItsStartToARetiredEnding() {
        make()
        val key = Fixture.key("video")
        assertTrue(Fixture.start(transfers, key, root))
        assertEquals(listOf(Triple(key, true, null)), host.answers.toList())
        assertEquals(setOf(key to Job.Work), platform.jobs.toSet())
        // The credentials are the secret store's and nowhere on disk.
        assertEquals("https://example.test/a?token=private", transport.secrets.values.single().getString("url"))
        assertEquals("secret", transport.secrets.values.single().getJSONArray("headers").getJSONObject(0).getString("value"))
        val kept = File(dir(key), "record").readText()
        assertFalse(kept.contains("token=private") || kept.contains("secret"))
        assertEquals(listOf("video"), transfers.ids(Fixture.app))

        transfers.feed(JobStarted(key, Run(1, Job.Work), transfers.conditions))
        val run = transport.seen.single { it.getString("op") == "run" }
        assertEquals(transport.secrets.keys.single(), run.getString("key"))
        assertEquals("net.fetch https://example.test", run.getString("authority"))
        assertEquals(listOf("open", "run", "close", "place"), transport.ops().filter { it in setOf("open", "run", "close", "cancel", "place") })
        val (raised, delivery, ended) = host.raised.single()
        assertEquals(key to 1, raised to delivery)
        val public = transfers.read(key)!!
        assertEquals("success", public.getString("result"))
        val path = public.getJSONArray("records").getJSONObject(0).getString("path")
        assertEquals("app:/data/transfers/video/0", path)
        assertEquals("response", File(root, path.removePrefix("app:/")).readText())

        // The handler completed without retiring: the work delivers the ending again.
        ended(Outcome.Completed)
        assertEquals(listOf(1 to true), platform.finished.toList())
        transfers.feed(JobStarted(key, Run(2, Job.Work), transfers.conditions))
        assertEquals(2, host.raised.last().second)
        assertTrue(transfers.feed(Retire(key)).accepted)
        // The run outlives the retirement until its wake's outcome.
        assertEquals(1, platform.finished.size)
        host.raised.last().third(Outcome.Completed)
        assertEquals(listOf(1 to true, 2 to false), platform.finished.toList())
        assertNull(transfers.find(Fixture.app, "video"))
        assertNull(transfers.read(key))
        assertFalse(dir(key).exists())
        assertFalse(File(root, "data/transfers/video").exists())
        assertTrue(transport.secrets.isEmpty())
        assertTrue(platform.jobs.isEmpty())
    }

    @Test fun aFailedCleanupIsToldAndARetirementOfTheRetiredRecordCleansAgain() {
        make()
        val key = Fixture.key("video")
        assertTrue(Fixture.start(transfers, key, root))
        transport.failing = setOf("forget")
        val told = ArrayList<Boolean>()
        assertTrue(transfers.retire(key) { told += it })
        assertEquals(listOf(false), told)
        assertEquals(Phase.Retired, record(key)!!.phase)
        assertNull(transfers.find(Fixture.app, "video"))
        assertEquals(key, transfers.retired(Fixture.app, "video"))
        assertFalse("the id is taken while its record stands", Fixture.start(transfers, Fixture.key("video"), root))
        // Tried again while the store is still locked, then with it open.
        assertTrue(transfers.retire(key) { told += it })
        assertEquals(listOf(false, false), told)
        transport.failing = emptySet()
        assertTrue(transfers.retire(key) { ok -> told += ok && record(key) == null })
        assertEquals(listOf(false, false, true), told)
        assertTrue(transport.secrets.isEmpty())
        assertFalse(dir(key).exists())
        assertNull(transfers.retired(Fixture.app, "video"))
        assertTrue(Fixture.start(transfers, Fixture.key("video"), root))
    }

    @Test fun aRetirementDuringACleanupJoinsItAndStartsNoSecond() {
        val cleanups = ArrayList<() -> Lifecycle.Event?>()
        transfers = Transfers(store, transport::exchange, platform, host, Fixture.inline,
            { effect, _, body -> if (effect is Lifecycle.Clean) cleanups += body else body()?.let { transfers.feed(it) } })
        transfers.enter(Fixture.app)
        transfers.feed(SessionStarted(Fixture.app, 1))
        val key = Fixture.key("video")
        assertTrue(Fixture.start(transfers, key, root))
        val told = ArrayList<String>()
        assertTrue(transfers.retire(key) { told += "first $it" })
        assertTrue(transfers.retire(key) { told += "second $it" })
        assertEquals("one cleanup removes the files", 1, cleanups.size)
        assertTrue(told.isEmpty())
        transfers.feed(cleanups.single()()!!)
        assertEquals(listOf("first true", "second true"), told)
        assertNull(record(key))
    }

    @Test fun aRefusedCredentialFailsTheStartWithItsCodeAndKeepsNothing() {
        make()
        val key = Fixture.key("video")
        assertTrue(Fixture.start(transfers, key, root, url = "https://denied.test/a?token=private"))
        assertEquals(listOf(Triple(key, false, "NotAllowedError")), host.answers.toList())
        assertNull(transfers.find(Fixture.app, "video"))
        assertFalse(dir(key).exists())
        assertTrue(platform.jobs.isEmpty())
        assertTrue(transport.secrets.isEmpty())
        // The id is free again.
        assertTrue(Fixture.start(transfers, Fixture.key("video"), root))
    }

    @Test fun anIdIsRegisteredOnceUntilItIsRetired() {
        make()
        val key = Fixture.key("video")
        assertTrue(Fixture.start(transfers, key, root))
        assertFalse(Fixture.start(transfers, Fixture.key("video"), root))
        assertEquals(1, transport.secrets.size)
        assertEquals(key, transfers.find(Fixture.app, "video"))
        assertEquals(listOf(key.generation), dir(key).parentFile!!.list()!!.toList())
    }

    @Test fun aResponseIsJudgedByItsStatusAndTheDownloadTotal() {
        make()
        val bad = Fixture.key("bad")
        Fixture.start(transfers, bad, root)
        transport.run = { request ->
            File(request.getString("response")).writeText("not found")
            JSONObject().put("status", 404).put("contentType", "text/plain")
        }
        transfers.feed(JobStarted(bad, Run(1, Job.Work), transfers.conditions))
        assertEquals("bad-status", transfers.read(bad)!!.getString("failureReason"))
        // A bad status keeps its body.
        assertEquals(404, transfers.read(bad)!!.getJSONArray("records").getJSONObject(0).getInt("status"))
        assertEquals("not found", File(dir(bad), "0.response").readText())

        val large = Fixture.key("large")
        Fixture.start(transfers, large, root, total = 4)
        transport.run = { request ->
            assertEquals(4L, request.getLong("remaining"))
            File(request.getString("response")).writeText("too long")
            JSONObject().put("status", 200).put("contentType", "text/plain")
        }
        transfers.feed(JobStarted(large, Run(2, Job.Work), transfers.conditions))
        assertEquals("download-total-exceeded", transfers.read(large)!!.getString("failureReason"))
        assertEquals(0, transfers.read(large)!!.getJSONArray("records").length())
        assertEquals(listOf("record"), dir(large).list()!!.toList())

        val invalid = Fixture.key("invalid")
        Fixture.start(transfers, invalid, root)
        transport.run = { JSONObject().put("status", 999) }
        transfers.feed(JobStarted(invalid, Run(3, Job.Work), transfers.conditions))
        assertEquals("fetch-error", transfers.read(invalid)!!.getString("failureReason"))
    }

    @Test fun aNetworkFailureLeavesTheWorkToRunAgain() {
        make()
        val key = Fixture.key("video")
        Fixture.start(transfers, key, root)
        transport.run = { JSONObject().put("failure", "network") }
        transfers.feed(JobStarted(key, Run(1, Job.Work), transfers.conditions))
        assertEquals(listOf(1 to true), platform.finished.toList())
        assertEquals(Phase.Ready to 1, record(key)!!.let { it.phase to it.retries })
        assertEquals("", transfers.read(key)!!.getString("result"))
        assertEquals(listOf("record"), dir(key).list()!!.toList())
    }

    @Test fun anAbortDoesNotWaitBehindACopy() {
        val copying = CountDownLatch(1)
        val release = CountDownLatch(1)
        transport.snapshot = { copying.countDown(); assertTrue(release.await(5, TimeUnit.SECONDS)); it.writeText("upload") }
        make(threads = true)
        val key = Fixture.key("video")
        try {
            assertTrue(Fixture.start(transfers, key, root, upload = true))
            assertTrue(copying.await(5, TimeUnit.SECONDS))
            assertTrue(host.answers.isEmpty())
            assertTrue(transfers.feed(Abort(key)).accepted)
            assertEquals("aborted", transfers.read(key)!!.getString("failureReason"))
            // The queue goes on while the copy is still held: the start is answered.
            Fixture.await("the start's answer") { host.answers.isNotEmpty() }
            assertEquals(Triple(key, true, null), host.answers.single())
            assertEquals(1L, release.count)
        } finally { release.countDown() }
        Fixture.await("the copy's report") { transfers.state().first.draining.isEmpty() }
        // The cancelled copy committed nothing.
        assertEquals(listOf("record"), dir(key).list()!!.toList())
        assertEquals(Result.Aborted, record(key)!!.result)
    }

    @Test fun aStopCancelsTheExchangeThroughItsHandle() {
        val sending = CountDownLatch(1)
        val cancelled = CountDownLatch(1)
        transport.run = { sending.countDown(); assertTrue(cancelled.await(5, TimeUnit.SECONDS)); JSONObject().put("failure", "cancelled") }
        make(threads = true)
        val key = Fixture.key("video")
        Fixture.start(transfers, key, root)
        Fixture.await("the start's answer") { host.answers.isNotEmpty() }
        transfers.feed(JobStarted(key, Run(1, Job.Work), transfers.conditions))
        assertTrue(sending.await(5, TimeUnit.SECONDS))
        val handle = transport.seen.single { it.getString("op") == "run" }.getLong("handle")
        val watch = Thread {
            Fixture.await("the cancellation") { transport.seen.toList().any { it.getString("op") == "cancel" && it.getLong("handle") == handle } }
            cancelled.countDown()
        }.apply { start() }
        transfers.feed(JobStopped(key, 1, Stop.System))
        watch.join(6000)
        Fixture.await("the exchange's report") { transfers.state().first.draining.isEmpty() }
        assertEquals(listOf(1 to true), platform.finished.toList())
        assertEquals(Phase.Ready to 1, record(key)!!.let { it.phase to it.stops })
        Fixture.await("the handle's close") { "close" in transport.ops() }
    }

    @Test fun withNoHostHalfAnEndingSpendsItsBackgroundWakesAndWaitsForALaunch() {
        make(host = Sessions)
        val key = Fixture.key("video")
        Fixture.start(transfers, key, root)
        transfers.feed(SessionEnded(Fixture.app, 1))
        // No session is live: each wake the work asks for is reported expired.
        for (run in 1..3) {
            assertEquals(Job.Work, record(key)!!.job)
            transfers.feed(JobStarted(key, Run(run, Job.Work), transfers.conditions))
        }
        assertEquals(listOf(1 to true, 2 to true, 3 to false), platform.finished.toList())
        val ending = record(key)!!
        assertEquals(Result.Success, ending.result)
        assertEquals(Triple(null, 3, 3), Triple(ending.job, ending.wakes, ending.delivery))
        assertEquals(1, transport.ops().count { it == "run" })
        // The ending is kept, placed, for the launch that delivers it.
        assertEquals("response", File(root, "data/transfers/video/0").readText())
        assertEquals("success", transfers.read(key)!!.getString("result"))
        transfers.feed(JobStarted(key, Run(4, Job.Work), transfers.conditions))
        assertEquals(3 to 3, record(key)!!.let { it.wakes to it.delivery })
    }

    @Test fun aFailedWriteLeavesTheProcessAsItsSuccessorWouldBe() {
        make()
        val key = Fixture.key("video")
        Fixture.start(transfers, key, root)
        val kept = transport.run
        transport.run = { request -> store.failing = true; kept(request) }
        assertThrows(IOException::class.java) { transfers.feed(JobStarted(key, Run(1, Job.Work), transfers.conditions)) }
        // The response was not counted, and the run is let go to run again.
        assertEquals(Process(), transfers.state().first)
        assertTrue(transfers.state().second.isEmpty())
        assertEquals(listOf(1 to true), platform.finished.toList())
        assertEquals(Phase.Sending to 0, record(key)!!.let { it.phase to it.done })
        assertTrue(host.raised.isEmpty())
        // No event is taken before the namespace is entered again.
        assertFalse(transfers.feed(Abort(key)).accepted)
        transport.run = kept
        assertNotNull(transfers.enter(Fixture.app))
        transfers.feed(JobStarted(key, Run(2, Job.Work), transfers.conditions))
        assertEquals(key to 1, host.raised.single().let { it.first to it.second })
        assertEquals(Result.Success to 2, record(key)!!.let { it.result to it.attempt })
        assertEquals(setOf("record", "0.response"), dir(key).list()!!.toSet())
    }

    @Test fun whatAFailedStepWroteIsReadAtTheNextRecovery() {
        make()
        val key = Fixture.key("video")
        Fixture.start(transfers, key, root)
        // A new process reads a force-stop, and its recovery fails once the mark is written.
        platform.stopped = true
        store.failing = true
        store.landing = 1
        transfers = Transfers(store, transport::exchange, platform, host, Fixture.inline, Fixture.inline { transfers })
        assertThrows(IOException::class.java) { transfers.enter(Fixture.app) }
        assertNull(record(key)!!.result)
        // The platform's evidence is gone; the mark on disk is not.
        platform.stopped = false
        assertNotNull(transfers.enter(Fixture.app))
        assertEquals(Result.Aborted, record(key)!!.result)
    }

    @Test fun afterAFailedWriteNothingIsEnteredUntilTheOperationsItCutShortHaveReturned() {
        val copying = CountDownLatch(1)
        val release = CountDownLatch(1)
        val copies = AtomicInteger()
        transport.snapshot = { file ->
            if (copies.incrementAndGet() == 1) { copying.countDown(); assertTrue(release.await(5, TimeUnit.SECONDS)) }
            file.writeText("upload")
        }
        make(threads = true)
        val key = Fixture.key("video")
        try {
            assertTrue(Fixture.start(transfers, key, root, upload = true))
            assertTrue(copying.await(5, TimeUnit.SECONDS))
            // Another registration's record cannot be written while the copy is held.
            store.failing = true
            assertThrows(IOException::class.java) { Fixture.start(transfers, Fixture.key("other"), root) }
            // The start the copy belonged to is answered: what became of it is not known.
            Fixture.await("the start's answer") { host.answers.isNotEmpty() }
            assertEquals(Triple(key, false, "UnknownError"), host.answers.single())
            // The copy still has its staging, so the namespace is not entered and no second copy starts.
            assertNull(transfers.enter(Fixture.app))
            assertFalse(transfers.feed(SessionStarted(Fixture.app, 1)).accepted)
            assertEquals(1, copies.get())
        } finally { release.countDown() }
        Fixture.await("the recovery") { transfers.enter(Fixture.app) != null }
        // The copy is made again, and the report of the one cut short is not taken for its own.
        Fixture.await("the second copy") { record(key)!!.phase == Phase.Ready }
        assertEquals(2, copies.get())
        assertNull(record(key)!!.result)
        assertEquals(6L, transfers.read(key)!!.getLong("uploadTotal"))
        assertEquals(setOf("record", "0.upload"), dir(key).list()!!.toSet())
        assertEquals(listOf(key.generation), dir(key).parentFile!!.list()!!.toList())
        assertEquals(1, host.answers.size)
    }

    @Test fun anAnswerToAScheduleFromBeforeAFailedWriteIsNotTakenForALaterOne() {
        make()
        val key = Fixture.key("video")
        Fixture.start(transfers, key, root)
        // A new process asks for its work again, and the platform has not answered.
        val answers = ArrayList<(Boolean) -> Unit>()
        platform.answers = answers
        make()
        assertEquals(1, answers.size)
        store.failing = true
        assertThrows(IOException::class.java) { transfers.feed(JobStarted(key, Run(1, Job.Work), transfers.conditions)) }
        assertNotNull(transfers.enter(Fixture.app))
        assertEquals(2, answers.size)
        // The first answer is to a schedule the failure forgot, though the second carries its token.
        answers[0](false)
        assertEquals(Job.Work to false, record(key)!!.let { it.job to it.parked })
        assertEquals(1, transfers.state().first.asking.size)
        answers[1](true)
        assertTrue(transfers.state().first.asking.isEmpty())
        assertEquals(Job.Work to false, record(key)!!.let { it.job to it.parked })
    }

    @Test fun aFailedWriteSettlesTheAnswersAndTheRunsItsQueueHeld() {
        val waiting = ArrayDeque<Runnable>()
        transfers = Transfers(store, transport::exchange, platform, host, { waiting.add(it) }, Fixture.inline { transfers })
        fun perform() { while (waiting.isNotEmpty()) waiting.removeFirst().run() }
        fun enter() { transfers.enter(Fixture.app); perform(); transfers.feed(SessionStarted(Fixture.app, 1)) }
        enter()
        val key = Fixture.key("video")
        Fixture.start(transfers, key, root, upload = true)
        while (transfers.state().second.firstOrNull() !is Lifecycle.Answer) waiting.removeFirst().run()
        // The start is registered and its answer waits in the queue when another write fails.
        store.failing = true
        assertThrows(IOException::class.java) { Fixture.start(transfers, Fixture.key("other"), root) }
        assertEquals(listOf(Triple(key, true, null)), host.answers.toList())

        enter()
        transfers.feed(JobStarted(key, Run(1, Job.Work), transfers.conditions))
        // A second run of the registration is let go by a Finish that waits in the queue.
        transfers.feed(JobStarted(key, Run(2, Job.Work), transfers.conditions))
        assertTrue(platform.finished.isEmpty())
        store.failing = true
        assertThrows(IOException::class.java) { Fixture.start(transfers, Fixture.key("another"), root) }
        assertEquals(setOf(1 to true, 2 to true), platform.finished.toSet())
        assertEquals(1, host.answers.size)
    }

    @Test fun aNewProcessTakesUpWhatTheLastOneLeftOnDisk() {
        make()
        val key = Fixture.key("video")
        Fixture.start(transfers, key, root, upload = true)
        assertEquals(6L, transfers.read(key)!!.getLong("uploadTotal"))
        File(dir(key).parentFile, ".snapshot-${key.generation}").mkdir()
        transport.snapshot = { fail("the upload is committed") }
        make()
        assertFalse(File(dir(key).parentFile, ".snapshot-${key.generation}").exists())
        assertEquals(listOf(Fixture.app, Fixture.app), platform.acknowledged.toList())
        transfers.feed(JobStarted(key, Run(1, Job.Work), transfers.conditions))
        assertTrue(transport.seen.single { it.getString("op") == "run" }.getString("upload").endsWith("/0.upload"))
        assertEquals("success", transfers.read(key)!!.getString("result"))
    }
}
