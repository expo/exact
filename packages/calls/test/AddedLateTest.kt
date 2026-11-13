package exact.calls

import android.os.Looper
import android.os.ParcelUuid
import android.telecom.DisconnectCause
import androidx.core.telecom.CallControlResult
import androidx.core.telecom.CallControlScope
import androidx.core.telecom.CallEndpointCompat
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import java.util.UUID
import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.util.ReflectionHelpers

/** What happens to a call before Telecom has added it (D9, D11): the list's moves and the notification's buttons are
 * done once its scope exists, and an end frees the call at once. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class AddedLateTest {
    /** Telecom's side of one call: the operations asked of it, in order. */
    private class Scope : CallControlScope {
        val asked = ArrayList<String>()
        override val coroutineContext = EmptyCoroutineContext
        override fun getCallId() = ParcelUuid(UUID(0, 1))
        override suspend fun setActive(): CallControlResult { asked += "setActive"; return CallControlResult.Success() }
        override suspend fun setInactive(): CallControlResult { asked += "setInactive"; return CallControlResult.Success() }
        override suspend fun answer(callType: Int): CallControlResult { asked += "answer"; return CallControlResult.Success() }
        override suspend fun disconnect(disconnectCause: DisconnectCause): CallControlResult {
            asked += "disconnect ${disconnectCause.code}"
            return CallControlResult.Success()
        }
        override suspend fun requestEndpointChange(endpoint: CallEndpointCompat): CallControlResult = CallControlResult.Success()
        override val currentCallEndpoint: Flow<CallEndpointCompat> = emptyFlow()
        override val availableEndpoints: Flow<List<CallEndpointCompat>> = emptyFlow()
        override val isMuted: Flow<Boolean> = emptyFlow()
    }

    private val module = CallsModule(ExtensionContext(RuntimeEnvironment.getApplication(), "calls"))
    private val calls = ReflectionHelpers.getField<LinkedHashMap<String, CallsModule.Live>>(module, "calls")

    private fun entry(state: String) = JSONObject().put("id", "c").put("state", state).put("direction", "incoming")
    private fun ringing() = CallsModule.Live(entry("ringing")).also { calls["c"] = it }
    private fun list(e: JSONObject) {
        module.later("exact:listed", JSONObject().put("entries", JSONArray().put(e))) {}
        shadowOf(Looper.getMainLooper()).idle()
    }

    @Test fun aCallEndedBeforeItWasAddedIsFreedThenDisconnected() {
        val call = ringing()
        list(entry("ended").put("endedReason", "declined"))
        assertTrue("the end frees the call before Telecom adds it", call.ended.isCompleted)
        assertFalse(calls.containsKey("c"))
        val scope = Scope()
        runBlocking { module.added("c", call, scope, true) }
        assertEquals(listOf("disconnect ${DisconnectCause.REJECTED}"), scope.asked)
    }

    @Test fun aCallAnsweredBeforeItWasAddedIsAnsweredOnceAdded() {
        val call = ringing()
        list(entry("connected"))
        val scope = Scope()
        runBlocking { module.added("c", call, scope, true) }
        assertEquals(listOf("answer"), scope.asked)
    }

    @Test fun aNotificationButtonPressedBeforeTheCallWasAddedIsPressedOnceAdded() {
        val call = ringing()
        module.notificationAction("c", "decline")
        val scope = Scope()
        runBlocking { module.added("c", call, scope, true) }
        assertEquals(listOf("disconnect ${DisconnectCause.REJECTED}"), scope.asked)
    }
}
