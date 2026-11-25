package dev.exact.canvas

import android.content.ActivityNotFoundException
import android.content.Intent
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, shadows = [OpenUrlTest.Host::class])
class OpenUrlTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            val answers = mutableListOf<Pair<Int, String?>>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun answer(id: Int, value: String?) { answers.add(id to value) }
        }
    }

    class Links : ExactActivity() {
        var last: Intent? = null
        override fun startActivity(intent: Intent) {
            last = intent
            when (intent.data?.scheme) {
                "missing" -> throw ActivityNotFoundException()
                "private" -> throw SecurityException()
            }
        }
    }

    @Test fun openAnswersEachIntentOnceEvenWithoutAnActivity() {
        val activity = Robolectric.buildActivity(Links::class.java).get()
        val requests = Requests(activity)
        fun open(id: Int, url: String): IntArray {
            val bytes = url.toByteArray()
            val payload = IntArray((bytes.size + 3) / 4)
            bytes.forEachIndexed { i, b -> payload[i / 4] = payload[i / 4] or ((b.toInt() and 255) shl (8 * (i % 4))) }
            return intArrayOf(Requests.OPEN, id, 1 + payload.size, bytes.size) + payload
        }
        Host.answers.clear()
        requests.perform(open(1, "DeMo:///links?q=a+b&encoded=%2B"))
        assertEquals(Intent.ACTION_VIEW, activity.last?.action)
        assertEquals(false, activity.last?.hasCategory(Intent.CATEGORY_BROWSABLE))
        assertEquals("demo:///links?q=a+b&encoded=%2B", activity.last?.dataString)
        requests.perform(open(2, "missing:one"))
        requests.perform(open(3, "private:two"))
        Requests.detached(open(4, "demo:three"))
        requests.perform(open(5, "sms:5550100"))
        assertEquals(Intent.ACTION_SENDTO, activity.last?.action)
        requests.perform(open(6, "https://example.com"))
        assertEquals(true, activity.last?.hasCategory(Intent.CATEGORY_BROWSABLE))
        assertEquals(listOf(1 to "opened", 2 to "failed", 3 to "failed", 4 to "failed", 5 to "opened", 6 to "opened"), Host.answers)
    }
}
