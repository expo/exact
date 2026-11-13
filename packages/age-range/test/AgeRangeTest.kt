package exact.agerange

import android.os.Looper
import com.google.android.gms.tasks.Task
import com.google.android.gms.tasks.TaskCompletionSource
import com.google.android.gms.tasks.Tasks
import com.google.android.play.agesignals.AgeSignalsAccessRequest
import com.google.android.play.agesignals.AgeSignalsAccessResult
import com.google.android.play.agesignals.AgeSignalsManager
import com.google.android.play.agesignals.AgeSignalsRequest
import com.google.android.play.agesignals.AgeSignalsResult
import com.google.android.play.agesignals.model.AgeSignalsStatus
import dev.exact.canvas.Exact
import dev.exact.canvas.ExactActivity
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import java.lang.ref.WeakReference
import org.json.JSONArray
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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class AgeRangeTest {
    /** Play, whose access prompt answers when the test says. */
    private class Play : AgeSignalsManager {
        val prompts = ArrayList<TaskCompletionSource<AgeSignalsAccessResult>>()
        override fun requestAgeSignalsAccess(request: AgeSignalsAccessRequest): Task<AgeSignalsAccessResult> =
            TaskCompletionSource<AgeSignalsAccessResult>().also { prompts += it }.task
        override fun checkAgeSignals(request: AgeSignalsRequest): Task<AgeSignalsResult> =
            Tasks.forResult(AgeSignalsResult.builder().setAgeLower(16).setAgeUpper(17).build())
    }

    @After fun leave() { Exact.reader = null }

    private fun ask(module: AgeRangeModule, vararg gates: Int, answered: (JSONObject) -> Unit) =
        module.later("requestAgeRange", JSONObject().put("ageGates", JSONArray(gates.toList())), answered)

    private fun shared(prompt: TaskCompletionSource<AgeSignalsAccessResult>) {
        prompt.setResult(AgeSignalsAccessResult.builder().setAgeSignalsStatus(AgeSignalsStatus.SHARED).build())
        shadowOf(Looper.getMainLooper()).idle()
    }

    /** LLP 1133.028 D10: a second request waits for the first and is then asked of Play, as on Apple. */
    @Test fun aSecondRequestWaitsForTheFirstThenIsAskedOfPlay() {
        Exact.reader = WeakReference(Robolectric.buildActivity(ExactActivity::class.java).get())
        val play = Play()
        val module = AgeRangeModule(ExtensionContext(RuntimeEnvironment.getApplication(), "age-range"), play)
        var a: JSONObject? = null
        var b: JSONObject? = null
        ask(module, 13, 16) { a = it }
        ask(module, 18) { b = it }
        assertNull("the second waits rather than fails: $b", b)
        assertEquals("Play is asked one request at a time", 1, play.prompts.size)

        shared(play.prompts[0])
        assertEquals("sharing", a?.getString("status"))
        assertNull(b)
        assertEquals("the second is asked of Play once the first has answered", 2, play.prompts.size)

        shared(play.prompts[1])
        assertEquals("sharing", b?.getString("status"))
        assertEquals(16, b?.getInt("lowerBound"))
    }
}
