package exact.browserautomation

import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.LooperMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, shadows = [QueueTest.Journal::class])
@LooperMode(LooperMode.Mode.PAUSED)
class QueueTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Journal {
        companion object {
            var report: (() -> Unit)? = null
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun journal(line: String) { report?.invoke() }
        }
    }

    @Test fun aPostDuringTheOverflowReportKeepsTheLast32Posts() {
        val module = BrowserAutomationModule(ExtensionContext(RuntimeEnvironment.getApplication(), "browser-automation"))
        val post = module.javaClass.getDeclaredMethod("post", JSONObject::class.java).apply { isAccessible = true }
        fun offer(n: Int) { post.invoke(module, JSONObject().put("n", n)) }
        var reports = 0
        Journal.report = { reports++; offer(33) }
        try {
            for (n in 0..32) offer(n)
            for (n in 34..40) offer(n)
            val pending = module.javaClass.getDeclaredField("unsent").apply { isAccessible = true }.get(module) as Collection<*>
            assertEquals((9..40).toList(), pending.map { (it as JSONObject).getInt("n") })
            assertEquals(1, reports)
        } finally {
            Journal.report = null
        }
    }
}
