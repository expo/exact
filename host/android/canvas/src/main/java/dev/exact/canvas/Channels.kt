package dev.exact.canvas

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.net.Uri
import org.xmlpull.v1.XmlPullParser

/**
 * The app's notification channels (LLP 1133.004.000 D1): what `host.android.notificationChannels` declares, which the
 * build writes as `res/xml/exact_channels.xml` for an app that grants notifications (and not otherwise, so an app
 * that does not notify has no channel at all), and the `default` channel, which a post without `android.channelId`
 * takes. Made after the first frame of each launch, again when the locale changes (their names are the strings
 * tables'), and by [Replaced] when the app is updated, so a channel an update adds exists before a push that arrives
 * ahead of the next launch: Firebase's client shows a background message on a channel that does not exist as its own
 * fallback channel, which has no declared sound.
 *
 * Android keeps a channel's importance and sound once it is made, and the person may change them
 * ([channels](https://developer.android.com/develop/ui/views/notifications/channels)): an existing channel is made
 * again with its installed importance and sound, so only its name and description change. A channel this host made
 * that the declaration no longer names is deleted. A package's own channels (`@exact/alarms`') are never touched.
 */
object Channels {
    /** The preferences that keep the ids this host made. */
    private const val KEPT = "exact.channels"
    const val DEFAULT = "default"

    class Declared(val id: String, val importance: Int, val name: CharSequence, val description: CharSequence?, val sound: Uri?)

    /** Whether the build wrote a declaration: the app grants `device.notifications`. */
    fun granted(context: Context): Boolean = list(context) != 0

    private fun list(context: Context): Int = context.resources.getIdentifier("exact_channels", "xml", context.packageName)

    /** The declared channels, the default one first unless declared; null when the app grants no notifications. */
    fun declared(context: Context): List<Declared>? {
        val xml = list(context).takeIf { it != 0 } ?: return null
        val res = context.resources
        val out = ArrayList<Declared>()
        val parser = res.getXml(xml)
        try {
            while (parser.next() != XmlPullParser.END_DOCUMENT) {
                if (parser.eventType != XmlPullParser.START_TAG || parser.name != "channel") continue
                val id = parser.getAttributeValue(null, "id") ?: continue
                val text = { attribute: String -> parser.getAttributeResourceValue(null, attribute, 0).takeIf { it != 0 }?.let(res::getText) }
                val sound = parser.getAttributeResourceValue(null, "sound", 0).takeIf { it != 0 }?.let {
                    // By name, not by id: an id can change from one build to the next, and the channel keeps its URI.
                    Uri.parse("android.resource://${context.packageName}/raw/${res.getResourceEntryName(it)}")
                }
                out += Declared(id, importance(parser.getAttributeValue(null, "importance")), text("name") ?: id, text("description"), sound)
            }
        } finally {
            parser.close()
        }
        if (out.none { it.id == DEFAULT }) out.add(0, Declared(DEFAULT, NotificationManager.IMPORTANCE_DEFAULT, res.getText(R.string.exact_default_channel), null, null))
        return out
    }

    fun importance(word: String?): Int = when (word) {
        "min" -> NotificationManager.IMPORTANCE_MIN
        "low" -> NotificationManager.IMPORTANCE_LOW
        "high" -> NotificationManager.IMPORTANCE_HIGH
        else -> NotificationManager.IMPORTANCE_DEFAULT
    }

    /** Makes or renames every declared channel and deletes the ones this host made that are no longer declared. Any
     * thread. */
    fun ensure(context: Context) {
        val declared = declared(context) ?: return
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        for (d in declared) {
            val installed = manager.getNotificationChannel(d.id)
            val channel = NotificationChannel(d.id, d.name, installed?.importance ?: d.importance)
            channel.description = d.description?.toString()
            if (installed != null) channel.setSound(installed.sound, installed.audioAttributes)
            else if (d.sound != null) channel.setSound(d.sound, android.media.AudioAttributes.Builder()
                .setUsage(android.media.AudioAttributes.USAGE_NOTIFICATION)
                .setContentType(android.media.AudioAttributes.CONTENT_TYPE_SONIFICATION).build())
            manager.createNotificationChannel(channel)
        }
        val prefs = context.getSharedPreferences(KEPT, Context.MODE_PRIVATE)
        val ids = declared.map { it.id }.toSet()
        for (gone in prefs.getStringSet("made", emptySet()).orEmpty() - ids) manager.deleteNotificationChannel(gone)
        prefs.edit().putStringSet("made", ids).apply()
    }

    /** The first frame of this process, or a locale change: the channels are made on a thread of their own. */
    fun made(context: Context) {
        val app = context.applicationContext
        if (granted(app)) Thread({ ensure(app) }, "exact-channels").start()
    }

    /** The app was updated (`MY_PACKAGE_REPLACED`): its channels are made with no session. Declared by the build only
     * for an app that grants notifications. */
    class Replaced : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == Intent.ACTION_MY_PACKAGE_REPLACED) ensure(context)
        }
    }
}
