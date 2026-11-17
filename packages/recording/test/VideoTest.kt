package exact.recording

import android.Manifest
import android.content.pm.PackageManager
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.util.Range
import android.util.Size
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.UseCase
import androidx.camera.core.impl.StreamSpec
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.video.AudioStats
import androidx.camera.video.OutputOptions
import androidx.camera.video.OutputResults
import androidx.camera.video.PendingRecording
import androidx.camera.video.Recorder
import androidx.camera.video.Recording
import androidx.camera.video.RecordingStats
import androidx.camera.video.VideoRecordEvent
import androidx.core.util.Consumer
import dev.exact.canvas.ExactActivity
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Extensions
import dev.exact.canvas.Native
import dev.exact.canvas.TrackConsumer
import dev.exact.canvas.TrackEntry
import dev.exact.canvas.TrackError
import dev.exact.canvas.TrackOwner
import dev.exact.canvas.VideoSource
import exact.mediacapture.CameraCapture
import exact.mediacapture.FrameRateTest
import exact.mediacapture.MediaCaptureModule
import java.io.File
import org.json.JSONObject
import org.junit.Assert.*
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
import org.robolectric.shadows.ShadowMediaMetadataRetriever
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter.from

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [VideoTest.Host::class])
class VideoTest {
    @Implements(Recorder::class, isInAndroidSdk = false)
    class Encoder {
        @Implementation fun start(pending: PendingRecording): Recording {
            prepared = pending
            return ReflectionHelpers.callStaticMethod<Recording>(Recording::class.java, "from",
                from(PendingRecording::class.java, pending), from(Long::class.javaPrimitiveType, 1L)).also { active = it }
        }
        @Implementation fun stop(recording: Recording, error: Int, cause: Throwable?) { stops++ }
        companion object {
            var prepared: PendingRecording? = null
            var active: Recording? = null
            var stops = 0
        }
    }

    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            var agent = true
            @JvmStatic @Implementation fun underAgent(name: String) = agent
            var time = 0.0
            var next = 0L
            val timers = LinkedHashMap<Long, Double>()
            @JvmStatic @Implementation fun now() = time
            @JvmStatic @Implementation fun hatchAct(node: Int, kind: Int, text: String?): Long {
                assertEquals(0, node)
                return when (kind) {
                    6 -> (++next).also { timers[it] = time + text!!.toDouble() }
                    8 -> { timers.remove(text!!.toLong()); 1L }
                    else -> error("unexpected hatch act $kind")
                }
            }
            @JvmStatic @Implementation fun changed(topic: String) {}
            @JvmStatic @Implementation fun appFile(path: String) = File(RuntimeEnvironment.getApplication().cacheDir, path.substringAfterLast('/')).path
        }
    }

    @Test fun aCameraOnlyStreamRecordsWithoutOpeningAnotherCamera() {
        val context = ExtensionContext(RuntimeEnvironment.getApplication(), "recording")
        var consumers = 0
        val owner = object : TrackOwner {
            override fun subscribe(id: String, consumer: TrackConsumer, start: Boolean, origin: String): () -> Unit {
                consumers++
                consumer.liveVideo(null)
                return { consumers-- }
            }
        }
        context.tracks.add(TrackEntry("video-test-camera", "video", owner = owner))
        context.tracks.add(TrackEntry("video-test-stream", "stream", listOf("video-test-camera")))
        val module = RecordingModule(context)
        fun call(name: String, args: JSONObject): JSONObject {
            var answer: JSONObject? = null
            module.later(name, args) { answer = it }
            return requireNotNull(answer)
        }
        val start = call("startMediaRecorder", JSONObject().put("stream", "video-test-stream").put("path", "app:/data/video-test"))
        assertEquals("recording", start.optString("state"))
        assertEquals(1, consumers)
        val result = call("stopMediaRecorder", JSONObject().put("recorder", start.getString("id")))
        assertEquals("video/mp4", result.getString("mimeType"))
        val file = context.file(result.getString("path"))!!
        assertEquals(file.length(), result.getLong("size"))
        assertEquals("ftyp", file.readBytes().copyOfRange(4, 8).toString(Charsets.US_ASCII))
        assertEquals(16, result.getInt("width"))
        assertEquals(16, result.getInt("height"))
        assertEquals(0, consumers)
        file.delete()
        context.tracks.remove("video-test-camera"); context.tracks.remove("video-test-stream")
    }

    @Test fun anUnsupportedCombinationIsRefusedThroughTheRestoringAttachPoint() {
        Host.agent = false
        try {
            val context = ExtensionContext(RuntimeEnvironment.getApplication(), "recording")
            val file = File(context.application.cacheDir, "unsupported.mp4")
            var attempts = 0
            val source = object : VideoSource {
                override val camera: Any? = null
                override fun add(useCase: Any) { fail("recording must use the restoring attach point") }
                override fun attach(useCase: Any): Boolean { attempts++; return false }
                override fun remove(useCase: Any) { fail("the rejected output was never attached") }
                override fun <T : Any> shared(key: String, make: () -> T): T = make()
                override fun apply(constraints: JSONObject) {}
            }
            val take = VideoTake("unsupported", "app:/data/unsupported.mp4", file, context, 0.0, {})
            try { take.liveVideo(source); fail("the unsupported combination must refuse") }
            catch (e: TrackError) { assertEquals("NotSupportedError", e.code) }
            assertEquals(1, attempts)
            assertFalse(file.exists())
        } finally { Host.agent = true }
    }

    @Test
    @Config(shadows = [Host::class, Encoder::class])
    fun aTakeOptsIntoPersistentRecordingAndStopsWhenItsTrackEnds() {
        Host.agent = false
        Encoder.prepared = null; Encoder.active = null; Encoder.stops = 0
        val context = ExtensionContext(RuntimeEnvironment.getApplication(), "recording")
        val file = File(context.application.cacheDir, "persistent.mp4")
        val take = VideoTake("persistent", "app:/data/persistent.mp4", file, context, 5000.0, {})
        try {
            val source = object : VideoSource {
                override val camera: Any? = null
                override fun add(useCase: Any) { fail("use attach") }
                override fun attach(useCase: Any) = true
                override fun remove(useCase: Any) {}
                override fun <T : Any> shared(key: String, make: () -> T): T = make()
                override fun apply(constraints: JSONObject) {}
            }
            take.liveVideo(source)
            val pending = requireNotNull(Encoder.prepared)
            // CameraX keeps a persistent recording across the shared session's unbind/rebind.
            assertTrue("analysis changes must not finalize the take", requireNotNull(Encoder.active).isPersistent)
            val options = ReflectionHelpers.callInstanceMethod<OutputOptions>(pending, "getOutputOptions")
            assertEquals(5000L, options.durationLimitMillis)
            assertEquals(0, Encoder.stops)
            take.ended()
            assertEquals(1, Encoder.stops)
            assertEquals("track-ended", take.info().getString("reason"))
        } finally {
            take.ended()
            Host.agent = true
            Encoder.prepared = null
            Encoder.active = null
            file.delete()
        }
    }

    @Test fun theDurationLimitExcludesPausedTimeAndAnswersOneFile() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        val front = Extensions.front
        Extensions.front = { activity }
        val context = ExtensionContext(RuntimeEnvironment.getApplication(), "recording")
        val file = File(context.application.cacheDir, "video-limit.mp4")
        try {
            val take = VideoTake("video-limit", "app:/data/video-limit.mp4", file, context, 5000.0, {})
            take.arm()
            assertEquals(listOf(5000.0), Host.timers.values.toList())
            Host.time = 2000.0; take.pause("requested")
            assertTrue(Host.timers.isEmpty())
            Host.time = 12000.0; take.resume()
            assertEquals(listOf(15000.0), Host.timers.values.toList())
            val token = Host.timers.keys.single()
            Host.time = 15000.0; Host.timers.remove(token)
            activity.hatches()!!.told(intArrayOf(6, token.toInt()), 0, 2)
            assertEquals("max-duration", take.info().getString("reason"))
            assertEquals("inactive", take.state)
            var answer: JSONObject? = null
            take.finish("requested", null) { answer = it }
            take.finish("requested", null) { assertSame(answer, it) }
            assertEquals(300, answer!!.getInt("duration"))
            assertEquals(file.length(), answer.getLong("size"))
            assertTrue(file.length() > 0)
        } finally {
            Extensions.front = front
            Host.time = 0.0; Host.timers.clear()
            file.delete()
        }
    }

    @Test
    @Config(shadows = [Host::class, Encoder::class, FrameRateTest.Provider::class,
        FrameRateTest.Camera2Info::class, FrameRateTest.Camera2Factory::class])
    fun disablingAndReenablingTheSoleTrackReattachesTheSameRecordingBesidePreviewAndAnalysis() {
        Host.agent = false
        Encoder.prepared = null; Encoder.active = null; Encoder.stops = 0
        val app = RuntimeEnvironment.getApplication()
        shadowOf(app.packageManager).setSystemFeature(PackageManager.FEATURE_CAMERA_ANY, true)
        shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).requestedPermissions = arrayOf(Manifest.permission.CAMERA)
        shadowOf(app).grantPermissions(Manifest.permission.CAMERA)
        ReflectionHelpers.setField(CameraCapture, "ready", Shadow.newInstanceOf(ProcessCameraProvider::class.java))
        val context = ExtensionContext(app, "recording")
        val media = MediaCaptureModule(context)
        fun call(name: String, args: JSONObject): JSONObject {
            var answer: JSONObject? = null
            media.later(name, args) { answer = it }
            return requireNotNull(answer)
        }
        val take = VideoTake("reenabled", "app:/data/reenabled.mp4", File(app.cacheDir, "reenabled.mp4"), context, 5000.0, {})
        try {
            val opened = call("getUserMedia", JSONObject().put("video", true).put("videoConstraints", JSONObject().put("frameRate", 30)))
            val id = opened.getJSONArray("tracks").getJSONObject(0).getString("id")
            val preview = Preview.Builder().build()
            val analysis = ImageAnalysis.Builder().build()
            context.tracks.subscribe(id, object : TrackConsumer {
                override fun liveVideo(source: VideoSource?) { source!!.add(preview); source.add(analysis) }
                override fun muted(reason: String) {}
                override fun unmuted() {}
                override fun ended() {}
            }, false, "app")
            take.leave = context.tracks.subscribe(id, take, true, "app")
            val original = requireNotNull(Encoder.active)
            val cases = requireNotNull(FrameRateTest.Provider.bound).useCases.toSet()
            assertEquals(3, cases.size)
            repeat(2) {
                call("setTrackEnabled", JSONObject().put("track", id).put("enabled", false))
                assertNull(FrameRateTest.Provider.bound)
                call("setTrackEnabled", JSONObject().put("track", id).put("enabled", true))
                assertEquals(cases, FrameRateTest.Provider.bound?.useCases?.toSet())
                assertSame(original, Encoder.active)
                assertEquals("recording", take.state)
                assertEquals(0, Encoder.stops)
            }
            val rival = VideoTake("rival", "app:/data/rival.mp4", File(app.cacheDir, "rival.mp4"), context, 0.0, {})
            try { rival.liveVideo(CameraCapture); fail("the reacquired capture must keep its recorder slot") }
            catch (e: TrackError) { assertEquals("NotReadableError", e.code) }
            call("stopTrack", JSONObject().put("track", id))
            assertEquals(1, Encoder.stops)
        } finally {
            media.destroy {}
            ReflectionHelpers.setField(CameraCapture, "ready", null)
            Host.agent = true
            Encoder.prepared = null; Encoder.active = null
        }
    }

    @Test
    @Config(shadows = [Host::class, Encoder::class])
    fun aRejectedReattachmentStopsThePersistentTake() {
        Host.agent = false
        Encoder.stops = 0
        var supported = true
        val source = object : VideoSource {
            override val camera: Any? = null
            override fun add(useCase: Any) { fail("use attach") }
            override fun attach(useCase: Any) = supported
            override fun remove(useCase: Any) {}
            override fun <T : Any> shared(key: String, make: () -> T): T = make()
            override fun apply(constraints: JSONObject) {}
        }
        val context = ExtensionContext(RuntimeEnvironment.getApplication(), "recording")
        val take = VideoTake("rejected", "app:/data/rejected.mp4", File(context.application.cacheDir, "rejected.mp4"), context, 5000.0, {})
        try {
            take.liveVideo(source)
            supported = false
            take.muted("idle")
            take.liveVideo(source)
            assertEquals("inactive", take.state)
            assertEquals("error", take.info().getString("reason"))
            assertEquals(1, Encoder.stops)
        } finally {
            take.ended()
            Host.agent = true
            Encoder.prepared = null; Encoder.active = null
        }
    }

    @Test fun stoppingBeforeTheCameraSuppliesVideoAnswersNotReadable() {
        Host.agent = false
        val context = ExtensionContext(RuntimeEnvironment.getApplication(), "recording")
        val file = File(context.application.cacheDir, "empty.mp4")
        try {
            val take = VideoTake("empty", "app:/data/empty.mp4", file, context, 5000.0, {})
            var released = false
            take.leave = { released = true }
            var result: JSONObject? = null
            take.finish("requested", null) { result = it }
            assertEquals("NotReadableError", result!!.getString("error"))
            assertEquals("the camera supplied no video", result.getString("message"))
            assertTrue(released)
            assertEquals("inactive", take.state)
            assertFalse(file.exists())
            take.finish("requested", null) { assertSame(result, it) }
        } finally { Host.agent = true }
    }

    @Test
    @Config(shadows = [Host::class, Encoder::class])
    fun theFinishedFileNeverFallsBackToTheCaptureRate() {
        Host.agent = false
        val context = ExtensionContext(RuntimeEnvironment.getApplication(), "recording")
        try {
            val results = ArrayList<JSONObject>()
            for (initial in listOf(Range(0, 0), Range(15, 15))) {
                val file = File(context.application.cacheDir, "rate-${initial.upper}.mp4")
                ShadowMediaMetadataRetriever.addMetadata(file.path, MediaMetadataRetriever.METADATA_KEY_DURATION, "5000")
                var range = initial
                val objects = HashMap<String, Any>()
                val source = object : VideoSource {
                    override val camera: Any? = null
                    override fun add(useCase: Any) { fail("use attach") }
                    override fun attach(useCase: Any): Boolean {
                        ReflectionHelpers.setField(useCase as UseCase, "mAttachedStreamSpec",
                            StreamSpec.builder(Size(1280, 720)).setExpectedFrameRateRange(range).build())
                        return true
                    }
                    override fun remove(useCase: Any) {}
                    @Suppress("UNCHECKED_CAST")
                    override fun <T : Any> shared(key: String, make: () -> T): T = objects.getOrPut(key, make) as T
                    override fun apply(constraints: JSONObject) {}
                }
                val take = VideoTake("rate", "app:/data/rate.mp4", file, context, 5000.0, {})
                take.liveVideo(source)
                // A session rebind may expose an unspecified stream spec after a real rate was negotiated.
                for (unspecified in listOf(Range(0, 60), Range(0, 0))) {
                    range = unspecified
                    take.liveVideo(source)
                }
                val pending = requireNotNull(Encoder.prepared)
                val options = ReflectionHelpers.callInstanceMethod<OutputOptions>(pending, "getOutputOptions")
                val listener = ReflectionHelpers.callInstanceMethod<Consumer<VideoRecordEvent>>(pending, "getEventListener")
                val audio = ReflectionHelpers.callStaticMethod<AudioStats>(AudioStats::class.java, "of",
                    from(Int::class.javaPrimitiveType, AudioStats.AUDIO_STATE_DISABLED), from(Throwable::class.java, null),
                    from(Double::class.javaPrimitiveType, 0.0), from(Long::class.javaPrimitiveType, 0L))
                val stats = ReflectionHelpers.callStaticMethod<RecordingStats>(RecordingStats::class.java, "of",
                    from(Long::class.javaPrimitiveType, 5_000_000_000L), from(Long::class.javaPrimitiveType, 100L),
                    from(AudioStats::class.java, audio))
                val output = ReflectionHelpers.callStaticMethod<OutputResults>(OutputResults::class.java, "of", from(Uri::class.java, Uri.EMPTY))
                listener.accept(ReflectionHelpers.callStaticMethod(VideoRecordEvent::class.java, "finalize",
                    from(OutputOptions::class.java, options), from(RecordingStats::class.java, stats), from(OutputResults::class.java, output)))
                results += take.info().getJSONObject("result")
            }
            assertEquals(listOf(null, null), results.map { if (it.has("frameRate")) it.getInt("frameRate") else null })
        } finally {
            Host.agent = true
            Encoder.prepared = null; Encoder.active = null
        }
    }

    @Test fun videoMetadataUsesTheRotatedPixelSize() {
        val file = File(RuntimeEnvironment.getApplication().cacheDir, "portrait.mp4")
        fun fact(key: Int, value: String) = ShadowMediaMetadataRetriever.addMetadata(file.path, key, value)
        fact(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH, "1280")
        fact(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT, "720")
        fact(MediaMetadataRetriever.METADATA_KEY_DURATION, "5000")
        fact(MediaMetadataRetriever.METADATA_KEY_CAPTURE_FRAMERATE, "30")
        for (rotation in listOf(0, 90, 180, 270)) {
            fact(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION, "$rotation")
            val metadata = VideoTake.metadata(file)
            assertEquals(if (rotation % 180 == 90) 720 else 1280, metadata.getInt("width"))
            assertEquals(if (rotation % 180 == 90) 1280 else 720, metadata.getInt("height"))
            assertEquals(5000, metadata.getInt("duration"))
            assertEquals(30, metadata.getInt("frameRate"))
        }
        fact(MediaMetadataRetriever.METADATA_KEY_CAPTURE_FRAMERATE, "0")
        assertFalse(VideoTake.metadata(file).has("frameRate"))
        fact(MediaMetadataRetriever.METADATA_KEY_VIDEO_FRAME_COUNT, "120")
        fact(MediaMetadataRetriever.METADATA_KEY_CAPTURE_FRAMERATE, "60")
        assertEquals(24, VideoTake.metadata(file).getInt("frameRate"))
    }
}
