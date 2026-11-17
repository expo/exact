package exact.backgroundtransfer

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], manifest = Config.NONE)
class TransferIOTest {
    @Test fun aCancellationBeforeTheRunReachesTheHandle() {
        val seen = ArrayList<String>()
        val io = TransferIO { request ->
            seen += request.getString("op")
            if (request.getString("op") != "open") assertEquals(7L, request.getLong("handle"))
            JSONObject().put("handle", 7)
        }
        io.cancel()
        assertEquals(listOf("open", "cancel"), seen)
    }

    @Test fun aCancellationInterruptsTheBlockingCall() {
        val entered = CountDownLatch(1)
        val cancelled = CountDownLatch(1)
        val closed = CountDownLatch(1)
        val io = TransferIO { request ->
            when (request.getString("op")) {
                "open" -> JSONObject().put("handle", 7)
                "run" -> {
                    assertEquals(7L, request.getLong("handle")); entered.countDown()
                    assertTrue(cancelled.await(2, TimeUnit.SECONDS))
                    JSONObject().put("failure", "cancelled")
                }
                "cancel" -> { cancelled.countDown(); JSONObject() }
                "close" -> { closed.countDown(); JSONObject() }
                else -> error("unexpected operation")
            }
        }
        val worker = Executors.newSingleThreadExecutor()
        try {
            val result = worker.submit<JSONObject> { io.run(JSONObject()) }
            assertTrue(entered.await(2, TimeUnit.SECONDS))
            io.cancel()
            assertEquals("cancelled", result.get(2, TimeUnit.SECONDS).getString("failure"))
            assertEquals(1L, closed.count)
        } finally { worker.shutdownNow() }
    }

    @Test fun aClosedHandleRunsNothing() {
        val seen = ArrayList<String>()
        val io = TransferIO { request -> seen += request.getString("op"); JSONObject().put("handle", 7) }
        io.close()
        io.close()
        io.cancel()
        assertNull(io.progress())
        assertEquals("cancelled", io.run(JSONObject()).getString("failure"))
        assertEquals(listOf("open", "close"), seen)
    }
}
