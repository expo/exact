package exact.localnetwork

import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Looper
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import java.net.InetAddress
import java.util.concurrent.Executor
import java.util.concurrent.TimeUnit
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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE, shadows = [LocalNetworkLifecycleTest.Host::class, LocalNetworkLifecycleTest.ShadowNsd::class])
class LocalNetworkLifecycleTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun extensionChanged(name: String) {}
        }
    }

    @Implements(NsdManager::class)
    class ShadowNsd {
        companion object {
            val discoveries = ArrayList<NsdManager.DiscoveryListener>()
            val stoppedDiscoveries = ArrayList<NsdManager.DiscoveryListener>()
            val infoCallbacks = ArrayList<NsdManager.ServiceInfoCallback>()
            val unregisteredCallbacks = ArrayList<NsdManager.ServiceInfoCallback>()

            fun reset() {
                discoveries.clear()
                stoppedDiscoveries.clear()
                infoCallbacks.clear()
                unregisteredCallbacks.clear()
            }
        }

        @Implementation fun discoverServices(serviceType: String, protocolType: Int, listener: NsdManager.DiscoveryListener) {
            discoveries += listener
        }

        @Implementation fun stopServiceDiscovery(listener: NsdManager.DiscoveryListener) {
            stoppedDiscoveries += listener
        }

        @Implementation fun registerServiceInfoCallback(serviceInfo: NsdServiceInfo, executor: Executor, callback: NsdManager.ServiceInfoCallback) {
            infoCallbacks += callback
        }

        @Implementation fun unregisterServiceInfoCallback(callback: NsdManager.ServiceInfoCallback) {
            unregisteredCallbacks += callback
        }
    }

    private lateinit var module: LocalNetworkModule

    @Before fun setUp() {
        ShadowNsd.reset()
        module = LocalNetworkModule(ExtensionContext(RuntimeEnvironment.getApplication(), "local-network"))
    }

    @After fun tearDown() {
        module.destroy {}
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun serviceInfo(name: String, type: String, ip: String, port: Int): NsdServiceInfo =
        NsdServiceInfo().apply {
            serviceName = name
            serviceType = type
            hostAddresses = listOf(InetAddress.getByName(ip))
            setPort(port)
        }

    @Test fun concurrentResolvesHonorIndependentTimeouts() {
        var shortReply: JSONObject? = null
        var longReply: JSONObject? = null
        module.later("resolveService", JSONObject().put("name", "printer").put("type", "_http._tcp").put("timeout", 50.0)) { shortReply = it }
        module.later("resolveService", JSONObject().put("name", "printer").put("type", "_http._tcp").put("timeout", 5000.0)) { longReply = it }
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, ShadowNsd.infoCallbacks.size)

        shadowOf(Looper.getMainLooper()).idleFor(100, TimeUnit.MILLISECONDS)
        assertEquals("NotFoundError", shortReply?.optString("error"))
        assertNull(longReply)
        assertTrue(ShadowNsd.unregisteredCallbacks.isEmpty())

        ShadowNsd.infoCallbacks.single().onServiceUpdated(serviceInfo("printer", "_http._tcp", "192.0.2.10", 8080))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("192.0.2.10", longReply?.optString("host"))
        assertEquals(8080, longReply?.optInt("port"))
    }

    @Test fun staleTimeoutAfterPauseAndResumeDoesNotAbortFreshResolve() {
        var first: JSONObject? = null
        module.later("resolveService", JSONObject().put("name", "speaker").put("type", "_http._tcp").put("timeout", 50.0)) { first = it }
        shadowOf(Looper.getMainLooper()).idle()
        ShadowNsd.infoCallbacks.last().onServiceUpdated(serviceInfo("speaker", "_http._tcp", "192.0.2.20", 9000))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(9000, first?.optInt("port"))

        module.foreground(false)
        module.foreground(true)
        var second: JSONObject? = null
        module.later("resolveService", JSONObject().put("name", "speaker").put("type", "_http._tcp").put("timeout", 5000.0)) { second = it }
        shadowOf(Looper.getMainLooper()).idleFor(150, TimeUnit.MILLISECONDS)
        assertNull("stale timeout aborted fresh resolve", second)

        ShadowNsd.infoCallbacks.last().onServiceUpdated(serviceInfo("speaker", "_http._tcp", "192.0.2.21", 9001))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(9001, second?.optInt("port"))
    }

    @Test fun staleDiscoveryAndResolveCallbacksAfterStopAreIgnored() {
        var browse1: JSONObject? = null
        module.later("browseServices", JSONObject().put("type", "_http._tcp")) { browse1 = it }
        shadowOf(Looper.getMainLooper()).idle()
        val l1 = ShadowNsd.discoveries.single()

        module.foreground(false)
        module.foreground(true)
        var browse2: JSONObject? = null
        module.later("browseServices", JSONObject().put("type", "_http._tcp")) { browse2 = it }
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(2, ShadowNsd.discoveries.size)
        val l2 = ShadowNsd.discoveries[1]

        // Late callbacks from l1 must not poison names or fail l2.
        l1.onServiceFound(NsdServiceInfo().apply { serviceName = "stale" })
        l1.onStartDiscoveryFailed("_http._tcp", 3)
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse(ShadowNsd.stoppedDiscoveries.contains(l2))
        assertNull(browse2)

        l2.onServiceFound(NsdServiceInfo().apply { serviceName = "fresh" })
        shadowOf(Looper.getMainLooper()).idle()
        val services = browse2!!.getJSONArray("services")
        assertEquals(1, services.length())
        assertEquals("fresh", services.getJSONObject(0).getString("name"))

        // Late resolve callback from a stopped ServiceInfoCallback must not abort a restarted resolve.
        var res1: JSONObject? = null
        module.later("resolveService", JSONObject().put("name", "cam").put("type", "_http._tcp").put("timeout", 5000.0)) { res1 = it }
        shadowOf(Looper.getMainLooper()).idle()
        val c1 = ShadowNsd.infoCallbacks.last()

        module.foreground(false)
        module.foreground(true)
        var res2: JSONObject? = null
        module.later("resolveService", JSONObject().put("name", "cam").put("type", "_http._tcp").put("timeout", 5000.0)) { res2 = it }
        shadowOf(Looper.getMainLooper()).idle()
        val c2 = ShadowNsd.infoCallbacks.last()
        assertNotSame(c1, c2)

        c1.onServiceLost()
        c1.onServiceInfoCallbackRegistrationFailed(4)
        shadowOf(Looper.getMainLooper()).idle()
        assertNull("stale callback aborted replacement resolve", res2)
        assertFalse(ShadowNsd.unregisteredCallbacks.contains(c2))

        c2.onServiceUpdated(serviceInfo("cam", "_http._tcp", "192.0.2.30", 8443))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(8443, res2?.optInt("port"))
        assertEquals("fresh", browse1!!.getJSONArray("services").getJSONObject(0).getString("name"))
        assertEquals(8443, res1?.optInt("port"))
    }
}
