package dev.exact.canvas

import android.hardware.display.DisplayManager
import android.provider.Settings
import android.view.Surface
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.shadows.ShadowDisplayManager
import org.robolectric.annotation.Config

/** `formatLocale` carries the hour cycle the person forces (LLP 1027.000.000), as Apple's does. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class DeviceTest {
    @Test fun orientationReadsTheContextsOwnDisplay() {
        val app = RuntimeEnvironment.getApplication()
        val id = ShadowDisplayManager.addDisplay("w800dp-h480dp-land-mdpi")
        try {
            val display = app.getSystemService(DisplayManager::class.java).getDisplay(id)
            shadowOf(display).setRotation(Surface.ROTATION_180)
            // Half a turn of a naturally wide display is landscape-secondary.
            assertEquals(16 or (2 shl 2) or 3, Device.orientation(app.createDisplayContext(display)))
        } finally {
            ShadowDisplayManager.removeDisplay(id)
        }
    }

    @Test fun touchExplorationIsTheScreenReaderProxy() {
        val context = RuntimeEnvironment.getApplication()
        val manager = context.getSystemService(android.view.accessibility.AccessibilityManager::class.java)
        val shadow = org.robolectric.Shadows.shadowOf(manager)
        for (enabled in listOf(false, true, false)) {
            shadow.setTouchExplorationEnabled(enabled)
            assertEquals(if (enabled) 2 else 1, Device.read(context).screenReader)
        }
    }
    @Test fun aClockTheLocaleKeepsAddsNoCycle() {
        assertEquals("", Device.formatLocale(Locale.US, hour24 = false))
        assertEquals("", Device.formatLocale(Locale.GERMANY, hour24 = true))
        // fr-CA's preferred hour is `HH 'h'`: the `h` is a literal.
        assertEquals("", Device.formatLocale(Locale.CANADA_FRENCH, hour24 = true))
    }

    @Test fun aForcedClockIsTheLocalesHourCycle() {
        assertEquals("en-US-u-hc-h23", Device.formatLocale(Locale.US, hour24 = true))
        assertEquals("de-DE-u-hc-h12", Device.formatLocale(Locale.GERMANY, hour24 = false))
        assertEquals("fr-CA-u-hc-h12", Device.formatLocale(Locale.CANADA_FRENCH, hour24 = false))
    }

    @Test fun aTagPastTheRunnersBoundIsTheLocale() {
        val long = Locale.forLanguageTag("en-US-x-" + List(7) { "abcdefgh" }.joinToString("-"))
        assertEquals("", Device.formatLocale(long, hour24 = true))
    }

    /** Settings' "Use 24-hour format" reaches the read, and unset is the locale's own clock. */
    @Test fun theSettingIsRead() {
        val context = RuntimeEnvironment.getApplication()
        RuntimeEnvironment.setQualifiers("en-rUS")
        Settings.System.putString(context.contentResolver, Settings.System.TIME_12_24, null)
        assertEquals("", Device.read(context).formatLocale)
        Settings.System.putString(context.contentResolver, Settings.System.TIME_12_24, "24")
        assertEquals("en-US-u-hc-h23", Device.read(context).formatLocale)
        Settings.System.putString(context.contentResolver, Settings.System.TIME_12_24, "12")
        assertEquals("", Device.read(context).formatLocale)
    }
}
