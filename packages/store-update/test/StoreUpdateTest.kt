package exact.storeupdate

import android.os.Looper
import com.google.android.gms.tasks.Task
import com.google.android.gms.tasks.Tasks
import com.google.android.play.core.appupdate.AppUpdateInfo
import com.google.android.play.core.appupdate.AppUpdateManager
import com.google.android.play.core.appupdate.AppUpdateOptions
import com.google.android.play.core.appupdate.testing.FakeAppUpdateManager
import com.google.android.play.core.install.model.AppUpdateType
import com.google.android.play.core.install.model.InstallErrorCode
import dev.exact.canvas.Exact
import dev.exact.canvas.ExactActivity
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import java.lang.ref.WeakReference
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** Play's in-app updates over its `FakeAppUpdateManager`, which plays the Play Store's side. From the tests
 *  juanclaudiopardo wrote for expo/exact#638. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class StoreUpdateTest {
    private val announced = ArrayList<String>()
    private lateinit var play: FakeAppUpdateManager
    private lateinit var module: StoreUpdateModule

    /** The fake, counting the flows started; [answer], when set, is the flow's result code in place of the fake's. */
    private class Counting(val fake: FakeAppUpdateManager, val answer: Int? = null) : AppUpdateManager by fake {
        var starts = 0
        override fun startUpdateFlow(info: AppUpdateInfo, activity: android.app.Activity, options: AppUpdateOptions): Task<Int> {
            starts += 1
            return answer?.let { Tasks.forResult(it) } ?: fake.startUpdateFlow(info, activity, options)
        }
    }

    private fun counting(answer: Int? = null) = Counting(play, answer).also {
        module = StoreUpdateModule(ExtensionContext(RuntimeEnvironment.getApplication(), "store-update"), it) { true }
    }

    private fun keptImmediate() = RuntimeEnvironment.getApplication().getSharedPreferences("exact.store-update", 0).getBoolean("immediate", false)

    @Before fun setUp() {
        Exact.reader = WeakReference(Robolectric.buildActivity(ExactActivity::class.java).get())
        Host.onChanged = { announced += it }
        play = FakeAppUpdateManager(RuntimeEnvironment.getApplication())
        module = StoreUpdateModule(ExtensionContext(RuntimeEnvironment.getApplication(), "store-update"), play) { true }
    }

    @After fun leave() {
        Exact.reader = null
        Host.onChanged = {}
        RuntimeEnvironment.getApplication().getSharedPreferences("exact.store-update", 0).edit().clear().commit()
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()

    private fun call(name: String, args: JSONObject = JSONObject()): JSONObject? {
        var answer: JSONObject? = null
        module.later(name, args) { answer = it }
        idle()
        return answer
    }

    private fun status() = call("storeUpdateStatus")!!
    /** Play's own fields, under `android`. */
    private fun playFields() = status().getJSONObject("android")
    private fun start(type: String) = call("startStoreUpdate", JSONObject().put("appUpdateType", type))

    @Test fun noUpdateIsReportedAndCannotStart() {
        play.setUpdateNotAvailable()
        val s = status()
        assertTrue(s.getBoolean("available"))
        assertEquals("current", s.getString("updateAvailability"))
        assertTrue(s.getJSONObject("android").isNull("availableVersionCode"))
        assertEquals("NotFoundError", start("flexible")?.getString("error"))
    }

    @Test fun anUpdateIsDescribedInPlaysWords() {
        play.setUpdateAvailable(7)
        play.setUpdatePriority(4)
        play.setClientVersionStalenessDays(3)
        val s = status()
        assertEquals("offered", s.getString("updateAvailability"))
        assertEquals(7, s.getJSONObject("android").getInt("availableVersionCode"))
        assertEquals(4, s.getJSONObject("android").getInt("updatePriority"))
        assertEquals(3, s.getJSONObject("android").getInt("clientVersionStalenessDays"))
        assertTrue(s.getJSONObject("android").getBoolean("flexibleAllowed"))
        assertTrue(s.getJSONObject("android").getBoolean("immediateAllowed"))
        assertEquals("unknown", s.getJSONObject("android").getString("installStatus"))
    }

    @Test fun aTypePlayDoesNotAllowIsRefused() {
        play.setUpdateAvailable(7, AppUpdateType.FLEXIBLE)
        val s = status()
        assertTrue(s.getJSONObject("android").getBoolean("flexibleAllowed"))
        assertFalse(s.getJSONObject("android").getBoolean("immediateAllowed"))
        assertEquals("NotAllowedError", start("immediate")?.getString("error"))
        assertFalse(play.isImmediateFlowVisible)
    }

    @Test fun aWordThatIsNotATypeIsATypeError() {
        assertEquals("TypeError", start("soon")?.getString("error"))
    }

    @Test fun aFlexibleUpdateDownloadsAnnouncingProgressThenCompletes() {
        play.setUpdateAvailable(7)
        var answer: JSONObject? = null
        module.later("startStoreUpdate", JSONObject().put("appUpdateType", "flexible")) { answer = it }
        idle()
        assertTrue("Play's confirmation shows", play.isConfirmationDialogVisible)
        play.userAcceptsUpdate()
        idle()
        assertEquals("accepted", answer?.getString("outcome"))

        announced.clear()
        play.downloadStarts()
        play.setTotalBytesToDownload(1000)
        play.setBytesDownloaded(400)
        idle()
        assertTrue("the status is announced as the download goes", announced.contains(StoreUpdateModule.TOPIC))
        val downloading = status()
        assertEquals("downloading", downloading.getJSONObject("android").getString("installStatus"))
        assertEquals(400, downloading.getJSONObject("android").getLong("bytesDownloaded"))
        assertEquals(1000, downloading.getJSONObject("android").getLong("totalBytesToDownload"))

        assertEquals("InvalidStateError", call("completeStoreUpdate")?.getString("error"))
        play.downloadCompletes()
        idle()
        assertEquals("downloaded", playFields().getString("installStatus"))
        assertNotNull(call("completeStoreUpdate"))
        assertTrue("Play installs with its splash screen", play.isInstallSplashScreenVisible)
        play.installCompletes()
        idle()
    }

    /** The fake answers RESULT_OK as soon as the flow starts, so the codes are checked here. */
    @Test fun theFlowsResultCodeIsItsOutcome() {
        assertEquals("accepted", StoreUpdateModule.outcome(android.app.Activity.RESULT_OK))
        assertEquals("canceled", StoreUpdateModule.outcome(android.app.Activity.RESULT_CANCELED))
        assertEquals("failed", StoreUpdateModule.outcome(com.google.android.play.core.install.model.ActivityResult.RESULT_IN_APP_UPDATE_FAILED))
        assertEquals("failed", StoreUpdateModule.outcome(null))
    }

    @Test fun aFailedDownloadReportsItsErrorCode() {
        play.setUpdateAvailable(7)
        module.later("startStoreUpdate", JSONObject().put("appUpdateType", "flexible")) {}
        idle()
        play.userAcceptsUpdate()
        play.downloadStarts()
        play.setInstallErrorCode(InstallErrorCode.ERROR_INSTALL_NOT_ALLOWED)
        play.downloadFails()
        idle()
        // The fake also fails getAppUpdateInfo while an error code is set.
        play.setInstallErrorCode(InstallErrorCode.NO_ERROR)
        val s = status()
        assertEquals("failed", s.getJSONObject("android").getString("installStatus"))
        assertEquals("install-not-allowed", s.getJSONObject("android").getString("installErrorCode"))
    }

    @Test fun anImmediateFlowLeftIsStartedAgainOnResume() {
        val play = counting()
        this.play.setUpdateAvailable(7)
        module.later("startStoreUpdate", JSONObject().put("appUpdateType", "immediate")) {}
        idle()
        assertTrue(this.play.isImmediateFlowVisible)
        this.play.userAcceptsUpdate()
        this.play.downloadStarts()
        idle()
        assertEquals("in-progress", status().getString("updateAvailability"))
        assertEquals(1, play.starts)
        module.resumed()
        idle()
        assertEquals("the immediate flow is started again on resume", 2, play.starts)
        assertEquals(AppUpdateType.IMMEDIATE, this.play.typeForUpdateInProgress)
    }

    @Test fun anAcceptedImmediateUpdateIsForgottenOnceInstalled() {
        val play = counting()
        this.play.setUpdateAvailable(7)
        module.later("startStoreUpdate", JSONObject().put("appUpdateType", "immediate")) {}
        idle()
        this.play.userAcceptsUpdate()
        idle()
        // The binary Play installed reads the same preferences and finds nothing in progress.
        this.play.setUpdateNotAvailable()
        assertTrue(keptImmediate())
        module.resumed()
        idle()
        assertEquals(1, play.starts)
        assertFalse("the kept kind is forgotten", keptImmediate())
    }

    @Test fun aFlexibleDownloadIsNotTurnedImmediateOnResume() {
        val play = counting()
        this.play.setUpdateAvailable(7)
        module.later("startStoreUpdate", JSONObject().put("appUpdateType", "flexible")) {}
        idle()
        this.play.userAcceptsUpdate()
        this.play.downloadStarts()
        idle()
        assertEquals("in-progress", status().getString("updateAvailability"))
        module.resumed()
        idle()
        assertEquals(1, play.starts)
        assertEquals(AppUpdateType.FLEXIBLE, this.play.typeForUpdateInProgress)
    }

    @Test fun aFlexibleStartForgetsAnEarlierImmediateOne() {
        RuntimeEnvironment.getApplication().getSharedPreferences("exact.store-update", 0).edit().putBoolean("immediate", true).commit()
        play.setUpdateAvailable(7)
        start("flexible")
        assertFalse(keptImmediate())
    }

    @Test fun aCanceledImmediateFlowAnswersCanceledAndIsNotResumed() {
        val play = counting(android.app.Activity.RESULT_CANCELED)
        this.play.setUpdateAvailable(7)
        assertEquals("canceled", start("immediate")?.getString("outcome"))
        assertFalse(keptImmediate())
        module.resumed()
        idle()
        assertEquals(1, play.starts)
    }

    @Test fun anErrorFromPlayIsReportedCleanly() {
        // Play's `ERROR_APP_NOT_OWNED`; an `adb` install on a phone with the Play Store got `not-available` instead.
        play.setInstallErrorCode(InstallErrorCode.ERROR_APP_NOT_OWNED)
        val s = status()
        assertTrue("Play is there to ask", s.getBoolean("available"))
        assertEquals("unknown", s.getString("updateAvailability"))
        assertEquals("app-not-owned", s.getJSONObject("android").getString("installErrorCode"))
        val refused = start("immediate")!!
        assertEquals("NotSupportedError", refused.getString("error"))
        assertEquals("app-not-owned", refused.getString("message"))
        assertEquals("InvalidStateError", call("completeStoreUpdate")?.getString("error"))
    }

    /** `available` says who made the install; Play's failure to answer is `unknown`, with its code. */
    @Test fun playFailingToAnswerLeavesTheInstallPlays() {
        play.setInstallErrorCode(InstallErrorCode.ERROR_PLAY_STORE_NOT_FOUND)
        val s = status()
        assertTrue(s.getBoolean("available"))
        assertEquals("unknown", s.getString("updateAvailability"))
        assertEquals("play-store-not-found", s.getJSONObject("android").getString("installErrorCode"))
    }

    /** A sideloaded APK is not Play's: Play is not asked, and nothing sends the person to Play's listing. */
    @Test fun anInstallPlayDidNotMakeAsksPlayNothing() {
        val counted = Counting(play)
        var asked = 0
        val asking = object : com.google.android.play.core.appupdate.AppUpdateManager by counted {
            override fun getAppUpdateInfo(): Task<AppUpdateInfo> { asked += 1; return counted.appUpdateInfo }
        }
        module = StoreUpdateModule(ExtensionContext(RuntimeEnvironment.getApplication(), "store-update"), asking) { false }
        play.setUpdateAvailable(7)
        val s = status()
        assertFalse(s.getBoolean("available"))
        assertEquals("unknown", s.getString("updateAvailability"))
        assertTrue(s.isNull("android"))
        assertEquals("NotSupportedError", start("flexible")?.getString("error"))
        assertEquals("NotSupportedError", call("openStorePage")?.getString("error"))
        assertEquals(0, asked)
        assertEquals(0, counted.starts)
    }

    @Test fun theStorePageOpensInPlaysStoreApp() {
        val activity = Exact.reader!!.get()!!
        assertNotNull(call("openStorePage"))
        val opened = shadowOf(activity).nextStartedActivity
        assertEquals(android.content.Intent.ACTION_VIEW, opened.action)
        assertEquals(StoreUpdateModule.PLAY_STORE, opened.`package`)
        assertEquals("market://details?id=${RuntimeEnvironment.getApplication().packageName}", opened.dataString)
    }
}
