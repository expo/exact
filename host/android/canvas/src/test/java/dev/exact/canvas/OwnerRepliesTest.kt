package dev.exact.canvas

import android.os.Looper
import dev.exact.canvas.OwnerReplies.Handed
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [OwnerRepliesTest.Host::class])
class OwnerRepliesTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            /** What reached exact2 by a posted turn rather than an agent wait's. */
            internal val posted = java.util.Collections.synchronizedList(ArrayList<Handed>())
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun answer(id: Int, answer: String?) { posted += Handed.Answer(id, answer) }
            @JvmStatic @Implementation fun pictureChanged(source: String, revoked: Boolean) { posted += Handed.Picture(source, revoked) }
            @JvmStatic @Implementation fun journal(line: String) { posted += Handed.Journal(line) }
            @JvmStatic @Implementation fun presence(answer: String) { posted += Handed.Changed("presence $answer") }
        }
    }

    @Test fun anOrdinaryReplyDoesNotOvertakeAnEarlierInput() {
        val handler = ArrayDeque<() -> Unit>()
        var price = 0
        var selectedPrice = 0
        val replies = OwnerReplies({ handler.addLast(it) }) { price = (it as Handed.Answer).value!!.toInt() }
        replies.hand(Handed.Answer(1, "10"))
        handler.addLast { selectedPrice = price }
        replies.hand(Handed.Answer(2, "20"))

        while (handler.isNotEmpty()) handler.removeFirst().invoke()

        assertEquals(10, selectedPrice)
        assertEquals(20, price)
        assertTrue(replies.take().isEmpty())
    }

    @Test fun anAgentWaitTakesOnlyWhatTheReaderHandedTheOwnerBeforeItsHandlerCanRun() {
        val owner = Executors.newSingleThreadExecutor()
        val handler = LinkedBlockingQueue<() -> Unit>()
        val waiting = CountDownLatch(1)
        val delivered = CountDownLatch(1)
        val applied = ArrayList<Handed>()
        var ownerThread: Thread? = null
        var unrelated = false
        val replies = OwnerReplies({ handler.add(it) }) {
            assertSame(ownerThread, Thread.currentThread())
            applied.add(it)
        }
        // What an extension did before its answer is taken before that answer, in the order it did it.
        val handed = listOf(
            Handed.Answer(1, "first"), Handed.Picture("photos:agent-1", true), Handed.Journal("photos: revoked"),
            Handed.Changed("photos.photoAssets"), Handed.Answer(2, null),
        )
        try {
            val clock = owner.submit {
                ownerThread = Thread.currentThread()
                waiting.countDown()
                assertTrue(delivered.await(10, TimeUnit.SECONDS))
                assertTrue(applied.isEmpty())
                for (h in replies.take()) replies.deliver(h)
                assertEquals(handed, applied)
                assertFalse("a clock wait must not dispatch input or a frame", unrelated)
            }
            assertTrue(waiting.await(10, TimeUnit.SECONDS))
            handler.add { unrelated = true }
            handed.forEach(replies::hand)
            delivered.countDown()
            clock.get(10, TimeUnit.SECONDS)
            owner.submit {
                while (true) (handler.poll() ?: break).invoke()
                assertTrue(unrelated)
                assertEquals("the queued wake must not deliver anything twice", handed.size, applied.size)
                replies.hand(Handed.Answer(3, "idle"))
                handler.remove().invoke()
                assertEquals(handed + Handed.Answer(3, "idle"), applied)
                assertTrue(replies.take().isEmpty())
            }.get(10, TimeUnit.SECONDS)
        } finally {
            delivered.countDown()
            owner.shutdownNow()
        }
    }

    /** A module call ([Requests.MODULE]) or presence check ([Requests.PRESENCE]) as exact2 writes it. */
    private fun request(kind: Int, id: Int, body: String): IntArray {
        val bytes = body.toByteArray(Charsets.UTF_8)
        val words = IntArray((bytes.size + 3) / 4) { w -> (0 until 4).sumOf { k -> ((bytes.getOrNull(w * 4 + k)?.toInt() ?: 0) and 255) shl (8 * k) } }
        return intArrayOf(kind, id, 1 + words.size, bytes.size) + words
    }

    /** With no activity the reader still answers: a tell, a source's read and a presence check each go to the
     * extensions, which answer by the owner's queue, after the picture and the journal line handed before them. An
     * unanswered call would hold every later `clock settle` for its whole bound. */
    @Test fun aDetachedReaderAnswersByTheOwnersQueueAfterWhatWasHandedBeforeIt() {
        Host.posted.clear()
        val owner = Owner(false, false, 0) { "0" }
        val thread = Owner::class.java.getDeclaredField("thread").apply { isAccessible = true }
            .get(owner) as android.os.HandlerThread
        val agent = Executors.newSingleThreadExecutor()
        Exact.owner = owner
        try {
            val requested = CountDownLatch(1)
            val handed = CountDownLatch(1)
            // An agent operation holds the owner: the requests go to main, and its next turn takes what came back.
            val turn = agent.submit<String> {
                owner.ask {
                    assertEquals("[]", owner.agentTurn(
                        request(Requests.MODULE, 5, """{"extension":"photos","preferred":{}}""") +
                            request(Requests.MODULE, 6, """{"extension":"photos","source":"photoAssets","args":{}}""") +
                            request(Requests.PRESENCE, 7, """{"call":1,"origin":"app"}""")))
                    requested.countDown()
                    assertTrue(handed.await(10, TimeUnit.SECONDS))
                    owner.agentTurn(null)
                }
            }
            assertTrue(requested.await(10, TimeUnit.SECONDS))
            Exact.pictureChanged("photos:agent-1", true)
            Exact.journal("photos: revoked photos:agent-1")
            shadowOf(Looper.getMainLooper()).idle()
            handed.countDown()
            val unsupported = org.json.JSONObject().put("error", "NotSupportedError").toString()
            assertEquals(
                org.json.JSONArray()
                    .put(org.json.JSONObject().put("picture", "photos:agent-1").put("revoked", true))
                    .put(org.json.JSONObject().put("journal", "photos: revoked photos:agent-1"))
                    .put(org.json.JSONArray().put(5).put(unsupported))
                    .put(org.json.JSONArray().put(6).put(unsupported))
                    .toString(),
                turn.get(10, TimeUnit.SECONDS),
            )
            owner.ask {}
            // The presence check answers by its own door, posted; the rest were the agent's, delivered once.
            assertEquals(1, Host.posted.size)
            assertTrue(Host.posted.single().toString(), (Host.posted.single() as Handed.Changed).topic.contains("NotSupportedError"))
        } finally {
            Exact.owner = null
            agent.shutdownNow()
            thread.quitSafely(); thread.join(5000)
        }
    }
}
