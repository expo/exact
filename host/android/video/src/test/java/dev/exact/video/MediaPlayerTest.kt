package dev.exact.video

import android.app.Activity
import android.os.Looper
import android.view.View
import android.widget.FrameLayout
import androidx.media3.ui.PlayerView
import dev.exact.canvas.MediaSink
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.time.Duration

/** The record's rules (LLP 1042.001 D1, D3, D5, D8) as a player follows them, with no media loaded. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class MediaPlayerTest {
    private val made = ArrayList<MediaPlayer>()

    private class Events : MediaSink {
        val names = ArrayList<String>()
        override fun event(name: String, payload: String) { if (name != "timeupdate") names += if (payload.isEmpty()) name else "$name($payload)" }
        override fun state(json: String) {}
        override fun firstFrame() {}
    }

    private fun player() = MediaPlayer(RuntimeEnvironment.getApplication()).also { made += it }

    // `preload="none"` keeps the player from opening the file, which these rules never need.
    private fun props(vararg pairs: Pair<String, Any?>) = JSONObject()
        .put("src", "assets/a.mp4").put("uri", "file:///nonexistent/a.mp4").put("preload", "none").put("fit", "contain")
        .apply { for ((k, v) in pairs) put(k, v ?: JSONObject.NULL) }

    @After fun release() { made.forEach { it.release() }; shadowOf(Looper.getMainLooper()).idle() }

    @Test fun anUnchangedPausedFalseAfterARebuildIsNoRequestToPlay() {
        val first = player(); val events = Events()
        first.bind(events)
        first.update(props("paused" to false), 640, 360, null)
        assertTrue("play" in events.names)
        first.stop()
        val record = first.snapshot()!!
        assertTrue(record.paused)
        first.release()

        val second = player(); val again = Events()
        second.bind(again)
        second.update(props("paused" to false), 640, 360, record)
        assertFalse("play" in again.names)
        assertTrue(second.snapshot()!!.paused)
        // Assigned again, it plays.
        second.update(props("paused" to true), 640, 360, null)
        second.update(props("paused" to false), 640, 360, null)
        assertTrue("play" in again.names)
    }

    @Test fun aBorrowedPlayerCarriesNothingOfTheNodeBefore() {
        val p = player(); val a = Events()
        p.bind(a)
        p.update(props("paused" to false, "playbackRate" to 2.0), 640, 360, null)
        assertEquals(2f, p.snapshot()!!.speed)
        val b = Events()
        p.bind(b)
        p.update(props("paused" to true).put("src", "assets/b.mp4"), 640, 360, null)
        val record = p.snapshot()!!
        assertEquals(1f, record.speed)
        assertTrue(record.paused)
        assertEquals(emptyList<String>(), b.names.filter { it == "play" || it == "ratechange" })
    }

    @Test fun nothingIsReportedAfterRelease() {
        val p = player(); val events = Events()
        p.bind(events)
        p.update(props("paused" to false), 640, 360, null)
        p.release()
        val before = events.names.size
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(before, events.names.size)
        made.remove(p)
    }

    @Test fun theOffScreenRulePausesAMutedAutoplayAndResumesItWhenSeen() {
        val p = player(); val events = Events()
        p.bind(events)
        p.update(props("autoplay" to true, "muted" to true), 640, 360, null)
        // It waits to be seen.
        assertEquals(emptyList<String>(), events.names.filter { it == "play" })
        p.shown(true)
        assertEquals(listOf("play"), events.names.filter { it == "play" || it == "pause" })
        assertFalse(p.engaged)
        p.shown(false)
        assertTrue(p.snapshot()!!.resumes)
        p.shown(true)
        assertEquals(listOf("play", "pause", "play"), events.names.filter { it == "play" || it == "pause" })
    }

    @Test fun aBoundPlayingVideoIsEngagedAndPlaysOnOutOfView() {
        val p = player(); val events = Events()
        p.bind(events)
        p.update(props("autoplay" to true, "muted" to true, "paused" to false), 640, 360, null)
        p.shown(true)
        assertTrue(p.engaged)
        p.shown(false)
        assertFalse(p.snapshot()!!.paused)
    }

    @Test fun theStopPausesAWatchedVideoAndHoldsAFeedVideoForWhenItIsSeen() {
        val watched = player(); watched.bind(Events())
        watched.update(props("paused" to false), 640, 360, null)
        watched.stop()
        watched.snapshot()!!.let { assertTrue(it.paused); assertFalse(it.resumes) }

        val feed = player(); feed.bind(Events())
        feed.update(props("autoplay" to true, "muted" to true), 640, 360, null)
        feed.shown(true)
        feed.stop()
        feed.snapshot()!!.let { assertTrue(it.paused); assertTrue(it.resumes) }
    }

    @Test fun aRateOutOfRangeIsRefusedWithoutAMediaError() {
        val p = player(); val events = Events()
        p.bind(events)
        p.update(props("playbackRate" to 1.5), 640, 360, null)
        p.update(props("playbackRate" to 9.0), 640, 360, null)
        assertTrue("error(invalid-value)" in events.names)
        assertEquals(1.5f, p.snapshot()!!.speed)
    }

    @Test fun anEmptySourceIsNotSupported() {
        val p = player(); val events = Events()
        p.bind(events)
        p.update(props().put("src", "").put("uri", ""), 640, 360, null)
        assertEquals(listOf("error(src-not-supported)"), events.names)
    }

    @Test fun aLoadStartsTheSourceAgainOnceAndARebuiltPlayerDoesNotRepeatIt() {
        val p = player(); val events = Events()
        p.bind(events)
        p.update(props("paused" to false), 640, 360, null)
        p.update(props("paused" to false, "load" to 1), 640, 360, null)
        // Loaded again, the bound `paused=false` plays it again, as a changed `src` would.
        assertEquals(listOf("play", "play"), events.names.filter { it == "play" })
        p.stop()
        val record = p.snapshot()!!
        p.release(); made.remove(p)

        val again = player(); val later = Events()
        again.bind(later)
        again.update(props("paused" to false, "load" to 1), 640, 360, record)
        assertEquals(emptyList<String>(), later.names.filter { it == "play" })
    }

    @Test fun aFocusedControllerHoldsTheWindowsFocusAtItsPlayButton() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val reader = View(activity).apply { isFocusable = true; isFocusableInTouchMode = true }
        val p = player(); p.bind(Events())
        activity.setContentView(FrameLayout(activity).apply { addView(reader, 100, 100); addView(p.view, 640, 360) })
        p.update(props("controls" to true), 640, 360, null)
        val view = p.controller as PlayerView
        view.hideController()
        reader.requestFocus()
        idle()
        p.update(props("controls" to true, "focused" to true), 640, 360, null)
        idle()
        assertTrue(view.findViewById<View>(androidx.media3.ui.R.id.exo_play_pause).isFocused)
        // Its controller hidden, the player keeps the focus, so the next key shows it again.
        view.hideController()
        idle()
        assertTrue(view.isFocused)
        // Not focused, it lets the focus go where the reader puts it.
        p.update(props("controls" to true, "focused" to false), 640, 360, null)
        reader.requestFocus()
        view.showController(); idle(); view.hideController(); idle()
        assertTrue(reader.isFocused)
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1))
}
