package exact.geolocation

import android.Manifest
import android.content.Context
import android.location.Location
import android.location.LocationManager
import android.os.Looper
import android.os.SystemClock
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import java.time.Duration
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [WatchTest.Host::class])
class WatchTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun changed(topic: String) { changes++ }
            var changes = 0
        }
    }

    private val app = RuntimeEnvironment.getApplication()
    private val module = GeolocationModule(ExtensionContext(app, "geolocation"))
    private val args = JSONObject().put("android", JSONObject().put("provider", LocationManager.GPS_PROVIDER))

    /** What `watchPosition` answers when the runner reads it now (null: it waits for the next answer). */
    private fun read(args: JSONObject = this.args): JSONObject? {
        var answer: JSONObject? = null
        ReflectionHelpers.callInstanceMethod<Unit>(module, "watch",
            ClassParameter.from(JSONObject::class.java, args),
            ClassParameter.from(Function1::class.java, { a: JSONObject -> answer = a }))
        shadowOf(Looper.getMainLooper()).idle()
        return answer
    }

    private fun fix(latitude: Double) = Location(LocationManager.GPS_PROVIDER).apply {
        this.latitude = latitude; longitude = 2.0; accuracy = 5f; time = System.currentTimeMillis()
        elapsedRealtimeNanos = SystemClock.elapsedRealtimeNanos()
    }

    /** A disabled provider's error replaces the fix a watch held; enabled again, the next fix answers it. */
    @Test fun aDisabledProviderIsAnsweredToAWatchThatHadAFix() {
        shadowOf(app).grantPermissions(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION)
        val manager = shadowOf(app.getSystemService(Context.LOCATION_SERVICE) as LocationManager)
        manager.setProviderEnabled(LocationManager.GPS_PROVIDER, true)
        assertNull("no fix yet", read())
        manager.simulateLocation(fix(48.0))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(48.0, read()!!.getJSONObject("coords").getDouble("latitude"), 0.0)

        val before = Host.changes
        manager.setProviderEnabled(LocationManager.GPS_PROVIDER, false)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue("the watch is announced", Host.changes > before)
        assertEquals("POSITION_UNAVAILABLE", read()!!.getString("error"))

        manager.setProviderEnabled(LocationManager.GPS_PROVIDER, true)
        // Past the request's 30 s interval, which the shadow keeps between two fixes.
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(31))
        manager.simulateLocation(fix(49.0))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(49.0, read()!!.getJSONObject("coords").getDouble("latitude"), 0.0)
    }

    /**
     * No provider to register with (passive under coarse alone) is held like a disabled one: a re-read gets the error
     * without announcing the watch again, which the runner would answer by reading it again, endlessly.
     */
    @Test fun aWatchWithNoProviderHoldsItsError() {
        shadowOf(app).grantPermissions(Manifest.permission.ACCESS_COARSE_LOCATION)
        val passive = JSONObject().put("android", JSONObject().put("provider", LocationManager.PASSIVE_PROVIDER))
        val before = Host.changes
        assertEquals("POSITION_UNAVAILABLE", read(passive)!!.getString("error"))
        assertTrue("announced at most once", Host.changes - before <= 1)
        val once = Host.changes
        assertEquals("POSITION_UNAVAILABLE", read(passive)!!.getString("error"))
        assertEquals("a re-read is not announced", once, Host.changes)

        // Released, the next reader asks afresh: with fine granted since, it registers and waits for a fix.
        module.release("watchPosition")
        shadowOf(app).grantPermissions(Manifest.permission.ACCESS_FINE_LOCATION)
        assertNull("registered, no fix yet", read(passive))
    }

    /** A grant that makes the provider usable drops the held error at resume, and the next read registers. */
    @Test fun aPreciseGrantDropsTheHeldError() {
        shadowOf(app).grantPermissions(Manifest.permission.ACCESS_COARSE_LOCATION)
        val passive = JSONObject().put("android", JSONObject().put("provider", LocationManager.PASSIVE_PROVIDER))
        module.later("geolocationStatus", JSONObject()) {}
        assertEquals("POSITION_UNAVAILABLE", read(passive)!!.getString("error"))

        shadowOf(app).grantPermissions(Manifest.permission.ACCESS_FINE_LOCATION)
        module.resumed()
        assertNull("registered, no fix yet", read(passive))
        assertNotNull(ReflectionHelpers.getField<Any?>(module, "watch"))
    }
}
