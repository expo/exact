package exact.backgroundtransfer

import exact.backgroundtransfer.Lifecycle.Delete
import exact.backgroundtransfer.Lifecycle.Durable
import exact.backgroundtransfer.Lifecycle.Intent
import exact.backgroundtransfer.Lifecycle.Job
import exact.backgroundtransfer.Lifecycle.Key
import exact.backgroundtransfer.Lifecycle.Namespace
import exact.backgroundtransfer.Lifecycle.Phase
import exact.backgroundtransfer.Lifecycle.Put
import exact.backgroundtransfer.Lifecycle.Record
import exact.backgroundtransfer.Lifecycle.Result
import exact.backgroundtransfer.Lifecycle.Stopping
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.File
import java.nio.file.Files
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], manifest = Config.NONE)
class RegistrationsTest {
    private lateinit var root: File
    private lateinit var store: Registrations

    @Before fun setup() {
        root = Files.createTempDirectory("transfer-registrations-").toFile()
        store = Registrations(root)
    }
    @After fun cleanup() { root.deleteRecursively() }

    private fun dir(key: Key) = File(root, "${if (key.namespace.agent) "agent" else "app"}.${key.namespace.name}/${key.generation}")
    private fun create(id: String = "video", requests: Int = 1, total: Long? = null, namespace: Namespace = Fixture.app,
                       saveTo: String? = null, upload: Boolean = false): Record {
        val record = Record(Fixture.key(id, namespace), Intent.Ordinary, requests)
        store.write(Put(record), Registrations.Admission("Upload video", "net.fetch https://example.test", "app",
            List(requests) { Registrations.Request("https://example.test/$it", "POST", saveTo, if (upload && it == 0) "app:/data/video" else null) },
            total, JSONObject().put("data", "/d").put("cache", "/c").put("temporary", "/t")))
        return record
    }
    /** Keep the response of [record]'s next request as attempt [number] would. */
    private fun respond(record: Record, number: Int, text: String = "response", status: Int = 200): Record {
        val attempt = store.begin(record.key, number, record.done)
        attempt.response.writeText(text)
        return record.copy(done = record.done + 1).also { store.write(Put(it), response = Registrations.Response(attempt, status, "text/plain")) }
    }

    @Test fun aRecordReadsBackAsTheLifecycleWroteIt() {
        val drive = Namespace("0123456789abcdef", agent = true)
        val first = create()
        val other = create("other", namespace = drive)
        val written = first.copy(intent = Intent.Ordinary, phase = Phase.Ended, job = Job.Work, parked = true, attempt = 4, stops = 2,
            retries = 3, result = Result.BadStatus, placed = true, wakes = 2, delivery = 5)
        store.write(Put(written))
        store.write(Stopping(setOf(Fixture.app, drive)))
        assertEquals(Durable(mapOf(first.key to written, other.key to other), setOf(Fixture.app, drive)), Registrations(root).durable())
        assertTrue(File(root, "agent.0123456789abcdef/${other.key.generation}/record").isFile)
        store.write(Delete(first.key))
        store.write(Stopping(emptySet()))
        assertEquals(Durable(mapOf(other.key to other)), Registrations(root).durable())
        assertFalse(dir(first.key).exists())
    }

    @Test fun aResponseIsKeptByTheWriteThatCountsIt() {
        val record = create(requests = 2)
        val attempt = store.begin(record.key, 1, 0)
        attempt.response.writeText("response")
        // The count without its response is refused, and nothing changes.
        assertThrows(IllegalArgumentException::class.java) { store.write(Put(record.copy(done = 1))) }
        assertEquals(record, store.durable().records[record.key])
        store.write(Put(record.copy(done = 1)), response = Registrations.Response(attempt, 201, "text/plain"))
        assertFalse(attempt.response.exists())
        assertEquals("response", File(dir(record.key), "0.response").readText())
        val kept = store.get(record.key)!!
        assertEquals(8L, kept.getLong("downloaded"))
        assertEquals(201, kept.getJSONArray("records").getJSONObject(0).getInt("status"))
        assertEquals(1, Registrations(root).durable().records[record.key]!!.done)
    }

    @Test fun aNewAttemptNeverResumesAPartialResponse() {
        var record = create(requests = 2, total = 20)
        record = respond(record, 1)
        val partial = store.begin(record.key, 2, 1)
        partial.response.writeText("part")
        assertEquals(12L, partial.remaining)
        val next = store.begin(record.key, 3, 1)
        assertFalse(partial.response.exists())
        assertEquals(0L, next.response.length())
        assertEquals("response", File(dir(record.key), "0.response").readText())
    }

    @Test fun aResponsePastTheDownloadTotalExceedsIt() {
        val record = create(total = 4)
        val attempt = store.begin(record.key, 1, 0)
        attempt.response.writeText("four")
        assertFalse(store.exceeds(attempt))
        attempt.response.writeText("fives")
        assertTrue(store.exceeds(attempt))
    }

    @Test fun onlyTheLeaseHolderWritesProgress() {
        val record = create().copy(phase = Phase.Sending, job = Job.Work, attempt = 2)
        store.write(Put(record))
        val stale = store.begin(record.key, 1, 0)
        val lease = store.begin(record.key, 2, 0)
        assertNull(store.progress(stale, 5, 5))
        assertEquals(0L, Registrations.publicRecord(store.get(record.key)!!).getLong("downloaded"))
        assertNotNull(store.progress(lease, 5, 7))
        val public = Registrations.publicRecord(store.get(record.key)!!)
        assertEquals(5L, public.getLong("uploaded"))
        assertEquals(7L, public.getLong("downloaded"))
        store.write(Put(record.copy(phase = Phase.Ready)))
        assertNull(store.progress(lease, 9, 9))
    }

    @Test fun anUploadIsCopiedOutsideTheMonitorAndCommittedOnce() {
        val record = create(upload = true)
        val reader = Executors.newSingleThreadExecutor()
        try {
            assertTrue(store.snapshot(record.key, { false }) { kept, path, file ->
                assertEquals("app:/data/video", path)
                assertEquals("net.fetch https://example.test", kept.getString("source"))
                // A reader is not kept waiting by the copy.
                assertNotNull(reader.submit<JSONObject?> { store.get(record.key) }.get(2, TimeUnit.SECONDS))
                file.writeText("upload")
            })
        } finally { reader.shutdownNow() }
        assertEquals(6L, store.get(record.key)!!.getLong("uploadTotal"))
        assertEquals("upload", store.begin(record.key, 1, 0).upload!!.readText())
        // A body a dead process committed is not copied again.
        assertTrue(Registrations(root).snapshot(record.key, { false }) { _, _, _ -> fail("the body is committed") })
        assertEquals(6L, store.get(record.key)!!.getLong("uploadTotal"))
    }

    @Test fun aCancelledOrFailedCopyCommitsNothing() {
        val record = create(upload = true)
        var cancelled = false
        assertFalse(store.snapshot(record.key, { cancelled }) { _, _, file -> file.writeText("upload"); cancelled = true })
        assertThrows(java.io.IOException::class.java) { store.snapshot(record.key, { false }) { _, _, _ -> throw java.io.IOException() } }
        assertEquals(listOf("record"), dir(record.key).list()!!.toList())
        assertEquals(listOf(record.key.generation), dir(record.key).parentFile!!.list()!!.toList())
        assertEquals(0L, store.get(record.key)!!.getLong("uploadTotal"))
    }

    @Test fun aSweepRemovesTheStagingADeadProcessLeft() {
        val record = create()
        val space = dir(record.key).parentFile!!
        File(space, ".start-${Fixture.key("lost").generation}").also { it.mkdir(); File(it, "record").writeText("{}") }
        File(space, ".snapshot-${record.key.generation}").also { it.mkdir(); File(it, "0.upload").writeText("interrupted") }
        assertEquals(setOf(record.key), Registrations(root).durable().records.keys)
        Registrations(root).sweep()
        assertEquals(listOf(record.key.generation), space.list()!!.toList())
    }

    @Test fun aFailedPlacementIsTakenUpWhereItStopped() {
        var record = create(requests = 3)
        for (number in 1..3) record = respond(record, number, "response $number")
        val published = ArrayList<String>()
        assertThrows(java.io.IOException::class.java) {
            store.place(record.key) { _, index, file, path, kept ->
                if (index == 1) throw java.io.IOException("disk full")
                assertTrue(kept)
                published += "$path=${file.readText()}"
            }
        }
        assertEquals(listOf("app:/data/transfers/video/0=response 1"), published)
        val half = store.get(record.key)!!.getJSONArray("records")
        assertTrue(half.getJSONObject(0).getBoolean("placed"))
        // The rename may have committed before the failure: the next placement is told to look.
        assertTrue(half.getJSONObject(1).getBoolean("placing"))
        assertNull(Registrations.publicRecord(store.get(record.key)!!).getJSONArray("records").getJSONObject(1).opt("path").takeIf { it != JSONObject.NULL })
        store.place(record.key) { _, index, file, path, _ ->
            if (index == 2) throw Registrations.BodyMissing()
            published += "$path=${file.readText()}"
        }
        assertEquals("app:/data/transfers/video/1=response 2", published.single { it.contains("/1=") })
        assertEquals(2, published.size)
        val public = Registrations.publicRecord(store.get(record.key)!!).getJSONArray("records")
        assertEquals("app:/data/transfers/video/1", public.getJSONObject(1).getString("path"))
        assertTrue(public.getJSONObject(2).getBoolean("bodyLost"))
        assertTrue(public.getJSONObject(2).isNull("path"))
        store.place(record.key) { _, _, _, _, _ -> fail("every response is placed") }
    }

    @Test fun aKeptResponseIsPublishedUnderItsIdAsOnePathSegment() {
        assertEquals("app:/data/transfers/bgt-1_x", Registrations.kept("bgt-1_x"))
        // What Apple's module makes of the same ids.
        assertEquals("app:/data/transfers/%C3%A9%2Fa%2Eb", Registrations.kept("é/a.b"))
        assertEquals("app:/data/transfers/%2E%2E", Registrations.kept(".."))
        assertEquals("app:/data/transfers/a%20b%25c%E4%B8%AD", Registrations.kept("a b%c中"))
        val record = respond(create("../escape"), 1)
        store.place(record.key) { _, _, _, path, kept ->
            assertEquals("app:/data/transfers/%2E%2E%2Fescape/0", path)
            assertTrue(kept)
        }
    }

    @Test fun aResponseWithASaveToIsPublishedThere() {
        val record = respond(create(saveTo = "app:/data/saved"), 1)
        store.place(record.key) { _, _, _, path, kept ->
            assertEquals("app:/data/saved", path)
            assertFalse(kept)
        }
        assertEquals("app:/data/saved", Registrations.publicRecord(store.get(record.key)!!).getJSONArray("records").getJSONObject(0).getString("path"))
    }

    @Test fun aDeleteInterruptedInsideItsRemovalLeavesNothingTheNextProcessReads() {
        class Death : RuntimeException()
        val record = create(upload = true)
        val other = create("other")
        store.snapshot(record.key, { false }) { _, _, file -> file.writeText("upload") }
        // The process dies when the record's file is unlinked and its directory is not.
        val dying = object : Registrations(root) {
            override fun remove(file: File) { super.remove(file); if (file.name == "record") throw Death() }
        }
        assertThrows(Death::class.java) { dying.write(Delete(record.key)) }
        assertFalse(dir(record.key).exists())
        val next = Registrations(root)
        assertEquals(setOf(other.key), next.durable().records.keys)
        next.sweep()
        assertEquals(listOf(other.key.generation), dir(other.key).parentFile!!.list()!!.toList())
    }

    @Test fun aCleanupLeavesTheRecordForItsDelete() {
        val record = create(requests = 2, upload = true)
        store.snapshot(record.key, { false }) { _, _, file -> file.writeText("upload") }
        respond(record, 1)
        val forgotten = ArrayList<String>()
        // A failure keeps everything for the next recovery's cleanup.
        assertThrows(IllegalStateException::class.java) { store.clean(record.key, {}) { _, _ -> error("the secret store is locked") } }
        assertTrue(File(dir(record.key), "0.upload").exists())
        store.clean(record.key, {}) { _, name -> forgotten += name }
        assertEquals(List(2) { "exact.transfer.${record.key.generation}.$it" }, forgotten)
        assertEquals(listOf("record"), dir(record.key).list()!!.toList())
        assertEquals(record.copy(done = 1), Registrations(root).durable().records[record.key])
        store.write(Delete(record.key))
        assertFalse(dir(record.key).exists())
    }

    @Test fun publicAnswersContainNeitherPrivatePathsNorRetryAuthority() {
        val record = respond(create(), 1, status = 404).copy(phase = Phase.Ended, result = Result.BadStatus)
        store.write(Put(record))
        store.place(record.key) { _, _, _, _, _ -> }
        val public = Registrations.publicRecord(store.get(record.key)!!)
        assertEquals("app:/data/transfers/video/0", public.getJSONArray("records").getJSONObject(0).getString("path"))
        assertEquals(setOf("id", "uploadTotal", "uploaded", "downloadTotal", "downloaded", "result", "failureReason", "apple", "records"),
            public.keys().asSequence().toSet())
        assertEquals("failure", public.getString("result"))
        assertEquals("bad-status", public.getString("failureReason"))
        assertEquals(setOf("index", "url", "status", "contentType", "path", "bodyLost"),
            public.getJSONArray("records").getJSONObject(0).keys().asSequence().toSet())
        for (private in listOf(root.path, "credentialKey", "net.fetch", record.key.generation)) assertFalse(public.toString().contains(private))
        for ((result, reason) in listOf(Result.Success to "", Result.Aborted to "aborted", Result.FetchError to "fetch-error",
            Result.DownloadTotalExceeded to "download-total-exceeded")) {
            store.write(Put(record.copy(result = result)))
            val answer = Registrations.publicRecord(store.get(record.key)!!)
            assertEquals(if (result == Result.Success) "success" else "failure", answer.getString("result"))
            assertEquals(reason, answer.getString("failureReason"))
        }
    }

    @Test fun aResponseThatIsASymbolicLinkIsRefusedAndNeverFollowed() {
        val record = create()
        val attempt = store.begin(record.key, 1, 0)
        val outside = File(root, "outside").also { it.writeText("preserved") }
        attempt.response.delete()
        Files.createSymbolicLink(attempt.response.toPath(), outside.toPath())
        assertThrows(IllegalArgumentException::class.java) {
            store.write(Put(record.copy(done = 1)), response = Registrations.Response(attempt, 200, "text/plain"))
        }
        store.clean(record.key, {}) { _, _ -> }
        store.write(Delete(record.key))
        assertEquals("preserved", outside.readText())
    }

    @Test fun platformAliasesAboveTheRootAreAllowedButTheRootIsNoLink() {
        val trusted = File(root, "trusted").also { it.mkdir() }
        val alias = File(root, "alias")
        Files.createSymbolicLink(alias.toPath(), trusted.toPath())
        val through = Registrations(File(alias, "private"))
        val record = Record(Fixture.key("alias"), Intent.Ordinary, 1)
        through.write(Put(record), Registrations.Admission("", "", "app", listOf(Registrations.Request("https://example.test/a", "GET")), null,
            JSONObject().put("data", "/d").put("cache", "/c").put("temporary", "/t")))
        assertNotNull(through.begin(record.key, 1, 0))
        val link = File(trusted, "linked")
        Files.createSymbolicLink(link.toPath(), File(trusted, "private").toPath())
        assertThrows(IllegalArgumentException::class.java) { Registrations(link) }
    }

    @Test fun onlyStorageNamesBecomeDirectories() {
        for (key in listOf(Key(Namespace("../app"), "video", Fixture.key("video").generation), Key(Fixture.app, "video", "../escape"),
            Key(Fixture.app, "video", "E02C61AB-F992-4DDC-83ED-3CD08F0BBB01"))) {
            assertThrows(IllegalArgumentException::class.java) { store.get(key) }
        }
        assertNull(store.get(Fixture.key("absent")))
        assertEquals(listOf<String>(), root.list()!!.toList())
    }
}
