package exact.recording

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** A level's statistics (LLP 1133.009 D1) and `<audio-waveform>`'s bars (D13), as the web's and Apple's are
 * (`web/level.test.mjs`, `web/waveform.test.mjs`). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class MeterTest {
    /** A meter on an idle track counts the whole silent span, however much of it the history keeps. */
    @Test fun aSilentMeterCountsItsWholeSpan() {
        val meter = Meter(50.0, 100, 0.0, agent = false)
        for ((t, duration) in listOf(10_000.0 to 10.0, 3_610_000.0 to 3610.0)) {
            val read = meter.read(t)
            assertEquals(duration, read.getDouble("totalSamplesDuration"), 1e-9)
            assertEquals(100, read.getJSONArray("levels").length())
            assertEquals(0.0, read.getDouble("audioLevel"), 0.0)
        }
    }

    /** The agent's three seconds draw sixty bars at a step's start, each a level in decibels. */
    @Test fun theAgentsThreeSecondsDrawSixtyBars() {
        val meter = Meter(WaveformBars.STEP, WaveformBars.HISTORY, 1000.0, agent = true)
        meter.live(null)
        val bars = WaveformBars(1000.0)
        bars.advance(1000.0, meter.advance(1000.0))
        bars.advance(4000.0, meter.advance(4000.0))
        assertEquals(60, bars.bars.size)
        assertEquals(0.0, bars.phase, 0.0)
        assertEquals(0.6, WaveformBars.height(0.1), 1e-12)
        assertEquals(0.0, WaveformBars.height(0.0), 0.0)
    }
}
