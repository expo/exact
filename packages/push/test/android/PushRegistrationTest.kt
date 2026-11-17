package exact.push

import android.content.Context
import android.content.ContextWrapper
import android.os.Looper
import com.google.android.gms.common.GoogleApiAvailability
import com.google.android.gms.tasks.Task
import com.google.android.gms.tasks.TaskCompletionSource
import com.google.firebase.FirebaseApp
import com.google.firebase.installations.FirebaseInstallations
import com.google.firebase.messaging.FirebaseMessaging
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadow.api.Shadow
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [30], manifest = Config.NONE, shadows = [PushRegistrationTest.Messaging::class,
    PushRegistrationTest.Installations::class, PushRegistrationTest.Firebase::class,
    PushRegistrationTest.PlayServices::class, PushRegistrationTest.Session::class])
class PushRegistrationTest {
    @Implements(FirebaseMessaging::class)
    class Messaging {
        @Implementation fun getToken(): Task<String> { reads++; return token.task }
        companion object {
            lateinit var token: TaskCompletionSource<String>
            var reads = 0
            @JvmStatic @Implementation fun getInstance(): FirebaseMessaging = Shadow.newInstanceOf(FirebaseMessaging::class.java)
        }
    }
    @Implements(FirebaseInstallations::class)
    class Installations {
        @Implementation fun getId(): Task<String> = id.task
        companion object {
            lateinit var id: TaskCompletionSource<String>
            @JvmStatic @Implementation fun getInstance(): FirebaseInstallations = Shadow.newInstanceOf(FirebaseInstallations::class.java)
        }
    }
    @Implements(FirebaseApp::class)
    class Firebase {
        companion object {
            @JvmStatic @Implementation fun getApps(app: Context): List<FirebaseApp> = listOf(Shadow.newInstanceOf(FirebaseApp::class.java))
        }
    }
    @Implements(GoogleApiAvailability::class)
    class PlayServices {
        @Implementation fun isGooglePlayServicesAvailable(app: Context) = 0
        companion object {
            @JvmStatic @Implementation fun getInstance(): GoogleApiAvailability = Shadow.newInstanceOf(GoogleApiAvailability::class.java)
        }
    }
    @Implements(Native::class, isInAndroidSdk = false)
    class Session {
        companion object {
            val changes = ArrayList<String>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun changed(topic: String) { changes += topic }
        }
    }
    private val app: Context get() = RuntimeEnvironment.getApplication()
    private val main get() = shadowOf(Looper.getMainLooper())
    private lateinit var module: PushModule
    private fun call(name: String, reply: (JSONObject) -> Unit) = module.later(name, JSONObject(), reply)

    @Before fun setup() {
        dev.exact.canvas.Exact.owner = null
        PushModule::class.java.getDeclaredField("asked").apply { isAccessible = true }.setBoolean(null, false)
        app.getSharedPreferences("exact.push", Context.MODE_PRIVATE).edit().clear().commit()
        Messaging.token = TaskCompletionSource(); Messaging.reads = 0
        Installations.id = TaskCompletionSource()
        Session.changes.clear()
        module = PushModule(ExtensionContext(app, "push"))
    }

    @Test fun subscribersShareRegistrationAndRotationsReachTheLiveSource() {
        val replies = ArrayList<JSONObject>()
        call("subscribePush", replies::add)
        call("subscribePush", replies::add)
        assertEquals(1, Messaging.reads)
        Messaging.token.setResult("fcm-token")
        main.idle()
        assertTrue(replies.isEmpty())
        Installations.id.setResult("fid")
        main.idle()
        assertEquals(2, replies.size)
        assertEquals("fcm-token", replies[0].getJSONObject("android").getString("token"))
        assertEquals("fid", replies[0].getJSONObject("android").getString("installationId"))
        call("pushSubscription") { assertEquals("fcm-token", it.getJSONObject("exact:answer").getJSONObject("android").getString("token")) }
        Session.changes.clear()
        Robolectric.buildService(PushService::class.java).get().onNewToken("rotated")
        main.idle()
        assertEquals(listOf("push.pushSubscription"), Session.changes)
        call("pushSubscription") { assertEquals("rotated", it.getJSONObject("exact:answer").getJSONObject("android").getString("token")) }
        Robolectric.buildService(PushService::class.java).get().onNewToken("rotated")
        main.idle()
        assertEquals(1, Session.changes.size)
    }

    @Test fun aTimeoutRefusesAndALateRegistrationCannotOverwriteItsRetry() {
        var first: JSONObject? = null
        call("subscribePush") { first = it }
        val late = Messaging.token
        main.idleFor(Duration.ofMinutes(1))
        assertEquals("AbortError", first!!.getString("error"))
        Messaging.token = TaskCompletionSource()
        var second: JSONObject? = null
        call("subscribePush") { second = it }
        late.setResult("late-token")
        main.idle()
        assertNull(PushModule.kept(app))
        Messaging.token.setResult("new-token")
        Installations.id.setResult("fid")
        main.idle()
        assertEquals("new-token", second!!.getJSONObject("android").getString("token"))
    }

    @Test fun aGrantedReadRegistersAKeptInstallationWithoutAToken() {
        val installed = object : ContextWrapper(app) {
            override fun getPackageName() = "dev.gate_house.dev"
        }
        module = PushModule(ExtensionContext(installed, "push"))
        call("pushSubscription") { assertTrue(it.isNull("exact:answer")) }
        assertEquals(0, Messaging.reads)
        app.getSharedPreferences("exact.push", Context.MODE_PRIVATE).edit().putString("installationId", "fid").commit()
        repeat(2) { call("pushSubscription") { assertTrue(it.isNull("exact:answer")) } }
        assertEquals(1, Messaging.reads)
        Messaging.token.setResult("fcm-token")
        main.idle()
        assertNull(PushModule.kept(app))
        Installations.id.setResult("fid")
        main.idle()
        assertEquals(listOf("push.pushSubscription"), Session.changes)
        call("pushSubscription") {
            val android = it.getJSONObject("exact:answer").getJSONObject("android")
            assertEquals("fid", android.getString("installationId"))
            assertEquals("fcm-token", android.getString("token"))
            assertEquals("dev.gate_house.dev", android.getString("appId"))
        }
        assertEquals(1, Messaging.reads)
    }
}
