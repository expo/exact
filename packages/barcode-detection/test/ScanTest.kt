package exact.barcodedetection

import android.os.Looper
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import dev.exact.canvas.VideoSource
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
import java.time.Duration

/** The poll controller, with camera delivery replaced at the completion boundary. No ML Kit model runs here. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ScanTest.Host::class])
class ScanTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            var clock = 100.0
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
            @JvmStatic @Implementation fun changed(topic: String) {}
            @JvmStatic @Implementation fun now() = clock
        }
    }

    private lateinit var module: BarcodeDetectionModule
    private lateinit var scan: Scan
    private val source = object : VideoSource {
        override val camera: Any? = null
        override fun add(useCase: Any) {}
        override fun attach(useCase: Any) = true
        override fun remove(useCase: Any) {}
        override fun <T : Any> shared(key: String, make: () -> T): T = make()
        override fun apply(constraints: JSONObject) {}
    }

    @Before fun setUp() {
        Host.clock = 100.0
        module = BarcodeDetectionModule(ExtensionContext(RuntimeEnvironment.getApplication(), "barcode-detection"))
        scan = Scan(module, "test-track", null)
        scan.liveVideo(null)
        ReflectionHelpers.setField(scan, "source", source)
        ReflectionHelpers.setField(scan, "state", "scanning")
    }
    @After fun tearDown() { scan.stop(); module.destroy {} }

    private fun pending(): Any? = ReflectionHelpers.getField<Any?>(scan, "pending")
    private fun code(value: String) = JSONObject().put("format", "qr_code").put("rawValue", value)
    private fun complete(poll: Any, codes: List<JSONObject>) {
        // Result's successful JVM representation is its value.
        scan.javaClass.declaredMethods.single { it.name == "complete" }.apply { isAccessible = true }
            .invoke(scan, poll, codes, 1440, 1080)
    }

    @Test fun concurrentPollsShareAFrameAndHistoryCountsFramesRatherThanCopies() {
        val answers = ArrayList<JSONObject>()
        scan.analyse { answers += it }
        val poll = requireNotNull(pending())
        scan.analyse { answers += it }
        assertSame(poll, pending())
        complete(poll, listOf(code("same"), code("same")))
        assertEquals(2, answers.size)
        assertEquals(1, answers.last().getInt("frame"))
        assertEquals(1, answers.last().getJSONArray("seen").getJSONObject(0).getInt("frames"))
        Host.clock = 200.0
        scan.analyse { answers += it }
        complete(requireNotNull(pending()), listOf(code("same")))
        val seen = answers.last().getJSONArray("seen").getJSONObject(0)
        assertEquals(100.0, seen.getDouble("firstSeen"), 0.0)
        assertEquals(200.0, seen.getDouble("lastSeen"), 0.0)
        assertEquals(2, seen.getInt("frames"))
        scan.analyse { answers += it }
        complete(requireNotNull(pending()), (0 until 33).map { code("code-$it") })
        val kept = answers.last().getJSONArray("seen")
        assertEquals(32, kept.length())
        assertEquals("code-32", kept.getJSONObject(0).getString("rawValue"))
        assertEquals("code-1", kept.getJSONObject(31).getString("rawValue"))
        assertEquals(1440, answers.last().getInt("width"))
        assertEquals(1080, answers.last().getInt("height"))
    }

    @Test fun muteEndAndReleaseAnswerWaitingPollsAndDiscardTheirLateFrames() {
        val answers = ArrayList<JSONObject>()
        scan.analyse { answers += it }
        val old = requireNotNull(pending())
        scan.muted("background")
        assertEquals("idle", answers.single().getString("state"))
        scan.unmuted()
        ReflectionHelpers.setField(scan, "state", "scanning")
        scan.analyse { answers += it }
        val fresh = requireNotNull(pending())
        complete(old, listOf(code("stale")))
        assertSame(fresh, pending())
        assertEquals(1, answers.size)
        complete(fresh, listOf(code("fresh")))
        assertEquals("fresh", answers.last().getJSONArray("seen").getJSONObject(0).getString("rawValue"))
        scan.analyse { answers += it }
        val ending = requireNotNull(pending())
        scan.ended()
        complete(ending, listOf(code("ended")))
        assertEquals(3, answers.size)
        assertEquals("ended", answers.last().getString("state"))
        assertEquals(1, answers.last().getInt("frame"))
        scan = Scan(module, "second", null)
        scan.liveVideo(null)
        ReflectionHelpers.setField(scan, "source", source)
        ReflectionHelpers.setField(scan, "state", "scanning")
        scan.analyse { answers += it }
        val released = requireNotNull(pending())
        scan.stop()
        complete(released, listOf(code("released")))
        assertEquals(4, answers.size)
        assertEquals(0, answers.last().getInt("frame"))
    }

    @Test fun aCaptureWithNoFrameAnswersOnceAndAnOldTimeoutCannotFinishANewPoll() {
        val answers = ArrayList<JSONObject>()
        scan.analyse { answers += it }
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(Scan.PATIENCE_MS))
        assertEquals(1, answers.size)
        assertEquals(0, answers.single().getInt("frame"))
        scan.analyse { answers += it }
        val poll = requireNotNull(pending())
        val timeout = ReflectionHelpers.getField<Runnable>(poll, "timeout")
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
        complete(poll, emptyList())
        // Model a timeout already dispatched when completion removes its queued callback.
        module.main.postDelayed(timeout, Scan.PATIENCE_MS - 100)
        scan.analyse { answers += it }
        val replacement = requireNotNull(pending())
        timeout.run()
        assertSame(replacement, pending())
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(Scan.PATIENCE_MS - 100))
        assertEquals(2, answers.size)
        assertSame(replacement, pending())
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
        assertEquals(3, answers.size)
        assertNull(pending())
    }
}
