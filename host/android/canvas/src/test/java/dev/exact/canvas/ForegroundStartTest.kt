package dev.exact.canvas

import android.app.Activity
import android.content.ComponentName
import android.content.pm.ServiceInfo
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** A foreground service's start follows Android's rule for a start from the foreground: the app has a visible
 * activity. Paused under a dialog it does, as when a permission's answer arrives before the resume; stopped it does
 * not (expo/exact#382). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class ForegroundStartTest {
    private val application = RuntimeEnvironment.getApplication()
    private val services = ForegroundServices(application, "calls") { false }

    @Before fun declare() {
        // The app's lines declare the call service with its type, as the build's manifest does.
        val info = ServiceInfo().apply {
            packageName = application.packageName
            name = ForegroundService.Call::class.java.name
        }
        // `foregroundServiceType` has a getter alone; the manifest's attribute sets the field.
        ServiceInfo::class.java.getDeclaredField("mForegroundServiceType").apply { isAccessible = true }
            .setInt(info, ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL)
        shadowOf(application.packageManager).addOrUpdateService(info)
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        Extensions.front = { activity }
        Extensions.started()
        Extensions.resumed()
    }

    @After fun leave() {
        Claims.held.toList().forEach { it.release() }
        Extensions.paused()
        Extensions.stopped(false)
        Extensions.front = { Exact.reader?.get() }
    }

    private fun started() = shadowOf(application).nextStartedService?.component

    @Test fun aStartWhileTheActivityShowsPausedUnderADialogIsMade() {
        Extensions.paused()
        val claim = services.claim("call", setOf("phoneCall"), "app")
        assertNotNull(claim)
        assertEquals(ComponentName(application, ForegroundService.Call::class.java), started())
    }

    @Test fun aStartOnceTheAppStoppedIsRefused() {
        Extensions.paused()
        Extensions.stopped(false)
        assertNull(services.claim("call", setOf("phoneCall"), "app"))
        assertNull(started())
    }
}
