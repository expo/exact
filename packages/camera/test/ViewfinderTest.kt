package exact.camera

import android.view.View
import android.widget.ImageView
import androidx.camera.core.Preview
import androidx.camera.view.PreviewView
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import dev.exact.canvas.TrackConsumer
import dev.exact.canvas.TrackEntry
import dev.exact.canvas.TrackOwner
import dev.exact.canvas.VideoSource
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ViewfinderTest.Host::class])
class ViewfinderTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            var agent = false
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = agent
        }
    }

    private class Source : VideoSource {
        override val camera: Any? = null
        lateinit var preview: Preview
        var attachments = 0
        override fun add(useCase: Any) { preview = useCase as Preview; attachments++ }
        override fun remove(useCase: Any) {}
        override fun <T : Any> shared(key: String, make: () -> T): T = make()
        override fun apply(constraints: JSONObject) {}
    }

    /** Delivers the already-leased track's callbacks, including a subscription made while backgrounded. */
    private class Track(private val source: VideoSource?) : TrackOwner {
        val consumers = LinkedHashSet<TrackConsumer>()
        var background = false
            set(value) {
                field = value
                consumers.toList().forEach { if (value) it.muted("background") else it.unmuted() }
            }
        var liveCallbacks = 0
        override fun leased(id: String) = consumers.isNotEmpty()
        override fun subscribe(id: String, consumer: TrackConsumer, start: Boolean, origin: String): () -> Unit {
            consumers += consumer
            liveCallbacks++
            consumer.liveVideo(source)
            if (background) consumer.muted("background")
            return { consumers -= consumer }
        }
    }

    private class Fixture(source: VideoSource?) {
        val context = ExtensionContext(RuntimeEnvironment.getApplication(), "camera")
        val module = CameraModule(context)
        val track = Track(source)
        init { addTrack("track") }
        fun addTrack(id: String) { context.tracks.add(TrackEntry(id, "video", owner = track)) }
        fun view(): View = module.view("exact-camera", "1", "finder", context.application,
            JSONObject().put("srcObject", "track")) { _, _ -> }
        fun show(): View = view().also {
            module.bound("1", "finder", "srcObject", "app")
            module.displayed("1", "finder", true, "app")
        }
        fun destroy(view: View) = module.destroyView(view)
    }

    @Test fun aViewfinderRecreatedWhileAnotherConsumerHoldsTheLeaseRestoresItsPreviewOnUnmute() {
        val source = Source()
        val f = Fixture(source)
        val first = f.show() as PreviewView
        assertSame(first.surfaceProvider, source.preview.surfaceProvider)
        val photo = object : TrackConsumer {
            override fun muted(reason: String) {}
            override fun unmuted() {}
            override fun ended() {}
        }
        val finishPhoto = f.context.tracks.subscribe("track", photo, false)
        f.track.background = true
        assertNull(source.preview.surfaceProvider)
        f.destroy(first)
        assertTrue(f.track.leased("track"))
        val replacement = f.view() as PreviewView
        assertEquals(2, f.track.consumers.size)
        assertNull(source.preview.surfaceProvider)
        val callbacks = f.track.liveCallbacks
        f.track.background = false
        assertEquals(callbacks, f.track.liveCallbacks)
        assertSame(replacement.surfaceProvider, source.preview.surfaceProvider)
        finishPhoto()
        f.destroy(replacement)
    }

    @Test fun endedDestroyedAndReplacedSubscriptionsCannotRestoreOrClearTheCurrentPreview() {
        val source = Source()
        val f = Fixture(source)
        val first = f.show()
        val destroyed = f.track.consumers.single()
        destroyed.muted("background")
        f.destroy(first)
        destroyed.unmuted()
        assertNull(source.preview.surfaceProvider)
        val current = f.view() as PreviewView
        val replaced = f.track.consumers.single()
        f.addTrack("other-track")
        f.module.update(current, JSONObject().put("srcObject", "other-track"))
        f.module.bound("1", "finder", "srcObject", "app")
        val active = f.track.consumers.single()
        for (stale in listOf(destroyed, replaced)) {
            stale.muted("background")
            stale.ended()
            assertSame(current.surfaceProvider, source.preview.surfaceProvider)
            active.muted("background")
            stale.liveVideo(source)
            stale.unmuted()
            assertNull(source.preview.surfaceProvider)
            active.unmuted()
            assertSame(current.surfaceProvider, source.preview.surfaceProvider)
        }
        active.ended()
        assertNull(source.preview.surfaceProvider)
        val attachments = source.attachments
        active.unmuted()
        assertEquals(attachments, source.attachments)
        assertNull(source.preview.surfaceProvider)
        f.destroy(current)
    }

    @Test fun theAgentsNullVideoSourceRestoresItsTestCardOnUnmute() {
        Host.agent = true
        try {
            val f = Fixture(null)
            val view = f.show() as ImageView
            assertNotNull(view.drawable)
            f.track.background = true
            assertNull(view.drawable)
            f.track.background = false
            assertNotNull(view.drawable)
            val ended = f.track.consumers.single()
            ended.ended()
            ended.unmuted()
            assertNull(view.drawable)
            f.destroy(view)
        } finally {
            Host.agent = false
        }
    }
}
