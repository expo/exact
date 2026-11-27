package exact.geofencing

import com.google.android.gms.location.Geofence
import com.google.android.gms.location.GeofencingRequest
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class FenceRequestTest {
    private fun region(name: String, loiteringDelay: Long? = null) = JSONObject().put("name", name)
        .put("latitude", 52.0).put("longitude", 4.0).put("radius", 150.0)
        .apply { if (loiteringDelay != null) put("options", JSONObject().put("loiteringDelay", loiteringDelay)) }

    /** A dwell fence listens for no entry, so only `INITIAL_TRIGGER_DWELL` reports one added while inside. */
    @Test fun aDwellFenceAddedWhileInsideReportsTheEntry() {
        val request = GeofencingModule.request(listOf(region("desk", loiteringDelay = 60_000)), true)
        assertEquals(GeofencingRequest.INITIAL_TRIGGER_DWELL, request.initialTrigger and GeofencingRequest.INITIAL_TRIGGER_DWELL)
        assertEquals(Geofence.GEOFENCE_TRANSITION_DWELL or Geofence.GEOFENCE_TRANSITION_EXIT, request.geofences.single().transitionTypes)
    }

    @Test fun aPlainFenceAddedWhileInsideReportsTheEntry() {
        val request = GeofencingModule.request(listOf(region("home")), true)
        assertEquals(GeofencingRequest.INITIAL_TRIGGER_ENTER, request.initialTrigger and GeofencingRequest.INITIAL_TRIGGER_ENTER)
        assertEquals(Geofence.GEOFENCE_TRANSITION_ENTER or Geofence.GEOFENCE_TRANSITION_EXIT, request.geofences.single().transitionTypes)
    }

    /** A re-arm raises nothing for either kind of fence. */
    @Test fun aReArmRaisesNoInitialEvent() {
        assertEquals(0, GeofencingModule.request(listOf(region("home"), region("desk", loiteringDelay = 60_000)), false).initialTrigger)
    }
}
