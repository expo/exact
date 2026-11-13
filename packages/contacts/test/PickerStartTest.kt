package exact.contacts

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

/** The contact picker (`selectContacts`): a phone with no contacts app answers `NotFoundError`, not the
 * person's cancel, and the next call shows the picker. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class PickerStartTest {
    /** An activity on which no app handles an intent. */
    class NoContactsApp : Activity() {
        @Deprecated("Activity's own") override fun startActivityForResult(intent: Intent, requestCode: Int) =
            throw android.content.ActivityNotFoundException()
    }

    private val store = ContactStore(ExtensionContext(RuntimeEnvironment.getApplication(), "contacts"), {}, {})
    private val answers = ArrayList<JSONObject>()
    private val asked = JSONObject().put("properties", org.json.JSONArray().put("name"))

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

    @Test fun noContactsAppIsNotFoundErrorAndTheNextCallShowsThePicker() {
        shown(NoContactsApp::class.java)
        store.later("selectContacts", asked) { answers += it }
        assertEquals("NotFoundError", answers.single().getString("error"))

        val activity = shown(Activity::class.java)
        store.later("selectContacts", asked) { answers += it }
        val made = shadowOf(activity).nextStartedActivityForResult
        assertEquals(Intent.ACTION_PICK, made.intent.action)
        Extensions.activityResult(made.requestCode, Activity.RESULT_CANCELED, null)
        assertEquals(0, answers.last().getJSONArray("contacts").length())
    }
}
