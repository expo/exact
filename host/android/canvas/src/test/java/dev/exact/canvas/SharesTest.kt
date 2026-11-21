package dev.exact.canvas

import android.content.Intent
import android.net.Uri
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class SharesTest {
    private val share = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, "a note")

    @Test fun aShareFromAnotherAppArrives() = assertTrue(Shares.arrived(share, restored = false))

    /** Back finished the activity a share started; Recents starts the task's base intent, the share, again. */
    @Test fun aShareStartedAgainFromRecentsIsAPlainLaunch() {
        val again = Intent(share).addFlags(Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY or Intent.FLAG_ACTIVITY_NEW_TASK)
        assertFalse(Shares.arrived(again, restored = false))
    }

    /** The system killed the process in the background and starts the activity again from its saved state. */
    @Test fun aShareRestoredWithItsActivityIsAPlainLaunch() = assertFalse(Shares.arrived(share, restored = true))

    /** Several files' text, given as one text (`Requests.sendIntent`) or as a list whose entries with words are joined. */
    @Test fun aSharesTextIsReadFromOneTextAndFromSeveralFilesList() {
        val listed = Intent(Intent.ACTION_SEND_MULTIPLE).putCharSequenceArrayListExtra(Intent.EXTRA_TEXT, arrayListOf<CharSequence>("Two photos", "", "Kyoto"))
        assertEquals("Two photos\nKyoto", Shares.text(listed))
        val files = listOf("a", "b").map { Uri.parse("content://dev.exact.files/$it.png") to "image/png" }
        assertEquals("a note", Shares.text(share))
        assertEquals("Two photos", Shares.text(Requests.sendIntent(null, "Two photos", files)))
        assertEquals("One photo", Shares.text(Requests.sendIntent(null, "One photo", files.take(1))))
        assertNull(Shares.text(Requests.sendIntent(null, "", files)))
        assertEquals("one text", Shares.text(Intent(Intent.ACTION_SEND_MULTIPLE).putExtra(Intent.EXTRA_TEXT, "one text")))
    }
}
