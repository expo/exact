package exact.geofencing

import dev.exact.canvas.ExactRegions
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [DeliverTest.Host::class])
class DeliverTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = true
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
}
