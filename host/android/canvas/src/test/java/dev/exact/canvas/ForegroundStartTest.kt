package dev.exact.canvas

import android.app.Activity
import android.app.Notification
import android.app.Service
import android.content.ComponentName
import android.content.pm.ServiceInfo
import android.os.Looper
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
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadow.api.Shadow
import org.robolectric.shadows.ShadowService

/** A foreground service's start follows Android's rule for a start from the foreground: the app has a visible
 * activity. Paused under a dialog it does, as when a permission's answer arrives before the resume; stopped it does
 * not (expo/exact#382). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ExtensionInvokeTest.Host::class, ForegroundStartTest.RecordingService::class])
class ForegroundStartTest {
    @Implements(Service::class)
    class RecordingService : ShadowService() {
        val startedTypes = ArrayList<Int>()
        val stoppedIds = ArrayList<Int>()

        @Implementation override fun startForeground(id: Int, notification: Notification, foregroundServiceType: Int) {
            startedTypes += foregroundServiceType
            super.startForeground(id, notification, foregroundServiceType)
        }

        @Implementation override fun stopSelf(startId: Int) {
            stoppedIds += startId
            super.stopSelf(startId)
        }
    }
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
        Exact.hidden(false)
        Claims.held.toList().forEach { it.release() }
        Extensions.paused()
        Extensions.stopped(false)
        Extensions.front = { Exact.reader?.get() }
    }

    private fun started() = shadowOf(application).nextStartedService?.component

    @Test fun promotedClaimsKeepExecutionUntilTheLastOneEndsWhileHidden() {
        val first = services.claim("call", setOf("phoneCall"), "app")!!
        val second = services.claim("call", setOf("phoneCall"), "app")!!
        Exact.hidden(true)
        assertFalse("a requested service is not yet promoted", Exact.executionEligible())
        val service = Robolectric.buildService(ForegroundService.Call::class.java).create().get()
        try {
            ExtensionInvokeTest.Host.eligibility.clear()
            Claims.started(service)
            assertTrue(Exact.executionEligible())
            assertEquals(listOf(true), ExtensionInvokeTest.Host.eligibility)
            Exact.hidden(false)
            Exact.hidden(true)
            assertTrue("a promoted service keeps execution when the Activity stops", Exact.executionEligible())
            first.release()
            assertTrue(Exact.executionEligible())
            second.release()
            assertFalse(Exact.executionEligible())
            assertEquals(false, ExtensionInvokeTest.Host.eligibility.last())
            assertEquals(true, ExtensionInvokeTest.Host.visibility.last())
        } finally { service.onDestroy() }
    }

    @Test fun playbackPromotionKeepsExecutionWhileTheActivityIsStopped() {
        Exact.hidden(true)
        assertFalse(Exact.executionEligible())
        try {
            ForegroundServices.playbackPromoted(true)
            assertTrue(Exact.executionEligible())
            assertTrue(ExtensionInvokeTest.Host.eligibility.last())
        } finally {
            ForegroundServices.playbackPromoted(false)
        }
        assertFalse(Exact.executionEligible())
        assertFalse(ExtensionInvokeTest.Host.eligibility.last())
    }

    @Test fun aFailedPromotionCannotKeepExecutionAfterTheActivityStops() {
        var lost = false
        val claim = services.claim("call", setOf("phoneCall"), "app",
            notification = { throw SecurityException("promotion refused") }, lost = { lost = true })!!
        val service = Robolectric.buildService(ForegroundService.Call::class.java).create().get()
        try {
            Claims.started(service)
            assertTrue(lost)
            assertTrue(claim.released)
            Exact.hidden(true)
            assertFalse(Exact.executionEligible())
            assertEquals(false, ExtensionInvokeTest.Host.eligibility.last())
        } finally { service.onDestroy() }
    }

    @Test fun aStartWhileTheActivityShowsPausedUnderADialogIsMade() {
        Extensions.paused()
        val claim = services.claim("call", setOf("phoneCall"), "app")
        assertNotNull(claim)
        assertEquals(ComponentName(application, ForegroundService.Call::class.java), started())
    }

    private fun declareCapture(mask: Int) {
        val info = ServiceInfo().apply {
            packageName = application.packageName
            name = ForegroundService.Capture::class.java.name
        }
        ServiceInfo::class.java.getDeclaredField("mForegroundServiceType").apply { isAccessible = true }
            .setInt(info, mask)
        shadowOf(application.packageManager).addOrUpdateService(info)
        shadowOf(application).grantPermissions(android.Manifest.permission.RECORD_AUDIO)
    }

    private fun recorded(service: Service) = Shadow.extract<RecordingService>(service)

    @Test fun aStartOnceTheAppStoppedIsRefused() {
        Extensions.paused()
        Extensions.stopped(false)
        assertNull(services.claim("call", setOf("phoneCall"), "app"))
        assertNull(started())
    }

    @Test fun unexpectedServiceDestroyReleasesClaimsAndAllowsRestart() {
        var lostReason: String? = null
        val first = services.claim("call", setOf("phoneCall"), "app", lost = { lostReason = it })!!
        val startIntent = shadowOf(application).nextStartedService!!
        val service = Robolectric.buildService(ForegroundService.Call::class.java).create().get()
        service.onStartCommand(startIntent, 0, 1)
        Exact.hidden(true)
        assertTrue(Exact.executionEligible())

        service.onDestroy()
        assertEquals("service destroyed", lostReason)
        assertTrue(first.released)
        assertTrue(Claims.held.none { it.kind == "call" })
        assertFalse(Exact.executionEligible())

        val second = services.claim("call", setOf("phoneCall"), "app")
        assertNotNull(second)
        assertEquals(ComponentName(application, ForegroundService.Call::class.java), started())
        second!!.release()
    }

    @Test fun earlyReleaseBeforeOnStartCommandPromotesWithPermittedTypeAndStops() {
        declareCapture(ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
        val capture = ForegroundServices(application, "media-capture") { false }
        val claim = capture.claim("capture", setOf("microphone"), "app")!!
        val startIntent = shadowOf(application).nextStartedService!!
        assertEquals(ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE, startIntent.getIntExtra(Claims.EXTRA_TYPES, 0))
        claim.release()

        val service = Robolectric.buildService(ForegroundService.Capture::class.java).create().get()
        try {
            service.onStartCommand(startIntent, 0, 7)
            assertEquals(listOf(ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE), recorded(service).startedTypes)
            assertEquals(listOf(7), recorded(service).stoppedIds)
            assertTrue(shadowOf(service).isForegroundStopped)
        } finally { service.onDestroy() }

        // Direct Claims.started without an intent also falls back to the last permitted mask, never 0.
        val fallback = Robolectric.buildService(ForegroundService.Capture::class.java).create().get()
        try {
            Claims.started(fallback)
            assertEquals(listOf(ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE), recorded(fallback).startedTypes)
            assertTrue(shadowOf(fallback).isForegroundStopped)
        } finally { fallback.onDestroy() }
    }

    @Test fun aNewClaimAfterReleaseAndBeforeOnDestroyDoesNotPromoteTheStoppingService() {
        declareCapture(ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
        val capture = ForegroundServices(application, "media-capture") { false }
        val first = capture.claim("capture", setOf("microphone"), "app")!!
        val firstIntent = shadowOf(application).nextStartedService!!
        val stopping = Robolectric.buildService(ForegroundService.Capture::class.java).create().get()
        stopping.onStartCommand(firstIntent, 0, 1)
        recorded(stopping).startedTypes.clear()

        first.release()
        assertEquals(listOf(1), recorded(stopping).stoppedIds)

        var promotedSecond = false
        var promotedThird = false
        val second = capture.claim("capture", setOf("microphone"), "app", promoted = { promotedSecond = true })!!
        val secondIntent = shadowOf(application).nextStartedService!!
        second.update {}
        val third = capture.claim("capture", setOf("microphone"), "app", promoted = { promotedThird = true })!!
        shadowOf(Looper.getMainLooper()).idle()

        assertTrue("the stopping service must not be promoted again", recorded(stopping).startedTypes.isEmpty())
        assertFalse(promotedSecond)
        assertFalse(promotedThird)
        Exact.hidden(true)
        assertFalse(Exact.executionEligible())

        // Destroying the already-stopped service leaves the new claims intact.
        stopping.onDestroy()
        assertFalse(second.released)
        assertFalse(third.released)

        val next = Robolectric.buildService(ForegroundService.Capture::class.java).create().get()
        try {
            next.onStartCommand(secondIntent, 0, 1)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(listOf(ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE), recorded(next).startedTypes)
            assertTrue(promotedSecond)
            assertTrue(promotedThird)
            assertTrue(Exact.executionEligible())
            second.release()
            third.release()
        } finally { next.onDestroy() }
    }

    @Test fun locationServiceRequiresRuntimePermissionAndRoutesAllSites() {
        val info = ServiceInfo().apply {
            packageName = application.packageName
            name = ForegroundService.Location::class.java.name
        }
        ServiceInfo::class.java.getDeclaredField("mForegroundServiceType").apply { isAccessible = true }
            .setInt(info, ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION)
        shadowOf(application.packageManager).addOrUpdateService(info)
        val geo = ForegroundServices(application, "geolocation") { false }
        assertTrue(geo.declared("location"))
        assertTrue(geo.declared("location", "location"))
        assertNull("refused without runtime location permission", geo.claim("location", setOf("location"), "app"))

        shadowOf(application).grantPermissions(android.Manifest.permission.ACCESS_FINE_LOCATION)
        var promoted = false
        val claim = geo.claim("location", setOf("location"), "app", promoted = { promoted = true })!!
        val startIntent = shadowOf(application).nextStartedService!!
        assertEquals(ComponentName(application, ForegroundService.Location::class.java), startIntent.component)
        assertEquals(ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION, startIntent.getIntExtra(Claims.EXTRA_TYPES, 0))

        val service = Robolectric.buildService(ForegroundService.Location::class.java).create().get()
        try {
            assertEquals("location", service.kind)
            service.onStartCommand(startIntent, 0, 1)
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(promoted)
            assertEquals(listOf(ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION), recorded(service).startedTypes)
            claim.release()
            assertTrue(shadowOf(service).isForegroundStopped)
        } finally { service.onDestroy() }
    }
}
