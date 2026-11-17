package dev.exact.canvas

import android.graphics.Bitmap
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLog

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class TrackPictureTest {
    @Test fun aThrowingObserverDoesNotSkipTheRestOrTheDeliveryCompletion() {
        val entry = TrackEntry("video", "video")
        val picture = Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)
        val calls = ArrayList<String>()
        entry.observePicture { calls += "first"; throw IllegalStateException("observer failed") }
        entry.observePicture { assertSame(picture, entry.picture); calls += "second" }
        entry.observePicture { calls += "third" }

        entry.picture = picture
        calls += "completed"

        assertEquals(listOf("first", "second", "third", "completed"), calls)
        assertTrue(ShadowLog.getLogsForTag("exact").any { it.msg.contains("picture observer") && it.throwable?.message == "observer failed" })
        entry.picture = picture
        assertEquals(4, calls.size)
    }
}
