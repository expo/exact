package exact.calendar

import android.app.Activity
import android.content.Intent
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import dev.exact.canvas.Extensions
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** The calendar app's insert screen (`showEventEditor`): a phone with no calendar app answers `NotFoundError`, not the
 * person's cancel, and the next call shows the editor. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class EditorStartTest {
    /** An activity on which no app handles an intent. */
    class NoCalendarApp : Activity() {
        @Deprecated("Activity's own") override fun startActivityForResult(intent: Intent, requestCode: Int) =
            throw android.content.ActivityNotFoundException()
    }

    private val store = EventStore(ExtensionContext(RuntimeEnvironment.getApplication(), "calendar"), {}, {})
    private val answers = ArrayList<JSONObject>()
    private val event = JSONObject().put("event", JSONObject().put("title", "Tea").put("start", "2026-11-12T16:00:00").put("duration", "PT1H"))

    private fun shown(type: Class<out Activity>): Activity {
        val activity = Robolectric.buildActivity(type).setup().get()
        Extensions.front = { activity }
        Extensions.started()
        Extensions.resumed()
        return activity
    }

    @After fun leave() {
        store.stopObserving()
        Extensions.paused()
        Extensions.stopped(false)
        Extensions.front = { null }
    }

    @Test fun noCalendarAppIsNotFoundErrorAndTheNextCallShowsTheEditor() {
        shown(NoCalendarApp::class.java)
        store.later("showEventEditor", event) { answers += it }
        assertEquals("NotFoundError", answers.single().getString("error"))

        val activity = shown(Activity::class.java)
        store.later("showEventEditor", event) { answers += it }
        val made = shadowOf(activity).nextStartedActivityForResult
        assertEquals(Intent.ACTION_INSERT, made.intent.action)
        Extensions.activityResult(made.requestCode, Activity.RESULT_CANCELED, null)
        assertEquals("unknown", answers.last().getString("action"))
    }
}
