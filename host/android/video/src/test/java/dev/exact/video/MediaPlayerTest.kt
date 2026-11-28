package dev.exact.video

import android.app.Activity
import android.graphics.Bitmap
import android.net.Uri
import android.os.Looper
import android.view.View
import android.widget.FrameLayout
import androidx.media3.common.C
import androidx.media3.common.Player
import androidx.media3.common.util.BitmapLoader
import androidx.media3.ui.PlayerView
import com.google.common.util.concurrent.SettableFuture
import dev.exact.canvas.MediaSink
import org.json.JSONArray
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
import org.robolectric.util.ReflectionHelpers
import java.time.Duration

/** The record's rules (LLP 1042.001 D1, D3, D5, D8) as a player follows them, with no media loaded. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class MediaPlayerTest {
    private val made = ArrayList<MediaPlayer>()

    private class Events : MediaSink {
        val names = ArrayList<String>()
        val states = ArrayList<JSONObject>()
        override fun event(name: String, payload: String) { if (name != "timeupdate") names += if (payload.isEmpty()) name else "$name($payload)" }
        override fun state(json: String) { states += JSONObject(json) }
        override fun firstFrame() {}
    }

    private fun player() = MediaPlayer(RuntimeEnvironment.getApplication()).also { made += it }

    // `preload="none"` keeps the player from opening the file, which these rules never need.
    private fun props(vararg pairs: Pair<String, Any?>) = JSONObject()
        .put("src", "assets/a.mp4").put("uri", "file:///nonexistent/a.mp4").put("preload", "none").put("fit", "contain")
        .apply { for ((k, v) in pairs) put(k, v ?: JSONObject.NULL) }

    @After fun release() { made.forEach { it.release() }; shadowOf(Looper.getMainLooper()).idle() }

    @Test fun changingStationsClearsTheCoverAndIgnoresLateArtwork() {
        val loads = mutableMapOf<Uri, SettableFuture<Bitmap>>()
        val loader = object : BitmapLoader {
            override fun supportsMimeType(mimeType: String) = true
            override fun decodeBitmap(data: ByteArray) = error("unexpected embedded artwork")
            override fun loadBitmap(uri: Uri) = loads.getOrPut(uri) { SettableFuture.create() }
        }
        val p = MediaPlayer(RuntimeEnvironment.getApplication(), loader, null).also { made += it }
        p.bind(Events())
        val native = ReflectionHelpers.getField<Player>(p, "exo")
        fun station(name: String, art: String) = p.update(props("audio" to true, "src" to name,
            "mediaTitle" to name, "mediaArtwork" to art), 0, 0, null)
        val first = Uri.parse("https://example.com/first.png")
        val second = Uri.parse("https://example.com/second.png")
        val image = Bitmap.createBitmap(2, 2, Bitmap.Config.ARGB_8888)
        station("First", first.toString())
        station("Second", second.toString())
        loads.getValue(second).set(image)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(second, native.currentMediaItem!!.mediaMetadata.artworkUri)
        station("Third", "")
        loads.getValue(first).set(image)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("Third", native.currentMediaItem!!.mediaMetadata.title)
        assertEquals(null, native.currentMediaItem!!.mediaMetadata.artworkUri)
    }

    @Test fun streamingPlaybackKeepsWifiAwake() {
        assertEquals(C.WAKE_MODE_NETWORK, playbackWakeMode(Uri.parse("https://example.com/radio.mp3")))
        assertEquals(C.WAKE_MODE_NETWORK, playbackWakeMode(Uri.parse("HTTP://example.com/radio.mp3")))
        assertEquals(C.WAKE_MODE_LOCAL, playbackWakeMode(Uri.parse("file:///radio.mp3")))
    }

    @Test fun backgroundPlaybackAudioLetsTheDisplaySleepByDefault() {
        val playback = MediaPlayer(RuntimeEnvironment.getApplication()) { null }.also { made += it }
        val standard = player()
        for (p in listOf(playback, standard)) {
            val audio = props("audio" to true, "controls" to true)
            p.update(audio, 640, 360, null)
            ReflectionHelpers.callInstanceMethod<Player.Listener>(p, "listener").onIsPlayingChanged(true)
            assertEquals(p === standard, p.view.keepScreenOn)
            p.update(audio.put("preventsDisplaySleepDuringVideoPlayback", true), 640, 360, null)
            assertTrue(p.view.keepScreenOn)

            p.update(props(), 640, 360, null)
            assertTrue(p.view.keepScreenOn)
            p.update(props("preventsDisplaySleepDuringVideoPlayback" to false), 640, 360, null)
            assertFalse(p.view.keepScreenOn)
        }
    }

    @Test fun aNativePlayWithRefusedFocusLeavesThePlayerPaused() {
        val p = player()
        ReflectionHelpers.setField(p, "focusType", "playback")
        val events = Events()
        p.bind(events)
        p.update(props("controls" to true), 640, 360, null)
        val native = (p.controller as PlayerView).player!!
        native.play()
        assertTrue("error(not-allowed)" in events.names)
        assertFalse(native.playWhenReady)
        assertTrue(p.snapshot()!!.paused)
    }

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

    @Test fun optedOutMutedAutoplayAudioControlsPauseOutOfView() {
        val p = player()
        val events = Events()
        p.bind(events)
        p.update(props("audio" to true, "controls" to true, "autoplay" to true, "muted" to true), 300, 54, null)
        p.shown(true)
        assertFalse(p.snapshot()!!.paused)
        p.shown(false)
        assertTrue(p.snapshot()!!.paused)
        p.shown(true)
        assertEquals(listOf("play", "pause", "play"), events.names.filter { it == "play" || it == "pause" })
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

    @Test fun theWholeContentFrameFollowsThePictureWhileControlsKeepTheElementBounds() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val p = player(); p.bind(Events())
        val root = FrameLayout(activity).apply { addView(p.view, 160, 80) }
        activity.setContentView(root)
        val cases = listOf(
            "contain" to listOf(0f, .5f, 1f, .5f),
            "cover" to listOf(-1f, 0f, 2f, 1f),
            "fill" to listOf(0f, 0f, 1f, 1f),
            "none" to listOf(.75f, .875f, .25f, .125f),
            "scale-down" to listOf(.75f, .875f, .25f, .125f),
        )
        for ((fit, rect) in cases) {
            p.update(props("controls" to true, "fit" to fit, "objectRect" to JSONArray(rect)), 160, 80, null)
            val view = p.controller as PlayerView
            view.setControllerAnimationEnabled(false)
            view.showController()
            view.setCustomErrorMessage("Cannot play")
            val content = view.findViewById<FrameLayout>(androidx.media3.ui.R.id.exo_content_frame)
            val leaves = listOf(view.videoSurfaceView!!, view.subtitleView!!,
                view.findViewById(androidx.media3.ui.R.id.exo_shutter), view.findViewById<View>(androidx.media3.ui.R.id.exo_artwork))
            leaves.forEach { it.visibility = View.VISIBLE }
            root.measure(View.MeasureSpec.makeMeasureSpec(200, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(120, View.MeasureSpec.EXACTLY))
            root.layout(0, 0, 200, 120)
            val (x, y, w, h) = listOf(rect[0] * 160, rect[1] * 80, rect[2] * 160, rect[3] * 80)
            assertEquals(fit, listOf(x, y, w, h), listOf(content.x, content.y, content.width.toFloat(), content.height.toFloat()))
            for (leaf in leaves) {
                assertEquals(fit, content, leaf.parent)
                assertEquals(fit, listOf(0, 0, w.toInt(), h.toInt()), listOf(leaf.left, leaf.top, leaf.width, leaf.height))
            }
            val error = view.findViewById<View>(androidx.media3.ui.R.id.exo_error_message)
            assertEquals(fit, content, error.parent)
            assertEquals(fit, w / 2, error.left + error.width / 2f, 1f)
            val controller = view.findViewById<View>(androidx.media3.ui.R.id.exo_controller)
            assertEquals(fit, listOf(0, 0, 160, 80), listOf(controller.left, controller.top, controller.width, controller.height))
            val visible = android.graphics.Rect()
            assertTrue(fit, content.getLocalVisibleRect(visible))
            assertEquals(fit, android.graphics.Rect((-x).coerceAtLeast(0f).toInt(), (-y).coerceAtLeast(0f).toInt(),
                w.coerceAtMost(160 - x).toInt(), h.coerceAtMost(80 - y).toInt()), visible)
        }
        activity.finish()
    }

    @Test fun captionsWithoutControlsFollowTheWholePictureBeforeClipping() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val p = player(); p.bind(Events())
        val root = FrameLayout(activity).apply { addView(p.view, 160, 80) }
        activity.setContentView(root)
        for (rect in listOf(listOf(-.5f, 0f, 2f, 1f), listOf(0f, -.5f, 1f, 2f), listOf(.75f, .875f, .25f, .125f))) {
            p.update(props("controls" to false, "objectRect" to JSONArray(rect)), 160, 80, null)
            root.measure(View.MeasureSpec.makeMeasureSpec(200, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(120, View.MeasureSpec.EXACTLY))
            root.layout(0, 0, 200, 120)
            val frame = p.view as FrameLayout
            val captions = (0 until frame.childCount).map { frame.getChildAt(it) }.filterIsInstance<androidx.media3.ui.SubtitleView>().single()
            val (x, y, w, h) = listOf(rect[0] * 160, rect[1] * 80, rect[2] * 160, rect[3] * 80)
            assertEquals(rect.toString(), listOf(x, y, w, h), listOf(captions.x, captions.y, captions.width.toFloat(), captions.height.toFloat()))
            val visible = android.graphics.Rect()
            assertTrue(captions.getLocalVisibleRect(visible))
            assertEquals(android.graphics.Rect((-x).coerceAtLeast(0f).toInt(), (-y).coerceAtLeast(0f).toInt(),
                w.coerceAtMost(160 - x).toInt(), h.coerceAtMost(80 - y).toInt()), visible)
        }
        activity.finish()
    }

    @Test fun releasingWhileFullscreenEmitsNothingAndSkipsPostReleaseReattach() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val p = MediaPlayer(activity).also { made += it }
        val events = Events()
        p.bind(events)
        p.update(props("controls" to true), 640, 360, null)
        p.update(props("controls" to true, "fullscreen" to 1), 640, 360, null)
        assertTrue("fullscreenchange(true)" in events.names)
        assertTrue(events.states.last().getBoolean("fullscreen"))
        val inline = p.controller as PlayerView
        p.release()
        made.remove(p)
        val eventsBefore = events.names.size
        val statesBefore = events.states.size
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(eventsBefore, events.names.size)
        assertEquals(statesBefore, events.states.size)
        assertEquals(null, inline.player)
        activity.finish()
    }

    @Test fun retiringAndRebindingAFullscreenPlayerCarriesNoStateOrLateEvent() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val p = MediaPlayer(activity).also { made += it }
        val first = Events()
        p.bind(first)
        p.update(props("controls" to false), 640, 360, null)
        p.update(props("controls" to false, "fullscreen" to 1), 640, 360, null)
        assertTrue(first.states.last().getBoolean("fullscreen"))
        assertTrue(p.lendable)

        p.stop()
        assertEquals(1, first.names.count { it == "fullscreenchange(false)" })
        assertFalse(first.states.last().getBoolean("fullscreen"))

        val second = Events()
        p.bind(second)
        p.update(props("controls" to false, "src" to "assets/b.mp4"), 640, 360, null)
        assertFalse(second.states.last().getBoolean("fullscreen"))
        assertFalse("fullscreenchange(false)" in second.names)
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse("fullscreenchange(false)" in second.names)
        assertFalse(second.states.last().getBoolean("fullscreen"))

        // Direct rebind without stop() also clears fullscreen silently before the next borrower uses the player.
        p.update(props("controls" to false, "src" to "assets/b.mp4", "fullscreen" to 2), 640, 360, null)
        assertTrue(second.states.last().getBoolean("fullscreen"))
        val third = Events()
        p.bind(third)
        p.update(props("controls" to false, "src" to "assets/c.mp4"), 640, 360, null)
        assertFalse(third.states.last().getBoolean("fullscreen"))
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse("fullscreenchange(false)" in third.names)
        assertFalse(third.states.last().getBoolean("fullscreen"))
        activity.finish()
    }

    @Test fun togglingControlsWhileFullscreenLeavesSynchronouslyBeforePresenting() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val p = MediaPlayer(activity).also { made += it }
        val events = Events()
        p.bind(events)
        p.update(props("controls" to true), 640, 360, null)
        p.update(props("controls" to true, "fullscreen" to 1), 640, 360, null)
        assertTrue(events.states.last().getBoolean("fullscreen"))

        p.update(props("controls" to false, "fullscreen" to 1), 640, 360, null)
        assertFalse(events.states.last().getBoolean("fullscreen"))
        assertEquals("TextureView", events.states.last().getString("renderer"))
        assertEquals(1, events.names.count { it == "fullscreenchange(false)" })
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, events.names.count { it == "fullscreenchange(false)" })

        // Re-entering fullscreen in the same update as a controls toggle succeeds because leave was synchronous.
        p.update(props("controls" to false, "fullscreen" to 2), 640, 360, null)
        assertTrue(events.states.last().getBoolean("fullscreen"))
        p.update(props("controls" to true, "fullscreen" to 3), 640, 360, null)
        assertTrue(events.states.last().getBoolean("fullscreen"))
        assertEquals("PlayerView", events.states.last().getString("renderer"))
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(events.states.last().getBoolean("fullscreen"))
        activity.finish()
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1))
}
