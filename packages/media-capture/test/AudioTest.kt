package exact.mediacapture

import android.os.Handler
import android.os.Looper
import dev.exact.canvas.AudioSource
import dev.exact.canvas.Cause
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import dev.exact.canvas.TrackConsumer
import exact.recording.RecordingModule
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [AudioTest.Host::class])
class AudioTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            var clock = 0.0
            var agentCapture = true
            lateinit var directory: File
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = name == "media-capture" && agentCapture
            @JvmStatic @Implementation fun changed(topic: String) {}
            @JvmStatic @Implementation fun now() = clock
            @JvmStatic @Implementation fun appFile(path: String) = File(directory, path.substringAfterLast('/')).path
        }
    }

    private class PCM : AudioSource {
        override val sampleRate = 48000
        override val channels = 1
        val taps = CopyOnWriteArrayList<(ShortArray, Int) -> Unit>()
        var onTap: () -> Unit = {}
        val thread = Executors.newSingleThreadExecutor()
        override fun tap(block: (ShortArray, Int) -> Unit): () -> Unit {
            onTap()
            taps += block
            return { taps -= block }
        }
        fun send(n: Int = 2400) = thread.submit {
            val samples = ShortArray(n + 17) { 16384 }
            taps.forEach { it(samples, n) }
        }.get(5, TimeUnit.SECONDS)
    }

    private class Probe : TrackConsumer {
        var leave: (() -> Unit)? = null
        var starts = 0
        val zero = ArrayList<Boolean>()
        val lengths = ArrayList<Int>()
        override fun live(source: AudioSource?) {
            starts++
            if (leave != null) return
            leave = source!!.tap { samples, n -> zero += (0 until n).all { samples[it] == 0.toShort() }; lengths += n }
        }
        override fun muted(reason: String) {}
        override fun unmuted() {}
        override fun ended() {}
    }

    private lateinit var capture: MediaCaptureModule
    private lateinit var recording: RecordingModule
    private lateinit var context: ExtensionContext
    private val pcm = PCM()

    @Before fun setUp() {
        Host.clock = 0.0
        Host.directory = File(RuntimeEnvironment.getApplication().cacheDir, "audio-tracks").apply { mkdirs() }
        context = ExtensionContext(RuntimeEnvironment.getApplication(), "media-capture")
        capture = MediaCaptureModule(context, pcm)
        recording = RecordingModule(ExtensionContext(RuntimeEnvironment.getApplication(), "recording"))
    }

    @After fun tearDown() {
        val recorders = ReflectionHelpers.getField<Map<String, Any>>(recording, "recorders")
        for ((id, recorder) in recorders) {
            if (ReflectionHelpers.getField<String>(recorder, "state") != "inactive") stop(id)
        }
        recording.release("audioLevel")
        recording.destroy {}
        capture.destroy {}
        pcm.thread.shutdown()
        assertTrue(pcm.thread.awaitTermination(5, TimeUnit.SECONDS))
        Host.directory.deleteRecursively()
    }

    private fun call(module: ExactExtension, name: String, args: JSONObject = JSONObject()): JSONObject {
        var answer: JSONObject? = null
        module.later(name, args, null, Cause("app")) { answer = it }
        return requireNotNull(answer)
    }

    private fun open(): Pair<String, String> {
        val stream = call(capture, "getUserMedia", JSONObject().put("audio", true))
        return stream.getString("id") to stream.getJSONArray("tracks").getJSONObject(0).getString("id")
    }

    private fun enable(track: String, on: Boolean) = call(capture, "setTrackEnabled", JSONObject().put("track", track).put("enabled", on))
    private fun level(track: String) = call(recording, "audioLevel", JSONObject().put("track", track))

    private fun stop(id: String): JSONObject {
        val recorders = ReflectionHelpers.getField<Map<String, Any>>(recording, "recorders")
        val writer = ReflectionHelpers.getField<Handler>(recorders.getValue(id), "writer")
        var answer: JSONObject? = null
        recording.later("stopMediaRecorder", JSONObject().put("recorder", id), null, Cause("app")) { answer = it }
        val drained = CountDownLatch(1)
        writer.post { drained.countDown() }
        assertTrue("the writer finishes", drained.await(5, TimeUnit.SECONDS))
        shadowOf(Looper.getMainLooper()).idle()
        return requireNotNull(answer)
    }

    @Test fun disabledTracksRecordSilenceAndKeepDurationWithoutMutingTheirSharedInput() {
        val (stream, track) = open()
        val (otherStream, otherTrack) = open()
        val starts = listOf(stream to "first", otherStream to "second").map { (id, name) ->
            val result = call(recording, "startMediaRecorder", JSONObject().put("stream", id)
                .put("path", "app:/data/$name.wav").put("mimeType", "audio/wav"))
            assertEquals("recording", result.getString("state"))
            result.getString("id") to name
        }
        val recorder = starts.first().first
        assertEquals("paused", call(recording, "pauseMediaRecorder", JSONObject().put("recorder", recorder)).getString("state"))
        assertEquals("recording", call(recording, "resumeMediaRecorder", JSONObject().put("recorder", recorder)).getString("state"))
        level(track); level(otherTrack)
        for ((step, enabled) in listOf(true, false, true).withIndex()) {
            enable(track, enabled)
            pcm.send()
            Host.clock += 50.0
            val read = level(track)
            assertEquals(if (enabled) 0.5 else 0.0, read.getDouble("audioLevel"), 0.0)
            assertEquals(0.5, level(otherTrack).getDouble("audioLevel"), 0.0)
            assertEquals(Host.clock / 1000, read.getDouble("totalSamplesDuration"), 1e-9)
            assertEquals((if (step == 0) 1 else step) * 0.0125, read.getDouble("totalAudioEnergy"), 1e-9)
        }
        for ((id, name) in starts) {
            assertEquals(150.0, stop(id).getDouble("duration"), 0.0)
            val bytes = File(Host.directory, "$name.wav").readBytes()
            assertEquals(44 + 7200 * 2, bytes.size)
            val samples = ByteBuffer.wrap(bytes, 44, bytes.size - 44).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
            for (step in 0..2) {
                val expected = if (name == "first" && step == 1) 0.toShort() else 16384.toShort()
                repeat(2400) { assertEquals("$name recorded step $step", expected, samples.get()) }
            }
        }
        recording.release("audioLevel")
        assertTrue(pcm.taps.isEmpty())
    }

    @Implements(android.content.pm.ServiceInfo::class)
    class MicrophoneService {
        @Implementation fun getForegroundServiceType() = android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
    }

    @Test
    @Config(shadows = [Host::class, MicrophoneService::class])
    fun aClaimKeepsTheCurrentCaptureWithoutAcquiringItAgain() {
        val service = android.content.pm.ServiceInfo().apply {
            packageName = context.application.packageName
            name = dev.exact.canvas.ForegroundService.Capture::class.java.name
        }
        shadowOf(context.application.packageManager).addOrUpdateService(service)
        val (_, track) = open()
        val leaveApp = context.tracks.subscribe(track, Probe(), true, Cause("app"))
        val reader = Probe()
        val leaveReader = context.tracks.subscribe(track, reader, false, Cause("action"))
        leaveApp()
        capture.foreground(false)
        capture.foreground(true)
        pcm.send()
        assertEquals(listOf(false), reader.zero)
        assertTrue(capture.leased(track))
        leaveReader()
        assertFalse(capture.leased(track))
    }

    @Test fun aSuspendedTrackDeliversNoSamplesUntilAnEligibleStart() {
        val (_, track) = open()
        val (_, otherTrack) = open()
        val leaveApp = context.tracks.subscribe(track, Probe(), true, Cause("app"))
        val listener = Probe()
        context.tracks.subscribe(track, listener, false, Cause("action"))
        capture.foreground(false)
        leaveApp()
        capture.foreground(true)
        assertFalse(capture.leased(track))
        val other = Probe()
        context.tracks.subscribe(otherTrack, other, true, Cause("app"))
        pcm.send()
        assertTrue(listener.lengths.isEmpty())
        assertEquals(listOf(false), other.zero)
        context.tracks.subscribe(track, Probe(), true, Cause("app"))
        assertTrue(capture.leased(track))
        assertFalse(call(capture, "mediaStreamTrack", JSONObject().put("track", track)).getBoolean("muted"))
        pcm.send()
        assertEquals(false, listener.zero.last())
    }

    @Test fun anIdleTrackClearsItsSuspensionBeforeTheNextRecorderStarts() {
        val (_, track) = open()
        val leave = context.tracks.subscribe(track, Probe(), true, Cause("app"))
        capture.foreground(false)
        leave()
        capture.foreground(true)
        context.tracks.subscribe(track, Probe(), true, Cause("app"))
        assertFalse(call(capture, "mediaStreamTrack", JSONObject().put("track", track)).getBoolean("muted"))
    }

    @Implements(MediaCaptureModule.Capture::class, isInAndroidSdk = false)
    class Microphone {
        companion object { var available = true }
        @Implementation fun lease(app: android.content.Context, options: JSONObject) { check(available) { "microphone busy" } }
        @Implementation fun unlease() {}
    }

    @Test
    @Config(shadows = [Host::class, Microphone::class])
    fun aFailedMicrophoneResumeCanAcquireOnTheNextReturn() {
        capture.destroy {}
        Host.agentCapture = false
        val app = RuntimeEnvironment.getApplication()
        shadowOf(app).grantPermissions(android.Manifest.permission.RECORD_AUDIO)
        context = ExtensionContext(app, "media-capture")
        capture = MediaCaptureModule(context, pcm)
        val (_, track) = open()
        val reader = Probe()
        try {
            val leave = context.tracks.subscribe(track, reader, true, Cause("app"))
            capture.foreground(false)
            Microphone.available = false
            capture.foreground(true)
            assertFalse(capture.leased(track))
            assertEquals("interrupted", call(capture, "mediaStreamTrack", JSONObject().put("track", track)).getString("muteReason"))
            assertEquals(1, reader.starts)
            Microphone.available = true
            capture.foreground(true)
            assertTrue(capture.leased(track))
            assertEquals(2, reader.starts)
            pcm.send()
            assertEquals(listOf(false), reader.zero)
            leave()
        } finally { Host.agentCapture = true }
    }

    @Test fun aFailedRecordingAnswersItsErrorWhenStopped() {
        val (stream, _) = open()
        val id = call(recording, "startMediaRecorder", JSONObject().put("stream", stream)
            .put("path", "app:/data/failed.wav").put("mimeType", "audio/wav")).getString("id")
        val take = ReflectionHelpers.getField<Map<String, exact.recording.MediaTake>>(recording, "recorders").getValue(id)
        take.finish("error", "capture failed") {}
        assertEquals("capture failed", stop(id).getString("message"))
    }

    @Test fun restoringTheWaveformPropWaitsForItsNewBinding() {
        val (_, track) = open()
        val props = JSONObject().put("track", track)
        val view = recording.view("audio-waveform", "s", "wave", context.application, props) { _, _ -> }
        try {
            recording.bound("s", "wave", "track", track, Cause("app"))
            recording.displayed("s", "wave", true, Cause("app"))
            assertEquals(1, pcm.taps.size)
            recording.update(view, JSONObject())
            recording.update(view, props)
            assertTrue(pcm.taps.isEmpty())
            recording.bound("s", "wave", "track", track, Cause("action"))
            assertFalse(capture.leased(track))
        } finally { recording.destroyView(view) }
    }

    @Test fun aRefusedSubscriptionLeavesThePreviousRecordingAndNoTap() {
        val (stream, track) = open()
        val previous = File(Host.directory, "previous.wav").apply { writeText("previous recording") }
        lateinit var writer: Thread
        context.tracks.add(dev.exact.canvas.TrackEntry(track, "audio", owner = object : dev.exact.canvas.TrackOwner {
            override fun subscribe(id: String, consumer: TrackConsumer, start: Boolean, cause: Cause): () -> Unit {
                writer = ReflectionHelpers.getField(consumer, "thread")
                consumer.live(pcm)
                throw dev.exact.canvas.TrackError("NotReadableError")
            }
        }))
        val answer = call(recording, "startMediaRecorder", JSONObject().put("stream", stream)
            .put("path", "app:/data/previous.wav").put("mimeType", "audio/wav"))
        assertEquals("NotReadableError", answer.getString("error"))
        writer.join(5_000)
        assertFalse(writer.isAlive)
        assertTrue(pcm.taps.isEmpty())
        assertEquals("previous recording", previous.readText())
    }

    @Implements(dev.exact.canvas.Exact::class, isInAndroidSdk = false)
    class Press {
        companion object { var live = true }
        @Implementation fun pressLive(id: Long) = id == 7L && live
    }

    @Test
    @Config(shadows = [Host::class, Press::class])
    fun aPressExpiringWhileTheRecorderStartsRollsBackOnlyItsOwnLease() {
        dev.exact.canvas.Extensions.started()
        val previous = File(Host.directory, "press.wav").apply { writeText("previous recording") }
        for (held in listOf(false, true)) {
            val (stream, track) = open()
            val passive = object : TrackConsumer {
                override fun muted(reason: String) {}
                override fun unmuted() {}
                override fun ended() {}
            }
            val leave = context.tracks.subscribe(track, passive, held, Cause(if (held) "app" else "action"))
            Press.live = true
            pcm.onTap = { Press.live = false }
            val args = JSONObject().put("stream", stream).put("path", "app:/data/press.wav").put("mimeType", "audio/wav")
            var answer: JSONObject? = null
            recording.later("startMediaRecorder", args, null, Cause("press", press = 7)) { answer = it }
            assertEquals("NotAllowedError", answer?.optString("error"))
            assertFalse(Press.live)
            assertEquals(held, capture.leased(track))
            assertEquals("previous recording", previous.readText())
            assertTrue(pcm.taps.isEmpty())
            leave()
        }
    }

    @Test fun lateSubscriptionsCancellationAndAudioThreadFlagChangesAreIndependent() {
        val (_, track) = open()
        val (_, otherTrack) = open()
        val probe = Probe()
        val other = Probe()
        val leave = context.tracks.subscribe(track, probe, true, Cause("app"))
        val leaveOther = context.tracks.subscribe(otherTrack, other, true, Cause("app"))
        enable(track, false)
        val late = Probe()
        val leaveLate = context.tracks.subscribe(track, late, false, Cause("app"))
        pcm.send(8192)
        assertEquals(listOf(true), probe.zero)
        assertEquals(listOf(true), late.zero)
        assertEquals(listOf(8192), late.lengths)
        assertEquals(listOf(false), other.zero)
        late.leave!!(); leaveLate()

        val ready = CountDownLatch(1)
        val proceed = CountDownLatch(1)
        val delivered = pcm.thread.submit {
            ready.countDown()
            check(proceed.await(5, TimeUnit.SECONDS))
            val samples = ShortArray(2400) { 16384 }
            pcm.taps.forEach { it(samples, samples.size) }
        }
        assertTrue(ready.await(5, TimeUnit.SECONDS))
        enable(track, true)
        proceed.countDown()
        delivered.get(5, TimeUnit.SECONDS)
        assertFalse(probe.zero.last())

        val racing = CountDownLatch(1)
        val audio = pcm.thread.submit {
            racing.countDown()
            val samples = ShortArray(2400) { 16384 }
            repeat(1000) { pcm.taps.forEach { it(samples, samples.size) } }
        }
        assertTrue(racing.await(5, TimeUnit.SECONDS))
        repeat(1000) { enable(track, it % 2 == 0) }
        audio.get(5, TimeUnit.SECONDS)
        enable(track, false)
        pcm.send()
        assertTrue(probe.zero.last())

        val stale = pcm.taps.toList()
        probe.leave!!(); probe.leave!!()
        val count = probe.zero.size
        pcm.thread.submit { stale.forEach { it(ShortArray(2400) { 16384 }, 2400) } }.get(5, TimeUnit.SECONDS)
        assertEquals(count, probe.zero.size)
        leave(); leave()
        assertEquals(1, pcm.taps.size)
        call(capture, "stopTrack", JSONObject().put("track", otherTrack))
        assertTrue(pcm.taps.isEmpty())
        leaveOther()

        val (_, lastTrack) = open()
        val last = Probe()
        val leaveLast = context.tracks.subscribe(lastTrack, last, true, Cause("app"))
        last.leave!!(); leaveLast()
        assertTrue(pcm.taps.isEmpty())
        val replacement = Probe()
        val leaveReplacement = context.tracks.subscribe(lastTrack, replacement, true, Cause("app"))
        pcm.send()
        assertEquals(listOf(false), replacement.zero)
        capture.destroy {}
        assertTrue(pcm.taps.isEmpty())
        leaveReplacement()
    }
}
