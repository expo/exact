// @exact/notifications on Android (LLP 1133.004 D1–D3, §6, §8; LLP 1133.004.000 D1–D5): NotificationManager for
// posts, POST_NOTIFICATIONS for the status and the request. The build makes one module per host when the app binds the
// extension, outside the agent; under the agent the runner's substitute answers, unless the drive binds it (`--bind
// notifications`).
//
// - A post's channel is a declared one (`host.android.notificationChannels`), `default` unless `android.channelId`
//   names another; the host made both after the first frame (`Channels.kt`). An id the declaration lacks refuses
//   `TypeError`; a literal one was refused by the bake.
// - `silent` always wins: the post keeps its channel and is silenced alone.
// - A post to a channel the person turned off, or with the app's notifications off, refuses `NotAllowedError`, as
//   `denied` does: the platform would drop it without a word.
// - A post with a `showTrigger` in the future is kept and posted then, with no session ([Later]).
// - Actions (stage 2's declared list) are buttons, three at most, a fourth dropped and journaled. Each starts the
//   app through its alias, a background action too (LLP 1133.004.000 D5), and writes the action's name and the reply
//   into the event; the notification closes. `authenticationRequired` asks for the unlock from Android 12; Android
//   has no destructive style.
// - The small icon is the app's `monochrome` icon (`exact_notification`), else the platform's generic glyph.
// - A post's tap is the host's activation ([ExtensionContext.activation]): it opens `navigate` as a link's location,
//   or raises `notificationclick` with a person's origin, warm or from a cold start.
// - The state is a fact (§6): the runtime permission (Android 13 and later) with the shared history, whether the
//   person turned the app's notifications off, and each declared channel's importance, read again when the app comes
//   back to the front and announced when it changed. A post while `prompt` asks first; `denied` refuses.
// - Tier 3: [NotificationsModule.builderHatch] sees each `Notification.Builder` before its post; what the module owns
//   (title, text, channel, content intent; the tag is the post's) it sets again after it.
// - No badge: Android has no app badge call, so `setAppBadge` stays unsupported in the interface's manifest;
//   `android.number` is the launcher's count.
//
// The reporter of expo/exact#537 (juanclaudiopardo) wrote the first version of this module, the posting, the status
// and the request, which this keeps.
package exact.notifications

import android.Manifest
import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.RemoteInput
import android.content.Context
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import dev.exact.canvas.Channels
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject

class NotificationsModule(context: ExtensionContext) : ExactExtension(context) {
    companion object {
        /** Every post's id: its tag tells posts apart, as the web's does, and Firebase posts a tagged notification
         * with id 0 too, so a local post and `closeNotification` meet one it showed (LLP 1133.004.000 D7). */
        const val ID = 0
        /** The group a silent post joins (the key AndroidX's `setSilent` uses). */
        const val SILENT_GROUP = "silent"
        /** The actions Android shows on one notification ([Notification.Builder.addAction]). */
        const val SHOWN = 3
        /** The platform's `CATEGORY_*` values `android.category` may name. */
        val CATEGORIES = setOf("alarm", "call", "email", "err", "event", "location_sharing", "message", "missed_call", "navigation",
            "progress", "promo", "recommendation", "reminder", "service", "social", "status", "stopwatch", "sys", "transport", "workout")

        /** Tier 3: the app's Kotlin sees each builder before its post. */
        @JvmStatic var builderHatch: ((Notification.Builder) -> Unit)? = null

        /** The status bar's icon: the app's `monochrome` icon, which Android draws by its alpha, else the platform's
         * generic glyph. An app with no small icon at all would have its post cancelled by SystemUI. */
        fun smallIcon(app: Context): Icon =
            app.resources.getIdentifier("exact_notification", "drawable", app.packageName).takeIf { it != 0 }
                ?.let { Icon.createWithResource(app, it) } ?: Icon.createWithResource("android", android.R.drawable.ic_popup_reminder)

        /** [intent], made distinct for slot [slot] of the post [identifier]: `PendingIntent` matches intents by
         * `Intent.filterEquals`, which ignores extras and request codes do not separate, so two tags whose intents
         * matched would share one `PendingIntent` and `FLAG_UPDATE_CURRENT` would give one the other's extras. The
         * identifier (API 29, the floor) is part of the match. Slot 0 is the body click, 1 the timed post's alarm,
         * 2 and on the actions. */
        fun distinct(intent: Intent, identifier: String, slot: Int): Intent = intent.setIdentifier("$identifier/$slot")

        /** The button of [action], a declared action's record: an activity start through the app's alias, as every
         * action is on Android (LLP 1133.004.000 D5), mutable only when it has a text field, whose reply the
         * platform writes into it. */
        private fun action(app: Context, identifier: String, action: JSONObject, args: JSONArray, slot: Int): Notification.Action {
            val name = action.getString("action")
            val intent = ExtensionContext.action(app, "notifications", "notificationclick", args, 1, action, identifier, ID)
            val placeholder = if (action.isNull("placeholder")) null else action.optString("placeholder")
            val mutable = if (placeholder == null) PendingIntent.FLAG_IMMUTABLE else if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0
            val pending = PendingIntent.getActivity(app, 2 + slot, distinct(intent, identifier, 2 + slot), PendingIntent.FLAG_UPDATE_CURRENT or mutable)
            val button = Notification.Action.Builder(null as Icon?, action.optString("title").ifEmpty { name }, pending)
            if (placeholder != null) button.addRemoteInput(RemoteInput.Builder(ExtensionContext.REPLY).setLabel(placeholder).build())
            if (Build.VERSION.SDK_INT >= 31 && action.optBoolean("authenticationRequired")) button.setAuthenticationRequired(true)
            return button.build()
        }

        /** Posts [record] (`record`'s fields): with a session or without one. Throws `SecurityException` when the
         * platform refuses it. */
        fun post(app: Context, record: JSONObject) {
            val tag = record.optString("tag")
            val identifier = record.getString("identifier")
            val body = record.optString("body").takeIf { record.has("body") }
            // An untagged post's click carries "" as the web's does; the identifier is the system's alone.
            val event = JSONObject().put("tag", tag).put("data", record.opt("data") ?: JSONObject.NULL)
                .put("title", record.optString("title")).put("body", body ?: JSONObject.NULL)
            val navigate = record.optString("navigate").takeIf { record.has("navigate") }
            val tap = ExtensionContext.activation(app, "notifications", "notificationclick", JSONArray().put(tag).put(event), navigate)
            val open = PendingIntent.getActivity(app, 0, distinct(tap, identifier, 0), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            val channel = record.getString("channel")
            val builder = Notification.Builder(app, channel)
            builderHatch?.invoke(builder)
            // An empty title is the web's too: the platform then shows the app's name.
            val title = record.optString("title").ifEmpty { app.applicationInfo.loadLabel(app.packageManager).toString() }
            builder.setSmallIcon(smallIcon(app)).setContentTitle(title).setContentIntent(open).setAutoCancel(true).setChannelId(channel)
            if (body != null) builder.setContentText(body).setStyle(Notification.BigTextStyle().bigText(body))
            // Android has no per-post silence on the platform's builder: a child of a group whose summary alone alerts
            // makes no sound and does not peek, whatever its channel plays (`GROUP_ALERT_SUMMARY`), which is how
            // AndroidX's `NotificationCompat.Builder.setSilent` silences one post (API 26, below the floor).
            if (record.optBoolean("silent")) builder.setGroupAlertBehavior(Notification.GROUP_ALERT_SUMMARY).setGroup(SILENT_GROUP)
            if (record.has("category")) builder.setCategory(record.getString("category"))
            if (record.has("number")) builder.setNumber(record.getInt("number"))
            val actions = record.optJSONArray("actions") ?: JSONArray()
            for (i in 0 until actions.length()) builder.addAction(action(app, identifier, actions.getJSONObject(i), JSONArray().put(tag).put(event), i))
            val manager = app.getSystemService(NotificationManager::class.java)
            // The first frame makes the channels on a thread of its own; a post can come first, and the platform
            // drops a post whose channel does not exist.
            if (manager.getNotificationChannel(channel) == null) Channels.ensure(app)
            manager.notify(identifier, ID, builder.build())
        }
    }

    private val app = context.application
    private val manager: NotificationManager get() = app.getSystemService(NotificationManager::class.java)

    /** The status last announced, to say a change once. */
    private var said: String? = null

    // A force stop cancels the app's alarms, and its next launch makes this module.
    init {
        Later.rearm(app)
    }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        when (name) {
            "showNotification" -> show(args, reply)
            "closeNotification" -> {
                args.optString("tag").takeIf { it.isNotEmpty() }?.let {
                    manager.cancel(it, ID)
                    Later.cancel(app, it)
                }
                reply(JSONObject())
            }
            "notificationStatus" -> status().also { said = it.toString(); reply(it) }
            "requestNotificationPermission" -> ask { reply(status().also(::announce)) }
            else -> reply(refused("NotSupportedError", "$name is not on Android"))
        }
    }

    /** Settings may have changed while the app was away. */
    override fun resumed() {
        if (said != null) announce(status())
    }

    private fun show(args: JSONObject, reply: (JSONObject) -> Unit) {
        val words = args.optJSONObject("android") ?: JSONObject()
        val channel = if (words.isNull("channelId")) Channels.DEFAULT else words.optString("channelId")
        if (Channels.declared(app)?.none { it.id == channel } != false) return reply(refused("TypeError", "android.channelId \"$channel\" is not a channel host.android.notificationChannels declares"))
        val category = if (words.isNull("category")) null else words.optString("category")
        if (category != null && category !in CATEGORIES) return reply(refused("TypeError", "android.category \"$category\" is not one of the platform's categories"))
        val number = if (words.isNull("number")) null else words.optDouble("number")
        if (number != null && (number < 0 || number != Math.floor(number) || number > Int.MAX_VALUE)) return reply(refused("TypeError", "android.number is a non-negative integer"))
        ask {
            val status = status().also(::announce)
            if (status.getString("state") != "granted") return@ask reply(refused("NotAllowedError", "notifications are off for this app"))
            val record = record(args, channel, category, number, actions(args))
            val posted = record.getString("channel")
            if (manager.getNotificationChannel(posted)?.importance == NotificationManager.IMPORTANCE_NONE) {
                return@ask reply(refused("NotAllowedError", "the channel \"$posted\" is off in Settings"))
            }
            val at = if (args.isNull("showTrigger")) null else args.optDouble("showTrigger").toLong()
            try {
                // A tag's later post is replaced, as a shown one is.
                Later.cancel(app, record.getString("identifier"))
                if (at != null && at > System.currentTimeMillis()) {
                    Later.schedule(app, record, at)
                    context.log("showNotification: scheduled")
                } else {
                    post(app, record)
                    context.log("showNotification: shown")
                }
                reply(JSONObject())
            } catch (e: SecurityException) {
                reply(refused("NotAllowedError", e.message ?: "refused"))
            }
        }
    }

    /** What a post shows, as the receiver that posts it later keeps it: the post's own fields and its identifier,
     * which is its tag, or a fresh one for an untagged post. */
    private fun record(args: JSONObject, channel: String, category: String?, number: Double?, actions: JSONArray): JSONObject {
        val tag = if (args.isNull("tag")) "" else args.optString("tag")
        val record = JSONObject().put("tag", tag).put("identifier", tag.ifEmpty { java.util.UUID.randomUUID().toString() })
            .put("title", args.optString("title")).put("channel", channel).put("silent", args.optBoolean("silent", false))
        for (key in listOf("body", "data", "navigate")) if (!args.isNull(key)) record.put(key, args.optString(key))
        category?.let { record.put("category", it) }
        number?.let { record.put("number", it.toInt()) }
        if (actions.length() > 0) record.put("actions", actions)
        return record
    }

    /** The post's actions, the first [SHOWN]: Android shows no more, so the rest are dropped and journaled. */
    private fun actions(args: JSONObject): JSONArray {
        val all = args.optJSONArray("actions") ?: return JSONArray()
        val shown = JSONArray()
        for (i in 0 until minOf(all.length(), SHOWN)) shown.put(all.getJSONObject(i))
        if (all.length() > SHOWN) {
            val dropped = (SHOWN until all.length()).joinToString(", ") { "\"${all.getJSONObject(it).optString("action")}\"" }
            context.journal("showNotification: Android shows $SHOWN actions; $dropped not shown")
        }
        return shown
    }

    /** The runtime permission's dialog while it is `prompt` (Android 13 and later), then [done]; at once otherwise. */
    private fun ask(done: () -> Unit) {
        if (Build.VERSION.SDK_INT >= 33 && context.permission(Manifest.permission.POST_NOTIFICATIONS).state == "prompt") {
            context.askPermission(Manifest.permission.POST_NOTIFICATIONS, done)
        } else done()
    }

    /** The permission fact (§6), in the interface's shape, with the platform's words. */
    private fun status(): JSONObject {
        val on = manager.areNotificationsEnabled()
        val permission = if (Build.VERSION.SDK_INT >= 33) context.permission(Manifest.permission.POST_NOTIFICATIONS) else null
        val state = when (permission?.state) {
            null -> if (on) "granted" else "denied"
            "granted" -> if (on) "granted" else "denied"
            "denied" -> "denied"
            else -> "prompt"
        }
        val channels = JSONArray()
        for (d in Channels.declared(app).orEmpty()) {
            val importance = when (manager.getNotificationChannel(d.id)?.importance ?: d.importance) {
                NotificationManager.IMPORTANCE_NONE -> "none"
                NotificationManager.IMPORTANCE_MIN -> "min"
                NotificationManager.IMPORTANCE_LOW -> "low"
                NotificationManager.IMPORTANCE_HIGH -> "high"
                NotificationManager.IMPORTANCE_MAX -> "max"
                else -> "default"
            }
            channels.put(JSONObject().put("id", d.id).put("importance", importance))
        }
        val android = JSONObject().put("areNotificationsEnabled", on)
            .put("shouldShowRationale", permission?.shouldShowRationale ?: false)
            .put("permanentlyDenied", permission?.permanentlyDenied ?: false).put("channels", channels)
        return JSONObject().put("available", true).put("state", state).put("badge", false).put("android", android)
    }

    /** A status read: its readers are asked again when it changed since the last one said. */
    private fun announce(status: JSONObject) {
        val text = status.toString()
        if (text == said) return
        if (said != null) context.changed("notifications.notificationStatus")
        said = text
    }

    private fun refused(code: String, message: String) = JSONObject().put("error", code).put("message", message)
}
