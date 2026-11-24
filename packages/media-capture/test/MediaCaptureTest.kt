package exact.mediacapture

import dev.exact.canvas.AudioSource
import dev.exact.canvas.Cause
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import dev.exact.canvas.TrackError
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
        module.later(name, args, null, Cause("app")) { answer = it }
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
        context.tracks.subscribe(id, consumer, start, Cause(origin))

    @Test
    @Config(shadows = [dev.exact.canvas.ExtensionInvokeTest.Host::class])
    fun aThrowingViewfinderHatchRollsBackItsSubscription() {
        val videos = VideoTracks(context) { 1 }
        val tracks = org.robolectric.util.ReflectionHelpers.getField<MutableMap<String, VideoTracks.Track>>(videos, "tracks")
        val camera = exact.camera.CameraModule(context)
        try {
            for (alreadyLeased in listOf(false, true)) {
                val id = "camera-$alreadyLeased"
                val track = VideoTracks.Track(id, "stream-$id")
                tracks[id] = track
                context.tracks.add(dev.exact.canvas.TrackEntry(id, "video", owner = videos))
                val leave = if (alreadyLeased) videos.subscribe(id, Consumer(CameraCapture), false, Cause("app")) else null
                var hatches = 0
                exact.camera.CameraModule.useCasesHatch = { hatches++; throw IllegalStateException("hatch refused") }
                val view = camera.view("exact-camera", "1", id, context.application, JSONObject().put("srcObject", id)) { _, _ -> }
                try {
                    camera.bound("1", id, "srcObject", id, Cause("app"))
                    camera.displayed("1", id, true, Cause("app"))
                    assertEquals(1, hatches)
                    assertEquals(if (alreadyLeased) 1 else 0, track.consumers.size)
                    assertEquals(alreadyLeased, track.leased)
                    leave?.invoke()
                    assertTrue(track.consumers.isEmpty())
                    assertFalse(track.leased)
                    exact.camera.CameraModule.useCasesHatch = { hatches++ }
                    camera.displayed("1", id, true, Cause("app"))
                    assertEquals(3, hatches)
                    assertEquals(1, track.consumers.size)
                    assertTrue(track.leased)
                } finally {
                    camera.destroyView(view)
                    leave?.invoke()
                }
                assertTrue(track.consumers.isEmpty())
                assertFalse(track.leased)
            }
        } finally {
            exact.camera.CameraModule.useCasesHatch = null
            videos.endAll("test ended")
            CameraCapture.foreground(true)
        }
    }

    @Test fun aThrowingAudioJoinRollsBackItsSubscription() {
        for (alreadyActive in listOf(false, true)) {
            for (callback in listOf("live", "muted", "unmuted")) {
                if (alreadyActive && callback == "unmuted") continue
                val id = open("audio")
                val leave = if (alreadyActive) subscribe(id, Consumer(), start = true) else null
                module.preferred(JSONObject().put("mediaStreamTrack", JSONObject().put("muted", callback == "muted")))
                var deliveries = 0
                val failure = IllegalStateException("join refused")
                val consumer = object : TrackConsumer {
                    private fun deliver(name: String) { deliveries++; if (name == callback) throw failure }
                    override fun live(source: AudioSource?) = deliver("live")
                    override fun muted(reason: String) = deliver("muted")
                    override fun unmuted() = deliver("unmuted")
                    override fun ended() = deliver("ended")
                }
                assertSame(failure, assertThrows(IllegalStateException::class.java) {
                    context.tracks.subscribe(id, consumer, true, Cause("app"))
                })
                val delivered = deliveries
                assertEquals(alreadyActive, module.leased(id))
                leave?.invoke()
                assertFalse(module.leased(id))
                val replacement = Consumer()
                subscribe(id, replacement, start = true)()
                call("stopTrack", JSONObject().put("track", id))
                assertTrue("audio" in replacement.events)
                assertEquals(delivered, deliveries)
            }
        }
    }

    @Test fun aFailedVideoStartNotifiesTheRemainingConsumer() {
        val id = open()
        foreground(false)
        val remaining = Consumer()
        val leave = subscribe(id, remaining)
        val videos = context.tracks.lookup(id)!!.owner as VideoTracks
        val failure = IllegalStateException("join refused")
        val joining = object : TrackConsumer by Consumer() {
            override fun liveVideo(source: VideoSource?) { throw failure }
        }
        // Admission is updated before the extension receives its foreground request.
        context.foreground = true
        assertSame(failure, assertThrows(IllegalStateException::class.java) {
            context.tracks.subscribe(id, joining, false, Cause("app"))
        })
        assertFalse(videos.leased(id))
        assertEquals(listOf("idle", "video", "idle"), remaining.events)
        assertEquals("idle", state(id).getString("muteReason"))
        val retry = subscribe(id, Consumer())
        assertEquals(listOf("idle", "video", "idle", "video", "unmuted"), remaining.events)
        retry(); leave()
        assertFalse(videos.leased(id))
    }

    @Test fun aFailedAudioStartNotifiesTheRemainingConsumer() {
        val id = open("audio")
        subscribe(id, Consumer())()
        val remaining = Consumer()
        val leave = subscribe(id, remaining)
        val failure = IllegalStateException("join refused")
        val joining = object : TrackConsumer by Consumer() {
            override fun live(source: AudioSource?) { throw failure }
        }
        assertSame(failure, assertThrows(IllegalStateException::class.java) {
            context.tracks.subscribe(id, joining, true, Cause("app"))
        })
        assertFalse(module.leased(id))
        assertEquals(listOf("idle", "audio", "idle"), remaining.events)
        assertEquals("idle", state(id).getString("muteReason"))
        val retry = subscribe(id, Consumer(), start = true)
        assertEquals(listOf("idle", "audio", "idle", "audio", "unmuted"), remaining.events)
        retry(); leave()
        assertFalse(module.leased(id))
    }

    @Test fun aFailedFirstAudioReaderDoesNotConsumeTheFirstStart() {
        val id = open("audio")
        val failure = IllegalStateException("join refused")
        val joining = object : TrackConsumer by Consumer() {
            override fun live(source: AudioSource?) { throw failure }
        }
        assertSame(failure, assertThrows(IllegalStateException::class.java) {
            context.tracks.subscribe(id, joining, false, Cause("app"))
        })
        assertFalse(module.leased(id))
        val replacement = Consumer()
        val leave = subscribe(id, replacement)
        assertTrue(module.leased(id))
        assertEquals(listOf("audio", "unmuted"), replacement.events)
        leave()
        assertFalse(module.leased(id))
    }

    @Test fun aFailedAudioJoinDoesNotKeepTheFailedLeasesPlatformMute() {
        val id = open("audio")
        val tracks = org.robolectric.util.ReflectionHelpers.getField<Map<String, Any>>(module, "tracks")
        val track = tracks.getValue(id)
        val failure = IllegalStateException("join refused")
        val joining = object : TrackConsumer by Consumer() {
            override fun live(source: AudioSource?) {
                // Agent activation skips Capture.lease, whose failure sets this before live is delivered.
                org.robolectric.util.ReflectionHelpers.setField(track, "platformMute", "interrupted")
                throw failure
            }
        }
        assertSame(failure, assertThrows(IllegalStateException::class.java) {
            context.tracks.subscribe(id, joining, false, Cause("app"))
        })
        assertFalse(module.leased(id))
        val replacement = Consumer()
        val leave = subscribe(id, replacement)
        assertTrue(module.leased(id))
        assertEquals(listOf("audio", "unmuted"), replacement.events)
        assertFalse(state(id).getBoolean("muted"))
        leave()
        assertFalse(module.leased(id))
    }

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
            foreground(true)
            assertEquals(listOf("idle", "video", "unmuted"), app.events)
            assertEquals(listOf("idle", "video", "unmuted"), consumer.events)
        }
    }

    @Test fun enablingAnUnleasedTrackWaitsForForeground() {
        val id = open()
        val consumer = Consumer()
        subscribe(id, consumer)
        call("setTrackEnabled", JSONObject().put("track", id).put("enabled", false))
        foreground(false)
        consumer.events.clear()
        call("setTrackEnabled", JSONObject().put("track", id).put("enabled", true))
        assertTrue(consumer.events.isEmpty())
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
            val release = videos.subscribe(back.id, Consumer(CameraCapture), false, Cause("app"))
            val consumer = Consumer(CameraCapture)
            videos.subscribe(front.id, consumer, false, Cause("app"))
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

    @Test fun aPressThatExpiredAtTheTrackOwnerLeavesNoRecorderSubscribed() {
        val id = open("audio")
        val recorder = Consumer()
        assertThrows(TrackError::class.java) {
            context.tracks.subscribe(id, recorder, true, Cause("press", press = 7))
        }
        val level = Consumer()
        context.tracks.subscribe(id, level, false, Cause("press", press = 7))
        assertEquals(listOf("idle"), level.events)
        val leave = subscribe(id, Consumer(), start = true)
        assertTrue(recorder.events.isEmpty())
        assertEquals("audio", level.events[1])
        leave()
    }

    @Test fun aSuspendedMicrophoneRespectsTheRecognizersHold() {
        val id = open("audio")
        val leave = subscribe(id, Consumer(), start = true)
        foreground(false)
        val release = context.tracks.hold("audio")
        foreground(true)
        val enable = JSONObject().put("track", id).put("enabled", true)
        call("setTrackEnabled", enable)
        assertFalse(module.leased(id))
        release()
        call("setTrackEnabled", enable)
        assertTrue(module.leased(id))
        leave()
    }

    @Test
    @Config(shadows = [dev.exact.canvas.ExtensionInvokeTest.Host::class])
    fun returningOneOwnerChecksTheOtherOwnersCameraBeforeRestartingCameraX() {
        val first = VideoTracks(context) { 1 }
        val other = VideoTracks(context) { 2 }
        val track = VideoTracks.Track("camera", "stream")
        val tracks = org.robolectric.util.ReflectionHelpers.getField<MutableMap<String, VideoTracks.Track>>(other, "tracks")
        tracks[track.id] = track
        try {
            val leaveApp = other.subscribe(track.id, Consumer(CameraCapture), false, Cause("app"))
            other.subscribe(track.id, Consumer(CameraCapture), false, Cause("action"))
            assertTrue(CameraCapture.holders.containsKey(track))
            other.foreground(false)
            leaveApp()
            first.foreground(true)
            assertFalse(CameraCapture.holders.containsKey(track))
            assertFalse(track.leased)
        } finally {
            other.endAll("test ended")
            CameraCapture.foreground(true)
        }
    }

    @Test fun explicitEnableNeedsAConsumerThatMayOpenTheDevice() {
        for (kind in listOf("audio", "video")) {
            val id = open(kind)
            val enable = JSONObject().put("track", id).put("enabled", true)
            val leave = subscribe(id, Consumer(), "action")
            call("setTrackEnabled", enable)
            assertEquals("idle", state(id).getString("muteReason"))
            if (kind == "audio") {
                subscribe(id, Consumer(), start = true)()
                leave()
                val leaveLevel = subscribe(id, Consumer())
                assertTrue(state(id).getBoolean("muted"))
                call("setTrackEnabled", enable)
                assertFalse(state(id).getBoolean("muted"))
                leaveLevel()
            } else leave()
        }
    }

    @Test fun aCameraHeldBeforeBackgroundRequiresACurrentAppConsumerToResume() {
        val id = open()
        val leaveApp = subscribe(id, Consumer())
        val leaveReader = subscribe(id, Consumer(), "action")
        foreground(false)
        leaveApp()
        foreground(true)
        assertEquals("idle", state(id).getString("muteReason"))
        val leaveNewApp = subscribe(id, Consumer())
        assertFalse(state(id).getBoolean("muted"))
        leaveNewApp()
        leaveReader()
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
