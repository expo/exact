package dev.exact.canvas

import android.content.Intent
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
}
