// A post for later (`showTrigger`, LLP 1133.004 §8; LLP 1133.004.000 D5): kept as its record and armed with
// `AlarmManager.setAndAllowWhileIdle`, which is inexact and fires through Doze's maintenance windows. Never
// `setExact*`: the exact-alarm permission is `@exact/alarms`' alone, and revoking it cancels every exact alarm the
// app holds (LLP 1133.030 §15 Q10).
//
// - [PostReceiver], unexported, posts the record when its alarm fires, with no session.
// - [ArmReceiver] arms every kept record again after a reboot (the boot permission is the notifications grant's) and
//   after an update; the module does too when it is made, since a force stop cancels the app's alarms.
// - `closeNotification(tag)` cancels a kept post as it closes a shown one.
package exact.notifications

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import org.json.JSONObject

internal object Later {
    private const val KEPT = "exact.notifications.later"

    private fun kept(app: Context) = app.getSharedPreferences(KEPT, Context.MODE_PRIVATE)

    /** The broadcast [record]'s alarm sends: explicit, so it reaches only [PostReceiver]. */
    private fun fire(app: Context, identifier: String, flags: Int): PendingIntent? =
        PendingIntent.getBroadcast(app, 1, NotificationsModule.distinct(Intent(app, PostReceiver::class.java).putExtra("identifier", identifier), identifier, 1),
            flags or PendingIntent.FLAG_IMMUTABLE)

    /** Keeps [record] and arms its alarm at [at], epoch milliseconds; a tag's earlier one is replaced. */
    fun schedule(app: Context, record: JSONObject, at: Long) {
        record.put("at", at)
        kept(app).edit().putString(record.getString("identifier"), record.toString()).apply()
        arm(app, record)
    }

    private fun arm(app: Context, record: JSONObject) {
        val alarms = app.getSystemService(AlarmManager::class.java) ?: return
        val pending = fire(app, record.getString("identifier"), PendingIntent.FLAG_UPDATE_CURRENT) ?: return
        alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, record.getLong("at"), pending)
    }

    /** Cancels the post kept for [identifier], if any. */
    fun cancel(app: Context, identifier: String) {
        kept(app).edit().remove(identifier).apply()
        fire(app, identifier, PendingIntent.FLAG_NO_CREATE)?.let {
            app.getSystemService(AlarmManager::class.java)?.cancel(it)
            it.cancel()
        }
    }

    /** Arms every kept post again; one whose time passed while nothing could fire it is posted now. */
    fun rearm(app: Context) {
        for ((identifier, text) in kept(app).all) {
            val record = (text as? String)?.let { runCatching { JSONObject(it) }.getOrNull() } ?: continue
            if (record.getLong("at") <= System.currentTimeMillis()) due(app, identifier) else arm(app, record)
        }
    }

    /** The post kept for [identifier] is due: posted, and no longer kept. A post the platform refuses (the person
     * turned notifications off meanwhile) is dropped, as the platform drops it. */
    fun due(app: Context, identifier: String) {
        val text = kept(app).getString(identifier, null) ?: return
        kept(app).edit().remove(identifier).apply()
        try {
            NotificationsModule.post(app, JSONObject(text))
        } catch (e: Exception) {
            Log.w("exact", "notifications: a post for later was not shown: $e")
        }
    }
}

/** A kept post's alarm fired. */
class PostReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        intent.getStringExtra("identifier")?.let { Later.due(context.applicationContext, it) }
    }
}

/** The phone started, or the app was updated: the kept posts are armed again. */
class ArmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED || intent.action == Intent.ACTION_MY_PACKAGE_REPLACED) Later.rearm(context.applicationContext)
    }
}
