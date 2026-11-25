// @exact/store-update on Android (LLP 1133.038): a newer binary from Google Play, through Play's in-app updates
// (https://developer.android.com/guide/playcore/in-app-updates). Not Exact's own signed updates (LLP 1030).
//
// `storeUpdateStatus` is `getAppUpdateInfo` in Play's words under `android`, announced again on resume and on each
// install state the registered listener hears. It asks Play only for an install Play made: a sideloaded APK is not
// the store's, and Play's listing of its package name is another build. `startStoreUpdate` fetches a fresh
// `AppUpdateInfo` (one starts a flow only once) and runs `startUpdateFlow` over the activity in front, answering by
// the flow's result code. `completeStoreUpdate` installs a downloaded flexible update, which restarts the app. An
// immediate flow the person left (Home, then back) is started again on resume, as Play asks; the kind is kept in
// preferences because a flexible download in progress reports the same availability.
//
// Under the agent the module asks Play for nothing: the status is a store install with nothing newer, with the
// drive's preferred status laid over it; `startStoreUpdate` answers the `store-update` ticket's choice.
//
// From the Play implementation juanclaudiopardo proposed and tested in expo/exact#638.
package exact.storeupdate

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import com.google.android.play.core.appupdate.AppUpdateInfo
import com.google.android.play.core.appupdate.AppUpdateManager
import com.google.android.play.core.appupdate.AppUpdateManagerFactory
import com.google.android.play.core.appupdate.AppUpdateOptions
import com.google.android.play.core.install.InstallException
import com.google.android.play.core.install.InstallState
import com.google.android.play.core.install.InstallStateUpdatedListener
import com.google.android.play.core.install.model.AppUpdateType
import com.google.android.play.core.install.model.InstallStatus
import com.google.android.play.core.install.model.UpdateAvailability
import dev.exact.canvas.Cause
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONObject

/** [play] stands in for Play's manager in a test, and [fromPlay] for the installer check. */
class StoreUpdateModule(
    context: ExtensionContext,
    private val play: AppUpdateManager? = null,
    private val fromPlay: (() -> Boolean)? = null,
) : ExactExtension(context) {
    companion object {
        const val TOPIC = "store-update.storeUpdateStatus"
        const val PLAY_STORE = "com.android.vending"
        private const val IMMEDIATE = "immediate"

        fun availability(value: Int) = when (value) {
            UpdateAvailability.UPDATE_NOT_AVAILABLE -> "current"
            UpdateAvailability.UPDATE_AVAILABLE -> "offered"
            UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS -> "in-progress"
            else -> "unknown"
        }

        fun installStatus(value: Int) = when (value) {
            InstallStatus.REQUIRES_UI_INTENT -> "requires-ui-intent"
            InstallStatus.PENDING -> "pending"
            InstallStatus.DOWNLOADING -> "downloading"
            InstallStatus.DOWNLOADED -> "downloaded"
            InstallStatus.INSTALLING -> "installing"
            InstallStatus.INSTALLED -> "installed"
            InstallStatus.FAILED -> "failed"
            InstallStatus.CANCELED -> "canceled"
            else -> "unknown"
        }

        /** Play's `InstallErrorCode`, or null for none. */
        fun errorCode(value: Int): String? = when (value) {
            0, 1 -> null
            -3 -> "api-not-available"
            -4 -> "invalid-request"
            -5 -> "install-unavailable"
            -6 -> "install-not-allowed"
            -7 -> "download-not-present"
            -9 -> "play-store-not-found"
            -10 -> "app-not-owned"
            -100 -> "internal-error"
            else -> "unknown"
        }

        /** The flow's result code: `RESULT_OK`, `RESULT_CANCELED` or Play's `RESULT_IN_APP_UPDATE_FAILED`. */
        fun outcome(code: Int?) = when (code) {
            Activity.RESULT_OK -> "accepted"
            Activity.RESULT_CANCELED -> "canceled"
            else -> "failed"
        }

        /** The intent that opens [packageName]'s page in Play's store app, and nowhere else. */
        fun storePage(packageName: String): Intent =
            Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$packageName")).setPackage(PLAY_STORE)

        /** Play's answer, nothing newer: what the status starts from. */
        fun android() = JSONObject().put("availableVersionCode", JSONObject.NULL).put("updatePriority", 0)
            .put("clientVersionStalenessDays", JSONObject.NULL).put("flexibleAllowed", false).put("immediateAllowed", false)
            .put("installStatus", "unknown").put("bytesDownloaded", 0).put("totalBytesToDownload", 0)
            .put("installErrorCode", JSONObject.NULL)

        private fun status(available: Boolean, availability: String, android: JSONObject?) = JSONObject()
            .put("available", available).put("updateAvailability", availability)
            .put("apple", JSONObject.NULL).put("android", android ?: JSONObject.NULL)

        private fun error(name: String, message: String? = null) =
            JSONObject().put("error", name).apply { if (message != null) put("message", message) }

        private val offered = setOf(UpdateAvailability.UPDATE_AVAILABLE, UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS)
    }

    private val manager by lazy { play ?: AppUpdateManagerFactory.create(context.application) }
    private val kept by lazy { context.application.getSharedPreferences("exact.store-update", Context.MODE_PRIVATE) }
    /** The last install state the listener heard: its error code is not in `AppUpdateInfo`. */
    private var heard: InstallState? = null
    private var listening = false
    /** Under the agent, the drive's preferred status. */
    private var laid = JSONObject()
    private val listener = InstallStateUpdatedListener { state ->
        heard = state
        say("install ${installStatus(state.installStatus())} ${state.bytesDownloaded()}/${state.totalBytesToDownload()}")
        context.changed(TOPIC)
    }

    private fun say(line: String) = context.log("store-update: $line")

    /** Whether Play's store app installed this app. */
    private fun installedByPlay(): Boolean = fromPlay?.invoke() ?: run {
        val pm = context.application.packageManager
        val pkg = context.application.packageName
        val installer = try {
            if (Build.VERSION.SDK_INT >= 30) pm.getInstallSourceInfo(pkg).installingPackageName
            else @Suppress("DEPRECATION") pm.getInstallerPackageName(pkg)
        } catch (e: Exception) { null }
        installer == PLAY_STORE
    }

    private fun listen() {
        if (listening) return
        listening = true
        manager.registerListener(listener)
    }

    override fun preferred(values: JSONObject) {
        laid = values.optJSONObject("storeUpdateStatus") ?: JSONObject()
        context.changed(TOPIC)
    }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) = later(name, args, null, Cause.none, reply)

    override fun later(name: String, args: JSONObject, preferred: Any?, cause: Cause, reply: (JSONObject) -> Unit) {
        when (name) {
            "storeUpdateStatus" -> if (context.agent) reply(agentStatus()) else status(reply)
            "startStoreUpdate" -> start(args, (preferred as? JSONObject)?.optString("tap")?.takeIf { it.isNotEmpty() }, cause, reply)
            "completeStoreUpdate" -> complete(cause, reply)
            "openStorePage" -> open(cause, reply)
            else -> reply(error("NotSupportedError"))
        }
    }

    /** A store install with nothing newer, the drive's preferred fields laid over it (`android`'s field by field). */
    private fun agentStatus(): JSONObject {
        val out = status(true, "current", android())
        laid.keys().forEach { key ->
            val value = laid.get(key)
            val into = out.optJSONObject(key)
            if (value is JSONObject && into != null) value.keys().forEach { into.put(it, value.get(it)) } else out.put(key, value)
        }
        return out
    }

    /** What Play says of the update. Play answers for an install it made; the rest is not the store's. */
    private fun status(reply: (JSONObject) -> Unit) {
        if (!installedByPlay()) return reply(status(false, "unknown", null))
        listen()
        manager.appUpdateInfo.addOnCompleteListener { task ->
            if (task.isSuccessful) return@addOnCompleteListener reply(answer(task.result))
            val code = errorCode((task.exception as? InstallException)?.errorCode ?: -2)
            say("no update info: $code")
            reply(status(true, "unknown", android().put("installErrorCode", code)))
        }
    }

    private fun answer(info: AppUpdateInfo): JSONObject {
        // FakeAppUpdateManager resets a failed download to UNKNOWN (Play's documentation does not say what Play does);
        // the listener heard the failure and its code either way.
        val failed = heard?.takeIf { it.installStatus() == InstallStatus.FAILED && info.installStatus() == InstallStatus.UNKNOWN }
        val android = JSONObject()
            .put("availableVersionCode", if (info.updateAvailability() in offered) info.availableVersionCode() else JSONObject.NULL)
            .put("updatePriority", info.updatePriority())
            .put("clientVersionStalenessDays", info.clientVersionStalenessDays() ?: JSONObject.NULL)
            .put("flexibleAllowed", info.isUpdateTypeAllowed(AppUpdateType.FLEXIBLE))
            .put("immediateAllowed", info.isUpdateTypeAllowed(AppUpdateType.IMMEDIATE))
            .put("installStatus", installStatus(failed?.installStatus() ?: info.installStatus()))
            .put("bytesDownloaded", info.bytesDownloaded()).put("totalBytesToDownload", info.totalBytesToDownload())
            .put("installErrorCode", failed?.let { errorCode(it.installErrorCode()) } ?: JSONObject.NULL)
        return status(true, availability(info.updateAvailability()), android)
    }

    private fun start(args: JSONObject, tap: String?, cause: Cause, reply: (JSONObject) -> Unit) {
        val type = when (args.optString("appUpdateType")) {
            "flexible" -> AppUpdateType.FLEXIBLE
            IMMEDIATE -> AppUpdateType.IMMEDIATE
            else -> return reply(error("TypeError", "appUpdateType is flexible or immediate"))
        }
        if (context.agent) return agentStart(type, tap, cause, reply)
        if (!installedByPlay()) return reply(error("NotSupportedError", "this install is not from Google Play"))
        val activity = context.activity ?: return reply(error("InvalidStateError", "no activity in front"))
        val options = AppUpdateOptions.newBuilder(type).setAllowAssetPackDeletion(args.optBoolean("allowAssetPackDeletion", false)).build()
        listen()
        manager.appUpdateInfo.addOnCompleteListener { task ->
            if (!task.isSuccessful) {
                val code = errorCode((task.exception as? InstallException)?.errorCode ?: -2)
                say("start: no update info: $code")
                return@addOnCompleteListener reply(error("NotSupportedError", code))
            }
            val info = task.result
            if (info.updateAvailability() !in offered) return@addOnCompleteListener reply(error("NotFoundError", "Google Play offers no newer binary"))
            if (!info.isUpdateTypeAllowed(options)) return@addOnCompleteListener reply(error("NotAllowedError", "Google Play does not allow this update type now"))
            launch(info, activity, options, reply)
        }
    }

    /** Under the agent: the laid status decides what Play would refuse, and the ticket's choice is the flow's outcome. */
    private fun agentStart(type: Int, tap: String?, cause: Cause, reply: (JSONObject) -> Unit) {
        val laid = agentStatus()
        val immediate = type == AppUpdateType.IMMEDIATE
        if (laid.optString("updateAvailability") !in setOf("offered", "in-progress")) {
            context.journal("startStoreUpdate: Google Play offers no newer binary (${cause.origin})")
            return reply(error("NotFoundError", "Google Play offers no newer binary"))
        }
        if (laid.optJSONObject("android")?.optBoolean(if (immediate) "immediateAllowed" else "flexibleAllowed") != true) {
            return reply(error("NotAllowedError", "Google Play does not allow this update type now"))
        }
        val outcome = tap ?: "accepted"
        context.journal("startStoreUpdate: ${if (immediate) IMMEDIATE else "flexible"} $outcome (${cause.origin})")
        reply(JSONObject().put("outcome", outcome))
    }

    private fun launch(info: AppUpdateInfo, activity: Activity, options: AppUpdateOptions, reply: (JSONObject) -> Unit) {
        val immediate = options.appUpdateType() == AppUpdateType.IMMEDIATE
        kept.edit().apply { if (immediate) putBoolean(IMMEDIATE, true) else remove(IMMEDIATE) }.apply()
        say("start ${if (immediate) IMMEDIATE else "flexible"} to ${info.availableVersionCode()}")
        manager.startUpdateFlow(info, activity, options).addOnCompleteListener { flow ->
            val outcome = outcome(if (flow.isSuccessful) flow.result else null)
            if (immediate && outcome != "accepted") kept.edit().remove(IMMEDIATE).apply()
            say("flow $outcome")
            reply(JSONObject().put("outcome", outcome))
            context.changed(TOPIC)
        }
    }

    private fun complete(cause: Cause, reply: (JSONObject) -> Unit) {
        if (context.agent) {
            if (agentStatus().optJSONObject("android")?.optString("installStatus") != "downloaded") {
                return reply(error("InvalidStateError", "no update has downloaded"))
            }
            context.journal("completeStoreUpdate: Google Play would install and restart the app (${cause.origin})")
            return reply(JSONObject())
        }
        if (!installedByPlay()) return reply(error("InvalidStateError", "no update has downloaded"))
        manager.appUpdateInfo.addOnCompleteListener { task ->
            if (!task.isSuccessful || task.result.installStatus() != InstallStatus.DOWNLOADED) {
                return@addOnCompleteListener reply(error("InvalidStateError", "no update has downloaded"))
            }
            say("complete")
            manager.completeUpdate().addOnCompleteListener { done ->
                reply(if (done.isSuccessful) JSONObject() else error("OperationError", errorCode((done.exception as? InstallException)?.errorCode ?: -2)))
            }
        }
    }

    /** The app's page in Play's store app, for an install Play made. */
    private fun open(cause: Cause, reply: (JSONObject) -> Unit) {
        val pkg = context.application.packageName
        if (context.agent) {
            context.journal("openStorePage: market://details?id=$pkg (${cause.origin})")
            return reply(JSONObject())
        }
        if (!installedByPlay()) return reply(error("NotSupportedError", "this install is not from Google Play"))
        val intent = storePage(pkg)
        if (!context.leave(intent.dataString ?: "", "external")) {
            return reply(error("NotAllowedError", "the embedder declined to leave the app for the store"))
        }
        val activity = context.activity
        try {
            if (activity != null) activity.startActivity(intent)
            else context.application.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (e: ActivityNotFoundException) {
            return reply(error("NotSupportedError", "Google Play's store app is not on this device"))
        }
        say("opened the store page")
        reply(JSONObject())
    }

    override fun resumed() {
        context.changed(TOPIC)
        if (context.agent || !kept.getBoolean(IMMEDIATE, false)) return
        manager.appUpdateInfo.addOnCompleteListener { task ->
            val info = if (task.isSuccessful) task.result else null
            val activity = context.activity
            if (info?.updateAvailability() != UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS || activity == null) {
                kept.edit().remove(IMMEDIATE).apply()
                return@addOnCompleteListener
            }
            say("resume the immediate update")
            launch(info, activity, AppUpdateOptions.defaultOptions(AppUpdateType.IMMEDIATE)) {}
        }
    }

    override fun destroy(done: () -> Unit) {
        if (listening) manager.unregisterListener(listener)
        done()
    }
}
