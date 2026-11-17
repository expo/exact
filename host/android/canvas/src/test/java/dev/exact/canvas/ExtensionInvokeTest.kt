package dev.exact.canvas

import java.util.concurrent.Executors
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadow.api.Shadow

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE,
    shadows = [ExtensionInvokeTest.OwnerThread::class, ExtensionInvokeTest.Host::class])
class ExtensionInvokeTest {
    /** Exact's dispatch runs against an owner thread without starting the native renderer. */
    @Implements(Owner::class, isInAndroidSdk = false)
    class OwnerThread : AutoCloseable {
        private val thread = Executors.newSingleThreadExecutor()
        val queued = ArrayDeque<() -> Unit>()
        @Implementation fun call(f: () -> Unit) { queued.addLast(f) }
        @Implementation fun drainStore(ms: Long) = false
        var publications = 0
            private set
        @Implementation fun __constructor__(hwbuf: Boolean, fast: Boolean, priority: Int, intent: (String) -> String?) {}
        @Implementation fun <T> sync(publish: Boolean, f: () -> T): T = thread.submit<T> {
            while (queued.isNotEmpty()) queued.removeFirst()()
            val answer = f()
            if (publish) publications++
            answer
        }.get()
        override fun close() { thread.shutdownNow() }
    }

    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            private val host = ThreadLocal<String>()
            val visibility = ArrayList<Boolean>()
            val eligibility = ArrayList<Boolean>()
            @JvmStatic @Implementation fun hidden(hidden: Boolean, eligible: Boolean) { visibility.add(hidden); eligibility.add(eligible) }
            var invokeAvailable = true
            var received: String? = null
            var onChanged: (String) -> Unit = {}
            fun start() { host.set("running") }
            fun stop() { host.remove(); received = null }
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun changed(topic: String) = onChanged(topic)
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun appFile(path: String): String? = null
            @JvmStatic @Implementation fun invoke(body: String): Int {
                if (!invokeAvailable) throw UnsatisfiedLinkError("Native.invoke")
                if (host.get() == null) return -1
                received = body
                return 37
            }
        }
    }

    private fun withOwner(run: (Owner, OwnerThread) -> Unit) {
        val owner = Owner(false, false, 0) { null }
        Shadow.extract<OwnerThread>(owner).use { run(owner, it) }
    }

    @Test fun hidingReachesTheHostBeforeReturningEvenWhenTheStoreWaitExpires() {
        withOwner { owner, thread ->
            Exact.owner = owner
            Host.visibility.clear()
            try {
                Exact.hidden(true)
                assertFalse(Exact.drainStore(0))
                assertEquals(listOf(true), Host.visibility)
                assertTrue("no freeze remains queued after stop", thread.queued.isEmpty())
                Exact.hidden(false)
                assertEquals(listOf(true, false), Host.visibility)
                assertEquals(2, thread.publications)
            } finally { Exact.owner = null; Host.visibility.clear() }
        }
    }

    @Test fun aMissingNativeEntryCanBeAnsweredAsNoSessionOnEitherThread() {
        withOwner { owner, _ ->
            Exact.owner = null
            Host.invokeAvailable = false
            try {
                assertThrows(UnsatisfiedLinkError::class.java) { Exact.invoke("main-thread host") }
                Exact.owner = owner
                assertEquals(-1, Exact.invoke("owner-thread host"))
            } finally {
                Host.invokeAvailable = true
                Exact.owner = null
            }
        }
    }

    @Test fun invokeReachesTheOwnerAndPublishesTheHandlersChanges() {
        withOwner { owner, thread ->
            Exact.owner = owner
            try {
                owner.sync(publish = false) { Host.start() }
                assertEquals(-1, Native.invoke("wrong thread"))
                assertEquals(37, Exact.invoke("handler arguments"))
                assertEquals("handler arguments", Host.received)
                assertEquals(1, thread.publications)
            } finally {
                owner.sync(publish = false) { Host.stop() }
                Exact.owner = null
            }
        }
    }

    @Test fun invokeAlsoWorksWhenTheHostRunsOnTheCallingThread() {
        Exact.owner = null
        Host.start()
        var publications = 0
        Exact.turned = { publications++ }
        try {
            assertEquals(37, Exact.invoke("main-thread host"))
            assertEquals(1, publications)
            assertEquals("main-thread host", Host.received)
        } finally { Host.stop(); Exact.turned = {} }
        assertEquals(-1, Exact.invoke("no host"))
    }
}
