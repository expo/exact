// Firebase's messaging service (LLP 1133.004.000 D6, D7), which Firebase's own receiver starts in the app's process.
//
// - While the app is in the background Firebase shows a notification message itself, and none of this runs: on its
//   `channel_id`, else the default channel (the manifest's meta-data), with the app's small icon unless the message
//   names an `icon`. Its tap starts the launcher activity with the message's `data` as extras, of which the host reads
//   only `navigate` (`ExactActivity`).
// - In front Firebase hands the message to [onMessageReceived] instead, which shows it the same way: the channel if
//   the app declares it, the tag (Firebase's key: the tag, id 0, so a local post and `closeNotification` meet it), and
//   a tap through the app's alias that opens `data.navigate`, or the app.
// - A data-only message raises `push` through the headless session (LLP 1133.017 D7–D9). The callback keeps FCM's
//   process custody until the handler settles or ten seconds pass, including boot and time queued behind a wake.
package exact.push

import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.graphics.drawable.Icon
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dev.exact.canvas.Channels
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class PushService : FirebaseMessagingService() {
    override fun onCreate() {
        // Firebase's provider no longer starts it with the process.
        PushModule.firebase(applicationContext)
        super.onCreate()
        // The declared channels, should none of the host's paths have made them yet (an update before any launch).
        Channels.ensure(applicationContext)
    }

    @Suppress("OVERRIDE_DEPRECATION")
    override fun onNewToken(token: String) {
        val app = applicationContext
        android.os.Handler(android.os.Looper.getMainLooper()).post { PushModule.rotated(app, token) }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        val shown = message.notification ?: run {
            val ended = CountDownLatch(1)
            val delivery = PushMessage(applicationContext, message.data) { ended.countDown() }
            delivery.start()
            try { ended.await(10, TimeUnit.SECONDS) }
            catch (_: InterruptedException) { Thread.currentThread().interrupt() }
            finally { delivery.expire() }
            return
        }
        val app = applicationContext
        val channel = shown.channelId?.takeIf { id -> Channels.declared(app)?.any { it.id == id } == true } ?: Channels.DEFAULT
        val navigate = message.data["navigate"]?.takeIf { it.isNotEmpty() }
        val tap = if (navigate != null) ExtensionContext.activation(app, "push", "navigate", JSONArray(), navigate)
            else app.packageManager.getLaunchIntentForPackage(app.packageName) ?: return
        val tag = shown.tag
        val shownAs = tag ?: "FCM-Notification:${System.currentTimeMillis()}"
        // Distinct per post, as `@exact/notifications` makes its intents (`Intent.filterEquals` ignores extras).
        val open = PendingIntent.getActivity(app, 0, tap.setIdentifier("$shownAs/0"), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val title = shown.title ?: app.applicationInfo.loadLabel(app.packageManager).toString()
        val builder = Notification.Builder(app, channel).setSmallIcon(icon(shown.icon)).setContentTitle(title)
            .setContentIntent(open).setAutoCancel(true)
        shown.body?.let { builder.setContentText(it).setStyle(Notification.BigTextStyle().bigText(it)) }
        app.getSystemService(NotificationManager::class.java).notify(shownAs, 0, builder.build())
    }

    /** The message's `icon`, a drawable the app has, else the app's small icon (`exact_notification`, which every app
     * that grants notifications has). */
    private fun icon(named: String?): Icon {
        val app = applicationContext
        val id = listOfNotNull(named, "exact_notification").firstNotNullOfOrNull { name -> app.resources.getIdentifier(name, "drawable", app.packageName).takeIf { it != 0 } }
        return if (id != null) Icon.createWithResource(app, id) else Icon.createWithResource("android", android.R.drawable.ic_popup_reminder)
    }
}
