package dev.exact.canvas

import android.content.Context
import android.os.VibratorManager
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** LLP 1009 D10: a GPU surface's haptics on the vibrator. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class SurfaceHapticsTest {
    /** Before Android 16 a primitive's delay is a pause after the one before ends: its duration comes off. */
    @Test fun aPatternsPrimitivesPlayAtTheirTimes() {
        val (delays, end) = SurfaceHaptics.schedule(intArrayOf(0, 50, 60), intArrayOf(12, 20, 20), relative = false)
        // At 0; 50 is 38 ms after the first ends at 12; 60 is due before the second ends at 70, so it follows it.
        assertArrayEquals(intArrayOf(0, 38, 0), delays)
        assertEquals(90, end)
        // From Android 16 a delay counts from the previous start, and the last to end ends the pattern.
        val (relative, last) = SurfaceHaptics.schedule(intArrayOf(10, 50, 60), intArrayOf(12, 40, 5), relative = true)
        assertArrayEquals(intArrayOf(10, 40, 10), relative)
        assertEquals(90, last)
    }

    private val texture = """{"texture":{"intensity":0.5,"sharpness":0}}"""

    /** Made while the activity is paused (its first request came after the pause), it plays nothing until resumed. */
    @Test fun madeWhilePausedItWaitsForTheResume() {
        val context: Context = RuntimeEnvironment.getApplication()
        val vibrator = context.getSystemService(VibratorManager::class.java).defaultVibrator
        shadowOf(vibrator).setHasAmplitudeControl(true)
        val haptics = SurfaceHaptics(context, suspended = true)
        haptics.told(1, texture)
        assertFalse(shadowOf(vibrator).isVibrating)
        haptics.suspended = false
        haptics.told(1, texture)
        assertTrue(shadowOf(vibrator).isVibrating)
        haptics.suspended = true
        assertFalse(shadowOf(vibrator).isVibrating)
    }
}
