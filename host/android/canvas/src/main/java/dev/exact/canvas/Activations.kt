package dev.exact.canvas

import android.app.NotificationManager
import android.app.RemoteInput
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import org.json.JSONArray
import org.json.JSONObject

/**
 * A person's act on the platform's own UI that an extension posted, such as a notification's tap (LLP 1133.004 D6;
 * LLP 1133.004.000 D4): the extension makes the intent the tap starts ([ExtensionContext.activation]) and the activity
 * hands it back here. It opens its `navigate` location as a link's, or raises the extension's event with a person's
 * origin.
 *
 * The intent goes to `.Notify`, an `<activity-alias>` of the app's activity with `android:exported="false"`. Only the
 * app, or the system acting on the app's `PendingIntent`, can start it, so an activation is read only from an intent
 * whose component is the alias, and needs no secret. `MainActivity` is exported: an activation extra on an intent to
 * it is another app's, and is dropped and journaled.
 *
 * A notification's action (LLP 1133.004 stage 2; LLP 1133.004.000 D5) is an activation too, which carries the
 * declared action's record: the activity writes the action's name and the person's reply into the event argument
 * `chosen`, as Apple's host does, and closes the notification, which Android leaves showing after an action. The
 * runner raises it only while the app's plan still declares that record, so a button an update removed or changed
 * raises nothing.
 *
 * Raised once the first frame is drawn: a tap that starts the process waits for the session that hears it. Main
 * thread.
 */
internal object Activations {
    /** The alias the shell's manifest declares. */
    const val ALIAS = "dev.exact.app.Notify"
    private const val EXTENSION = "dev.exact.activation.extension"
    private const val EVENT = "dev.exact.activation.event"
    private const val ARGS = "dev.exact.activation.args"
    private const val ACTION = "dev.exact.activation.action"
    private const val CHOSEN = "dev.exact.activation.chosen"
    private const val CLOSE_TAG = "dev.exact.activation.close.tag"
    private const val CLOSE_ID = "dev.exact.activation.close.id"
    /** The key of an action's text field (`RemoteInput`). */
    const val REPLY = "exact.reply"

    /** The first frame was drawn in this process: an activation is raised at once. */
    private var ready = false
    /** What arrived before it: an event to raise, with the declared action it names, or a journal line (`event`
     * null). */
    private class Held(val extension: String, val event: String?, val args: String, val action: String? = null)
    private val held = ArrayList<Held>()

    /** The intent a tap starts: the alias, at [navigate] when given, else raising [event] of [extension] with [args]. */
    fun intent(context: Context, extension: String, event: String, args: JSONArray, navigate: String?): Intent {
        val intent = Intent().setComponent(ComponentName(context.packageName, ALIAS))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        if (!navigate.isNullOrEmpty()) return intent.setAction(Intent.ACTION_VIEW).setData(Uri.parse(navigate))
        return intent.putExtra(EXTENSION, extension).putExtra(EVENT, event).putExtra(ARGS, args.toString())
    }

    /** The intent [action], a declared action's record, starts: [event] with [args], the action's name and the reply
     * written into `args[chosen]`; the notification posted as [tag] and [id] is closed. */
    fun action(context: Context, extension: String, event: String, args: JSONArray, chosen: Int, action: JSONObject, tag: String, id: Int): Intent =
        intent(context, extension, event, args, null).putExtra(ACTION, action.toString()).putExtra(CHOSEN, chosen)
            .putExtra(CLOSE_TAG, tag).putExtra(CLOSE_ID, id)

    /** Whether [intent] reached the app through the alias. */
    fun aliased(intent: Intent): Boolean = intent.component?.className == ALIAS

    /**
     * Whether [intent] carried an activation: taken off it, so a recreated activity does not raise it again. One that
     * came through the alias is raised; one that came to the exported activity is dropped and journaled.
     */
    fun receive(context: Context, intent: Intent): Boolean {
        val extension = intent.getStringExtra(EXTENSION) ?: return false
        val event = intent.getStringExtra(EVENT)
        val args = intent.getStringExtra(ARGS) ?: "[]"
        val action = intent.getStringExtra(ACTION)?.let { runCatching { JSONObject(it) }.getOrNull() }
        val chosen = intent.getIntExtra(CHOSEN, -1)
        val close = intent.getStringExtra(CLOSE_TAG)
        val id = intent.getIntExtra(CLOSE_ID, 0)
        for (key in listOf(EXTENSION, EVENT, ARGS, ACTION, CHOSEN, CLOSE_TAG, CLOSE_ID)) intent.removeExtra(key)
        val raised = when {
            !aliased(intent) || event == null ->
                Held(extension, null, "an activation of $extension that did not come through the app's own notification was dropped")
            action == null -> Held(extension, event, args)
            else -> {
                if (close != null) context.getSystemService(NotificationManager::class.java)?.cancel(close, id)
                // The reply of an action with a text field: what the person typed, "" for nothing; none without one.
                val reply = if (action.isNull("placeholder")) null
                    else RemoteInput.getResultsFromIntent(intent)?.getCharSequence(REPLY)?.toString() ?: ""
                Held(extension, event, choosing(args, chosen, action.getString("action"), reply), action.toString())
            }
        }
        if (ready) raise(raised) else held.add(raised)
        return true
    }

    /** [args] with [action] and [reply] written into the argument at [chosen], as Apple's host writes them. */
    private fun choosing(args: String, chosen: Int, action: String, reply: String?): String {
        val list = runCatching { JSONArray(args) }.getOrNull() ?: return args
        val event = list.optJSONObject(chosen) ?: return args
        event.put("action", action).put("reply", reply ?: JSONObject.NULL)
        return list.toString()
    }

    /** The first frame: what a launching tap carried is raised now. */
    fun started() {
        if (ready) return
        ready = true
        held.forEach(::raise)
        held.clear()
    }

    private fun raise(a: Held) {
        try {
            if (a.event != null) Exact.activate(a.extension, a.event, a.args, a.action) else Exact.journal(a.args)
        } catch (e: UnsatisfiedLinkError) {
            android.util.Log.w("exact", "activation: no session")
        }
    }
}
