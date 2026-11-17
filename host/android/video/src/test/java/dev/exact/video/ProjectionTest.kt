package dev.exact.video

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** HTML's media events as LLP 1042.001 D7 projects them from Media3's callbacks. */
class ProjectionTest {
    private val sent = ArrayList<String>()
    private val p = Projection { name, payload -> sent += if (payload.isEmpty()) name else "$name($payload)" }

    private fun taken(): List<String> = sent.toList().also { sent.clear() }

    @Test fun metadataSendsDurationchangeBeforeLoadedmetadata() {
        p.load(restoring = false, paused = true)
        p.buffering()
        p.metadata(10.0)
        p.ready()
        assertEquals(listOf("durationchange(10.0)", "loadedmetadata", "canplay"), taken())
        assertEquals(4, p.readyState)
    }

    @Test fun anIndefiniteDurationSendsNoDurationchange() {
        p.load(restoring = false, paused = true)
        p.metadata(null)
        assertEquals(listOf("loadedmetadata"), taken())
        assertEquals(null, p.duration)
    }

    @Test fun theFirstBufferingIsTheLoadNotAStall() {
        p.load(restoring = false, paused = false)
        p.buffering()
        p.metadata(5.0)
        p.ready()
        assertEquals(listOf("durationchange(5.0)", "loadedmetadata", "canplay"), taken())
        p.buffering()
        p.ready()
        assertEquals(listOf("waiting", "canplay"), taken())
        assertEquals(4, p.readyState)
    }

    @Test fun aPlayBeforeDataWaitsAndAutoplayDoesNot() {
        p.load(restoring = false, paused = true)
        assertTrue(p.play(autoplay = false))
        assertEquals(listOf("play", "waiting"), taken())
        p.load(restoring = false, paused = true)
        p.play(autoplay = true)
        assertEquals(listOf("play"), taken())
    }

    @Test fun pauseAndPlayEachSendOnceForAChange() {
        p.load(restoring = false, paused = true)
        p.metadata(1.0); p.ready(); taken()
        p.play(autoplay = false); p.play(autoplay = false)
        p.playing(true); p.playing(true)
        p.pause(); p.pause()
        assertEquals(listOf("play", "playing", "pause"), taken())
    }

    @Test fun theEndPausesThenEnds() {
        p.load(restoring = false, paused = false)
        p.metadata(1.0); p.ready(); p.playing(true); taken()
        p.end()
        assertEquals(listOf("pause", "ended"), taken())
        assertTrue(p.paused && p.ended)
    }

    @Test fun aLoopIsASeekAsChromeReportsIt() {
        p.load(restoring = false, paused = false)
        p.looped()
        assertEquals(listOf("seeking", "waiting", "seeked", "canplay", "playing"), taken())
        assertTrue(!p.paused)
    }

    @Test fun aSeekReportsItsTargetUntilItLands() {
        p.load(restoring = false, paused = true)
        p.metadata(30.0); p.ready(); taken()
        p.seek(12.5)
        assertEquals(12.5, p.seekTarget)
        p.buffering()
        p.ready()
        assertEquals(listOf("seeking", "seeked", "canplay"), taken())
        assertEquals(null, p.seekTarget)
    }

    @Test fun restorationSendsNoLoadOrSeekEvents() {
        p.load(restoring = true, paused = true)
        p.buffering()
        p.metadata(30.0)
        p.seek(4.0)
        p.ready()
        assertEquals(emptyList<String>(), taken())
        assertEquals(4, p.readyState)
    }

    @Test fun aPlayingNodeRestoredStallsAsAStallWould() {
        p.load(restoring = true, paused = false)
        p.buffering()
        p.metadata(30.0)
        p.ready()
        p.playing(true)
        assertEquals(listOf("waiting", "canplay", "playing"), taken())
    }
}
