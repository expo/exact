package exact.geofencing

import dev.exact.canvas.ExactRegions
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import android.os.Looper
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers
import java.lang.ref.WeakReference

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [DeliverTest.Host::class])
class DeliverTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            /** Whether the module is made under the agent, which registers no wake listener. */
            var agent = true
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = agent
            @JvmStatic @Implementation fun changed(topic: String) {}
        }
    }

    /** A wake that expires leaves its region pending, and the pass goes on to the next. */
    @Test fun anExpiredWakeDeliversTheNextPendingRegion() {
        val app = RuntimeEnvironment.getApplication()
        val context = ExtensionContext(app, "geofencing")
        val module = GeofencingModule(context)
        val store = ExactRegions.store(app)
        for ((name, at) in listOf("a" to 1.0, "b" to 2.0)) {
            store.put(JSONObject().put("name", name).put("latitude", 0.0).put("longitude", 0.0).put("radius", 100.0))
            store.transition(name, true, at)
        }

        ReflectionHelpers.callInstanceMethod<Unit>(module, "deliver")
        assertEquals(setOf("region:a:1"), context.ended.keys)
        context.ended.remove("region:a:1")!!("expired")
        assertEquals("the next name is raised", setOf("region:b:2"), context.ended.keys)
        context.ended.remove("region:b:2")!!("completed")
        assertEquals("the expired head is not raised again in that pass", emptySet<String>(), context.ended.keys)
        assertEquals(listOf("a"), store.pending().map { it.getString("name") })
    }

    /** The store holds the live module's wake listener weakly: a collection must not stop crossings reaching it. */
    @Test fun aCrossingAfterACollectionReachesTheLiveModule() {
        Host.agent = false
        val app = RuntimeEnvironment.getApplication()
        val context = ExtensionContext(app, "geofencing")
        val module = GeofencingModule(context)
        try {
            shadowOf(Looper.getMainLooper()).idle()
            val canary = WeakReference(Any())
            for (i in 0 until 50) {
                if (canary.get() == null) break
                System.gc()
                Thread.sleep(10)
            }
            assertTrue("a collection ran", canary.get() == null)
            val store = ExactRegions.store(app)
            store.put(JSONObject().put("name", "a").put("latitude", 0.0).put("longitude", 0.0).put("radius", 100.0))
            store.transition("a", true, 1.0)
            assertTrue("a session listens", ExactRegions.wake(app))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(setOf("region:a:1"), context.ended.keys)
        } finally {
            Host.agent = true
            module.destroy {}
        }
    }
}
