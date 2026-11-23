package dev.exact.canvas

import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID
import kotlin.math.roundToInt

/**
 * The app's live activities on Android (LLP 1126 D13): each one an ongoing notification that asks to be promoted to a
 * Live Update ([Live Updates](https://developer.android.com/develop/ui/views/notifications/live-update)). What a
 * notification shows is the activity's `android` mapping in `app.json`, which the bake checked and the build wrote as
 * `res/raw/exact_activities.json` (`reach.androidActivities`): the state fields its title, text, progress, time and
 * status-bar chip read, and its channel.
 *
 * The host's sends come as requests (kind 24, [Requests]) on the main thread, `{"ticket", "command", "args"}` as the
 * runner wrote them, or `{"enumerate": true}`. Each send is answered by its ticket, `{"ticket", "status", "body": {"id"}}`
 * or `{"ticket", "status": 400, "error"}`, and then the activities the shade shows are listed, as Apple's arm answers
 * and lists (`host/apple/activityarm/ActivityArm.swift`).
 *
 * The activity's id is its notification's tag, so the list is read again from
 * [NotificationManager.getActiveNotifications] after the process is made again, and an activity the person dismissed
 * is gone from it: a later send to it is refused, so it is never posted again. Its kind rides in the notification's
 * extras; its state, which may hold fields the mapping does not show, is kept in the app's private preferences, since
 * a notification listener reads every extra.
 *
 * `notify` and `cancel` reach the shade a few main-thread turns later. A send is answered once the shade shows what it
 * did, and until then the send's own post is the activity's, so a send that follows at once neither misses a start
 * nor revives an end.
 */
object Activities {
    private const val TAG = "ExactActivities"
    /** Every activity's notification id; its tag tells them apart. */
    private const val ID = 0x61637476
    private const val KIND = "dev.exact.activity.kind"
    private const val ENDED = "dev.exact.activity.ended"
    private const val STALE = "dev.exact.activity.stale"
    /** When the activity started, which orders the list as ActivityKit's is: an update moves a notification's post time. */
    private const val STARTED = "dev.exact.activity.started"
    /** Each post's own mark, by which [settle] knows the shade shows it. */
    private const val POST = "dev.exact.activity.post"
    /** The preferences that keep each activity's state by id. */
    private const val STATES = "exact.activities"
    /** How often, and how many times, [settle] reads the shade again. */
    private const val SHOWN_MS = 25L
    private const val SHOWN_TRIES = 40
    /** How long an activity ended with the `default` policy stays, as iOS keeps one on the Lock Screen. */
    private const val KEPT_MS = 4 * 60 * 60 * 1000L

    var application: Context? = null
    private val main by lazy { Handler(Looper.getMainLooper()) }

    /** A send's post, or its cancel when [extras] is null, that the shade may not show yet. */
    private class Pending(val mark: Long, val extras: Bundle?)
    private val pending = HashMap<String, Pending>()

    /** Lists again when the next stale date or kept end passes. */
    private val deadline = Runnable { application?.let { report(it) } }

    class Row(val kind: String, val channel: String, val title: String, val text: String?, val progress: String?, val chip: String?, val time: String?)

    private var rows: Map<String, Row>? = null

    private fun rows(context: Context): Map<String, Row> = rows ?: run {
        val id = context.resources.getIdentifier("exact_activities", "raw", context.packageName)
        val text = if (id == 0) "[]" else context.resources.openRawResource(id).bufferedReader().use { it.readText() }
        val list = JSONArray(text)
        (0 until list.length()).associate { i ->
            val row = list.getJSONObject(i)
            val android = row.getJSONObject("android")
            val field = { key: String -> android.optString(key).takeIf { it.isNotEmpty() } }
            val kind = row.getString("kind")
            kind to Row(kind, android.optString("channel", Channels.DEFAULT), android.getString("title"), field("text"), field("progress"), field("chip"), field("when"))
        }.also { rows = it }
    }

    private class Refused(message: String) : Exception(message)

    /** A request of the host's, on the main thread. */
    fun request(body: String) {
        val context = application ?: run { Log.w(TAG, "no application for an activity request"); return }
        val request = JSONObject(body)
        if (request.optBoolean("enumerate")) return report(context)
        val ticket = request.getLong("ticket")
        try {
            val args = request.getJSONArray("args")
            val id = when (val command = request.getString("command")) {
                "startActivity" -> start(context, args.getString(0), args.getJSONObject(1))
                "updateActivity" -> update(context, args.getString(0), args.getJSONObject(1), args.optNullable(2) as? String, (args.optNullable(3) as? Number)?.toLong())
                "endActivity" -> end(context, args.getString(0), args.getJSONObject(1), args.opt(2))
                else -> throw Refused("unknown activity send $command")
            }
            settle(context, ticket, id, pending.getValue(id).mark, SHOWN_TRIES)
        } catch (e: Exception) {
            // A refusal of ours, or the platform's: a permission revoked or a channel deleted mid-call.
            Exact.activities(JSONObject().put("ticket", ticket).put("status", 400).put("error", if (e is Refused) e.message else "$e").toString())
            report(context)
        }
    }

    /**
     * Answer [ticket] once the shade shows [id]'s post [mark], or no longer shows [id] for a cancel, then list. A
     * later send to [id] settles this one. A post the shade has not shown after [SHOWN_TRIES] reads is refused, and a
     * start's is cancelled, so no notification outlives an id the app never received.
     */
    private fun settle(context: Context, ticket: Long, id: String, mark: Long, tries: Int) {
        val own = pending[id]?.takeIf { it.mark == mark }
        val posted = active(context, id)
        val shown = own == null || if (own.extras == null) posted == null else posted?.notification?.extras?.getLong(POST) == mark
        if (!shown && tries > 0) {
            main.postDelayed({ settle(context, ticket, id, mark, tries - 1) }, SHOWN_MS)
            return
        }
        if (own != null) pending.remove(id)
        val reply = JSONObject().put("ticket", ticket)
        if (shown || own?.extras == null) reply.put("status", 200).put("body", JSONObject().put("id", id))
        else {
            if (posted == null) {
                manager(context).cancel(id, ID)
                forget(context, id)
            }
            reply.put("status", 400).put("error", "Android did not show activity $id's notification")
        }
        Exact.activities(reply.toString())
        report(context)
    }

    /** In front again, or the shade closed over the app: the person may have dismissed an activity or changed what
     * Settings allows. */
    fun resumed() {
        val context = application ?: return
        if (rows(context).isNotEmpty()) report(context)
    }

    private fun JSONArray.optNullable(i: Int): Any? = if (i >= length() || isNull(i)) null else get(i)

    private fun JSONObject.optNullable(key: String): Any? = if (!has(key) || isNull(key)) null else get(key)

    private fun manager(context: Context) = context.getSystemService(NotificationManager::class.java)!!

    private fun states(context: Context) = context.getSharedPreferences(STATES, Context.MODE_PRIVATE)

    private fun forget(context: Context, id: String) = states(context).edit().remove(id).apply()

    private fun active(context: Context, id: String) = manager(context).activeNotifications.firstOrNull { it.id == ID && it.tag == id }

    private fun start(context: Context, kind: String, state: JSONObject): String {
        val row = rows(context)[kind] ?: throw Refused("activity $kind has no android mapping in app.json's live_activities")
        check(row, state)
        val manager = manager(context)
        if (!manager.areNotificationsEnabled()) throw Refused("notifications are off for this app: ask with requestNotificationPermission() first, or the person turns them on in Settings")
        if (manager.getNotificationChannel(row.channel) == null) Channels.ensure(context)
        if (manager.getNotificationChannel(row.channel)?.importance == NotificationManager.IMPORTANCE_NONE) throw Refused("the person turned off channel ${row.channel} in Settings")
        val id = UUID.randomUUID().toString()
        val now = System.currentTimeMillis()
        post(context, row, id, now, now, state, alert = null, ended = null, stale = null)
        return id
    }

    private fun update(context: Context, id: String, state: JSONObject, alert: String?, stale: Long?): String {
        val (row, extras) = existing(context, id)
        check(row, state)
        if (extras.getLong(ENDED, 0) != 0L) throw Refused("activity $id has ended")
        post(context, row, id, System.currentTimeMillis(), extras.getLong(STARTED), state, alert = alert, ended = null, stale = stale)
        return id
    }

    private fun end(context: Context, id: String, state: JSONObject, policy: Any?): String {
        val (row, extras) = existing(context, id)
        check(row, state)
        val now = System.currentTimeMillis()
        val until = when {
            policy == "immediate" -> now
            policy == "default" -> now + KEPT_MS
            policy is Number -> policy.toLong()
            else -> throw Refused("dismissal policy is immediate, default, or Unix milliseconds")
        }
        if (until > now) post(context, row, id, now, extras.getLong(STARTED), state, alert = null, ended = until, stale = null)
        else {
            pending[id] = Pending(System.nanoTime(), null)
            manager(context).cancel(id, ID)
            forget(context, id)
        }
        return id
    }

    /** [id]'s row and its post's extras: this process's latest send to it, else the shade's. */
    private fun existing(context: Context, id: String): Pair<Row, Bundle> {
        val sent = pending[id]
        if (sent != null && sent.extras == null) throw Refused("activity $id has ended")
        val extras = sent?.extras ?: active(context, id)?.notification?.extras
            ?: throw Refused("no activity $id: it ended, or the person dismissed it")
        val kind = extras.getString(KIND)
        val row = rows(context)[kind] ?: throw Refused("activity $id's kind $kind has no android mapping")
        return row to extras
    }

    /** The state has each field [row] shows, of the type the bake checked against the declared shape. */
    private fun check(row: Row, state: JSONObject) {
        if (state.optNullable(row.title) !is String) throw Refused("activity ${row.kind}'s state has no string ${row.title}, which its notification's title shows")
        for ((field, number) in listOf(row.text to false, row.chip to false, row.progress to true, row.time to true)) {
            if (field == null) continue
            if (!state.has(field)) throw Refused("activity ${row.kind}'s state has no field $field")
            val value = state.optNullable(field) ?: continue
            if (if (number) value !is Number else value !is String) throw Refused("activity ${row.kind}'s $field is not a ${if (number) "number" else "string"}")
        }
    }

    /**
     * Post [id]'s notification at [now]: ongoing and asking for promotion while it is live, an ordinary one that times
     * out once it has [ended]. Its first post alerts as its channel does, and an update only with an [alert], as
     * ActivityKit's.
     */
    private fun post(context: Context, row: Row, id: String, now: Long, started: Long, state: JSONObject, alert: String?, ended: Long?, stale: Long?) {
        val text = { field: String? -> field?.let { state.optNullable(it) as? String } }
        val number = { field: String? -> field?.let { state.optNullable(it) as? Number }?.toDouble()?.takeIf { it.isFinite() } }
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
        val tap = launch?.let { PendingIntent.getActivity(context, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT) }
        val icon = context.resources.getIdentifier("exact_notification", "drawable", context.packageName).takeIf { it != 0 }
            ?: context.applicationInfo.icon
        val mark = System.nanoTime()
        val extras = Bundle().apply {
            putString(KIND, row.kind)
            putLong(STARTED, started)
            putLong(POST, mark)
            if (ended != null) putLong(ENDED, ended)
            if (stale != null) putLong(STALE, stale)
            // The framework's builder method is 36.1's; the extra is what it sets and what AndroidX's sets on every
            // version, and a system without Live Updates ignores it.
            if (ended == null) putBoolean(Notification.EXTRA_REQUEST_PROMOTED_ONGOING, true)
        }
        val builder = Notification.Builder(context, row.channel)
            .setSmallIcon(icon)
            .setContentTitle(text(row.title))
            .setContentIntent(tap)
            .setOngoing(ended == null)
            .setOnlyAlertOnce(alert == null)
            .addExtras(extras)
        val body = text(row.text)
        val progress = number(row.progress)
        body?.let { builder.setContentText(it) }
        // Below API 36 a progress bar shows only outside BigTextStyle's expanded template.
        if (body != null && progress == null) builder.setStyle(Notification.BigTextStyle().bigText(body))
        // Announced by accessibility services as the alert's text.
        alert?.let { builder.setTicker(it) }
        if (ended != null) builder.setTimeoutAfter(ended - now)
        // Ahead, the chip counts down to it by itself; already past, it is how long the activity has run.
        number(row.time)?.toLong()?.let { at -> builder.setWhen(at).setShowWhen(true).setUsesChronometer(ended == null && at <= now) }
        // 0 to 1, as HTML's `progress` without `max`.
        progress?.let { fraction ->
            val percent = (fraction.coerceIn(0.0, 1.0) * 100).roundToInt()
            builder.setCategory(Notification.CATEGORY_PROGRESS)
            if (Build.VERSION.SDK_INT >= 36) builder.setStyle(Notification.ProgressStyle().setProgress(percent))
            else builder.setProgress(100, percent, false)
        }
        if (Build.VERSION.SDK_INT >= 36 && ended == null) text(row.chip)?.let { builder.setShortCriticalText(it) }
        states(context).edit().putString(id, state.toString()).apply()
        pending[id] = Pending(mark, extras)
        manager(context).notify(id, ID, builder.build())
    }

    /** List what the shade shows, by [Exact.activities], and wait for the next deadline: an activity reads `promoted`
     * when its posted notification carries `FLAG_PROMOTED_ONGOING`, the system's word, not the request's. */
    private fun report(context: Context) {
        val now = System.currentTimeMillis()
        val states = states(context)
        val list = JSONArray()
        var next = Long.MAX_VALUE
        val shown = manager(context).activeNotifications.filter { it.id == ID && it.tag != null }
        shown.sortedBy { it.notification.extras.getLong(STARTED) }.forEach { posted ->
            val extras = posted.notification.extras
            val kind = extras.getString(KIND) ?: return@forEach
            val stale = extras.getLong(STALE, 0)
            val ended = extras.getLong(ENDED, 0)
            for (at in listOf(stale, ended)) if (at > now) next = minOf(next, at)
            val state = when {
                ended != 0L -> "ended"
                stale != 0L && stale <= now -> "stale"
                else -> "active"
            }
            val promoted = Build.VERSION.SDK_INT >= 36 && posted.notification.flags and Notification.FLAG_PROMOTED_ONGOING != 0
            list.put(JSONObject().put("id", posted.tag).put("kind", kind).put("state", states.getString(posted.tag, null) ?: "null")
                .put("activityState", state).put("token", JSONObject.NULL).put("promoted", promoted))
        }
        // The state of an activity the shade no longer shows, nor a send of this process is about to.
        val kept = shown.mapTo(HashSet()) { it.tag } + pending.keys
        states.all.keys.filter { it !in kept }.takeIf { it.isNotEmpty() }?.let { gone -> states.edit().apply { gone.forEach { remove(it) } }.apply() }
        main.removeCallbacks(deadline)
        if (next != Long.MAX_VALUE) main.postDelayed(deadline, next - now + 1)
        Exact.activities(JSONObject().put("activities", list).put("frequentPushesEnabled", false).toString())
    }
}
