package exact.mediacapture

import dev.exact.canvas.AudioSource
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import dev.exact.canvas.TrackConsumer
import dev.exact.canvas.VideoSource
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [MediaCaptureTest.Host::class])
class MediaCaptureTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = true
            @JvmStatic @Implementation fun changed(topic: String) {}
        }
    }

    private lateinit var context: ExtensionContext
    private lateinit var module: MediaCaptureModule

    private class Consumer(private val camera: VideoSource? = null) : TrackConsumer {
        val events = ArrayList<String>()
        override fun liveVideo(source: VideoSource?) { assertSame(camera, source); events += "video" }
        override fun live(source: AudioSource?) { assertNull(source); events += "audio" }
        override fun muted(reason: String) { events += reason }
        override fun unmuted() { events += "unmuted" }
        override fun ended() { events += "ended" }
    }

    @Before fun setUp() {
        context = ExtensionContext(RuntimeEnvironment.getApplication(), "media-capture")
        module = MediaCaptureModule(context)
    }

    @After fun tearDown() { module.destroy {} }

    private fun call(name: String, args: JSONObject = JSONObject()): JSONObject {
        var answer: JSONObject? = null
        module.later(name, args) { answer = it }
        return requireNotNull(answer)
    }

    private fun open(kind: String = "video") = call("getUserMedia", JSONObject().put(kind, true))
        .getJSONArray("tracks").getJSONObject(0).getString("id")

    private fun state(id: String) = call("mediaStreamTrack", JSONObject().put("track", id))

    private fun foreground(shown: Boolean) {
        // Extensions updates the admission fact before delivering the lifecycle request.
        context.foreground = shown
        module.foreground(shown)
    }

    private fun subscribe(id: String, consumer: Consumer, origin: String = "app", start: Boolean = false) =
        context.tracks.subscribe(id, consumer, start, origin)

    @Test fun anObserverNeverStartsOrHoldsTheCamera() {
        val id = open()
        val observer = Consumer()
        val leave = context.tracks.observe(id, observer)
        assertEquals(listOf("idle"), observer.events)
        foreground(false)
        foreground(true)
        assertEquals(listOf("idle"), observer.events)
        val release = subscribe(id, Consumer())
        assertEquals(listOf("idle", "video", "unmuted"), observer.events)
        release()
        assertEquals("idle", observer.events.last())
        assertTrue(state(id).getBoolean("muted"))
        leave()
    }

    @Test fun observersHearMuteResumeAndEndAndAreThenCleared() {
        val id = open()
        val leaveConsumer = subscribe(id, Consumer())
        val observer = Consumer()
        val leave = context.tracks.observe(id, observer)
        assertEquals(listOf("video"), observer.events)
        foreground(false)
        foreground(true)
        module.preferred(JSONObject().put("mediaStreamTrack", JSONObject().put("muted", true).put("muteReason", "interrupted")))
        module.preferred(JSONObject().put("mediaStreamTrack", JSONObject().put("muted", false)))
        call("stopTrack", JSONObject().put("track", id))
        assertEquals(listOf("video", "background", "unmuted", "interrupted", "unmuted", "ended"), observer.events)
        val videos = context.tracks.lookup(id)!!.owner as VideoTracks
        val tracks = org.robolectric.util.ReflectionHelpers.getField<Map<String, VideoTracks.Track>>(videos, "tracks")
        assertTrue(tracks.getValue(id).observers.isEmpty())
        assertTrue(tracks.getValue(id).consumers.isEmpty())
        val late = Consumer()
        context.tracks.observe(id, late)()
        assertEquals(listOf("ended"), late.events)
        leave(); leaveConsumer()
    }

    @Test fun anObserverIsNotAConsumerAndCannotReopenADisabledTrack() {
        val id = open()
        val observer = Consumer()
        val leave = context.tracks.observe(id, observer)
        val videos = context.tracks.lookup(id)!!.owner as VideoTracks
        val tracks = org.robolectric.util.ReflectionHelpers.getField<Map<String, VideoTracks.Track>>(videos, "tracks")
        val track = tracks.getValue(id)
        assertTrue(track.consumers.isEmpty())
        assertEquals(1, track.observers.size)
        call("setTrackEnabled", JSONObject().put("track", id).put("enabled", false))
        call("setTrackEnabled", JSONObject().put("track", id).put("enabled", true))
        assertFalse(track.leased)
        assertEquals(listOf("idle"), observer.events)
        leave(); leave()
        assertTrue(track.observers.isEmpty())
        val release = subscribe(id, Consumer())
        assertTrue(track.leased)
        assertEquals(listOf("idle"), observer.events)
        release()
    }

    @Test fun aViewfinderRecreatedBeforeForegroundTakesItsLeaseOnReturn() {
        for (releaseBeforeBackground in listOf(true, false)) {
            val id = open()
            val leave = subscribe(id, Consumer())
            if (releaseBeforeBackground) leave()
            foreground(false)
            if (!releaseBeforeBackground) leave()
            val replacement = Consumer()
            val remove = subscribe(id, replacement)
            assertEquals(listOf("idle"), replacement.events)
            foreground(true)
            assertEquals(listOf("idle", "video", "unmuted"), replacement.events)
            assertFalse(state(id).getBoolean("muted"))
            remove()
        }
    }

    @Test fun aViewfinderRecreatedAfterForegroundTakesItsLeaseImmediately() {
        val id = open()
        val leave = subscribe(id, Consumer())
        foreground(false)
        leave()
        foreground(true)
        val replacement = Consumer()
        subscribe(id, replacement)
        assertEquals(listOf("video", "unmuted"), replacement.events)
    }

    @Test fun foregroundDoesNotLeaseDisabledEndedOrUnconsumedTracks() {
        for (condition in listOf("disabled", "ended", "unconsumed")) {
            val id = open()
            foreground(false)
            val consumer = Consumer()
            val leave = subscribe(id, consumer)
            when (condition) {
                "disabled" -> call("setTrackEnabled", JSONObject().put("track", id).put("enabled", false))
                "ended" -> call("stopTrack", JSONObject().put("track", id))
                "unconsumed" -> leave()
            }
            foreground(true)
            assertFalse(condition, "video" in consumer.events)
            assertTrue(state(id).getBoolean("muted"))
            leave()
        }
    }

    @Test fun onlyAKeptAppConsumerCanAcquireTheLeaseOnReturn() {
        for (origin in listOf("tool", "wake")) {
            val id = open()
            foreground(false)
            val consumer = Consumer()
            subscribe(id, consumer, origin)
            val leaveApp = subscribe(id, Consumer())
            leaveApp()
            foreground(true)
            assertEquals(listOf("idle"), consumer.events)
            foreground(false)
            val app = Consumer()
            subscribe(id, app)
            context.origin = origin
            foreground(true)
            assertEquals(listOf("idle", "video", "unmuted"), app.events)
            assertEquals(listOf("idle", "video", "unmuted"), consumer.events)
        }
    }

    @Test fun enablingAnUnleasedTrackBeforeForegroundDoesNotBlankItsNewVideo() {
        val id = open()
        val consumer = Consumer()
        subscribe(id, consumer)
        call("setTrackEnabled", JSONObject().put("track", id).put("enabled", false))
        foreground(false)
        consumer.events.clear()
        call("setTrackEnabled", JSONObject().put("track", id).put("enabled", true))
        foreground(true)
        assertEquals(listOf("video", "unmuted"), consumer.events)
        assertFalse(state(id).getBoolean("muted"))
    }

    @Test
    @Config(shadows = [dev.exact.canvas.ExtensionInvokeTest.Host::class])
    fun aWaitingTrackRetriedBeforeForegroundDoesNotBlankItsNewVideo() {
        val videos = VideoTracks(context) { 1 }
        // Seed tracks without a provider; the real process lease table arbitrates their cameras.
        val tracks = org.robolectric.util.ReflectionHelpers.getField<MutableMap<String, VideoTracks.Track>>(videos, "tracks")
        val back = VideoTracks.Track("back", "stream-back")
        val front = VideoTracks.Track("front", "stream-front").apply { camera = "front" }
        tracks[back.id] = back
        tracks[front.id] = front
        try {
            val release = videos.subscribe(back.id, Consumer(CameraCapture), false, "app")
            val consumer = Consumer(CameraCapture)
            videos.subscribe(front.id, consumer, false, "app")
            assertEquals("in-use", videos.state(front.id).getString("muteReason"))
            videos.foreground(false)
            consumer.events.clear()
            release()
            assertTrue(front.leased)
            videos.foreground(true)
            assertEquals(listOf("video", "unmuted"), consumer.events)
            assertFalse(videos.state(front.id).getBoolean("muted"))
        } finally {
            videos.endAll("test ended")
            CameraCapture.foreground(true)
        }
    }

    @Test fun anExistingLeaseIsNotAcquiredTwice() {
        val id = open()
        val consumer = Consumer()
        subscribe(id, consumer)
        foreground(false)
        foreground(true)
        foreground(true)
        assertEquals(listOf("video", "unmuted", "background", "unmuted"), consumer.events)
    }

    @Test fun reacquiringALeasePreservesTheAgentsPlatformMute() {
        val id = open()
        foreground(false)
        module.preferred(JSONObject().put("mediaStreamTrack", JSONObject().put("muted", true).put("muteReason", "interrupted")))
        val consumer = Consumer()
        subscribe(id, consumer)
        foreground(true)
        assertTrue("video" in consumer.events)
        assertFalse("unmuted" in consumer.events)
        assertEquals("interrupted", state(id).getString("muteReason"))
    }

    @Test fun audioConsumersRemainActiveAcrossBackgroundAndResume() {
        val id = open("audio")
        val consumer = Consumer()
        subscribe(id, consumer, start = true)
        foreground(false)
        foreground(true)
        assertEquals(listOf("audio", "unmuted", "background", "audio", "unmuted"), consumer.events)
        assertTrue(module.leased(id))
    }

    @Test fun anIdleMicrophoneNeedsAnExplicitStartRatherThanAReturningLevelReader() {
        val id = open("audio")
        subscribe(id, Consumer(), start = true)()
        foreground(false)
        val level = Consumer()
        subscribe(id, level)
        foreground(true)
        assertEquals(listOf("idle"), level.events)
        assertFalse(module.leased(id))
        subscribe(id, Consumer(), start = true)
        assertTrue(module.leased(id))
        assertEquals(listOf("idle", "audio", "unmuted"), level.events)
    }
}
