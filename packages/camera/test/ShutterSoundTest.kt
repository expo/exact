package exact.camera

import android.media.MediaActionSound
import android.os.Build
import androidx.camera.core.ImageCapture
import androidx.camera.core.ImageCaptureException
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import dev.exact.canvas.TrackConsumer
import dev.exact.canvas.TrackEntry
import dev.exact.canvas.TrackOwner
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
import java.io.File
import java.util.concurrent.Executor

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31, 32, 33], manifest = Config.NONE,
    shadows = [ShutterSoundTest.Host::class, ShutterSoundTest.Sound::class, ShutterSoundTest.Capture::class])
class ShutterSoundTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun appFile(path: String) =
                File(RuntimeEnvironment.getApplication().cacheDir, path.removePrefix("app:/")).path
        }
    }

    @Implements(MediaActionSound::class)
    class Sound {
        companion object {
            var required = false
            var queries = 0
            val played = ArrayList<Int>()
            @JvmStatic @Implementation(minSdk = 33) fun mustPlayShutterSound(): Boolean {
                queries++
                return required
            }
        }
        @Implementation fun load(name: Int) {}
        @Implementation fun play(name: Int) { played += name }
    }

    /** Stops at CameraX's boundary: no device is opened, but the real takePhoto path must reach it. */
    @Implements(ImageCapture::class, isInAndroidSdk = false)
    class Capture {
        companion object { var shots = 0 }
        @Implementation fun takePicture(options: ImageCapture.OutputFileOptions, executor: Executor,
            callback: ImageCapture.OnImageSavedCallback) {
            shots++
            callback.onError(ImageCaptureException(ImageCapture.ERROR_CAPTURE_FAILED, "test capture", null))
        }
    }

    private class Source : VideoSource {
        override val camera: Any? = null
        override fun add(useCase: Any) {}
        override fun attach(useCase: Any) = true
        override fun remove(useCase: Any) {}
        override fun <T : Any> shared(key: String, make: () -> T): T = make()
        override fun apply(constraints: JSONObject) {}
    }

    @Before fun reset() {
        Sound.required = false
        Sound.queries = 0
        Sound.played.clear()
        Capture.shots = 0
    }

    @After fun leave() {
        ExtensionContext(RuntimeEnvironment.getApplication(), "camera").tracks.remove("video")
    }

    private fun take(shutterSound: Boolean? = null) {
        val context = ExtensionContext(RuntimeEnvironment.getApplication(), "camera")
        var leases = 0
        val owner = object : TrackOwner {
            override fun subscribe(id: String, consumer: TrackConsumer, start: Boolean, origin: String): () -> Unit {
                leases++
                return { leases-- }
            }
        }
        context.tracks.add(TrackEntry("video", "video", owner = owner).apply {
            state = { JSONObject().put("readyState", "live").put("enabled", true).put("muted", false) }
            video = { Source() }
        })
        val args = JSONObject().put("track", "video")
        shutterSound?.let { args.put("android", JSONObject().put("shutterSound", it)) }
        var answer: JSONObject? = null
        CameraModule(context).later("takePhoto", args) { answer = it }
        assertEquals(1, Capture.shots)
        assertEquals("UnknownError", answer?.optString("error"))
        assertEquals(0, leases)
    }

    @Test fun suppressionNeedsThePlatformsPermission() {
        take(false)
        assertEquals(if (Build.VERSION.SDK_INT < 33) listOf(MediaActionSound.SHUTTER_CLICK) else emptyList<Int>(), Sound.played)
        assertEquals(if (Build.VERSION.SDK_INT < 33) 0 else 1, Sound.queries)
    }

    @Test fun aRegionalRequirementKeepsTheSound() {
        Sound.required = true
        take(false)
        assertEquals(listOf(MediaActionSound.SHUTTER_CLICK), Sound.played)
        assertEquals(if (Build.VERSION.SDK_INT < 33) 0 else 1, Sound.queries)
    }

    @Test fun theDefaultAndAnExplicitSoundNeedNoMandateQuery() {
        take()
        assertEquals(listOf(MediaActionSound.SHUTTER_CLICK), Sound.played)
        assertEquals(0, Sound.queries)
        Capture.shots = 0
        take(true)
        assertEquals(listOf(MediaActionSound.SHUTTER_CLICK, MediaActionSound.SHUTTER_CLICK), Sound.played)
        assertEquals(0, Sound.queries)
    }
}
