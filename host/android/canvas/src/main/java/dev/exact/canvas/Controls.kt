package dev.exact.canvas

import android.app.KeyguardManager
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import org.json.JSONArray

/**
 * A control in Quick Settings (LLP 1126.004 D5): the tile of one `controls` row of app.json, which the build generates
 * as a subclass naming the row's kind, the bake's dispatch digest and how the bake placed it (`app/build.gradle.kts`).
 * The system draws it in its inactive appearance, with the row's description under its title, and binds it with
 * `BIND_QUICK_SETTINGS_TILE`; its [onClick] is the person's press.
 *
 * - `open`, a URL control: the app opens at the location, as a notification's tap opens one ([Activations.intent]);
 *   before API 34 a locked phone's opens once the person unlocks ([ControlPress]).
 * - `tool`: the press runs the tool where it is ([Controls.press]), in a host [Headless] boots when no activity shows
 *   the app, as origin `action` even with one. On a Lock Screen that holds a credential, `required` and `local` ask
 *   the person for it and press once they have ([ControlPress]); `always` presses there.
 * - `foreground`, a tool whose reach starts a capture: a tile click grants no while-in-use microphone, so the press
 *   opens the app and is pressed from it in front ([Controls.handoff]), still as origin `action`: the app in front is
 *   what Android needs to hand the press the microphone, not a grant of the app's authority. `always` refuses on a
 *   Lock Screen that holds a credential, since the app cannot come to the front there. Before API 34, whose launch
 *   asks for no credential, a locked `required` or `local` one opens the app once the person unlocks ([ControlPress]).
 *
 * [Tile service](https://developer.android.com/reference/android/service/quicksettings/TileService)
 */
abstract class ControlTile(
    private val kind: String,
    private val digest: String,
    private val intent: String,
    private val url: String?,
    private val authentication: String,
    private val description: String?,
) : TileService() {
    override fun onStartListening() {
        super.onStartListening()
        val tile = qsTile ?: return
        tile.state = Tile.STATE_INACTIVE
        tile.subtitle = description
        tile.updateTile()
    }

    override fun onClick() {
        super.onClick()
        // A press is the person's last only until they press another tile.
        Controls.superseded()
        // `required` and `local` ask for the device's credential where unlocking needs it now
        // ([KeyguardManager.isDeviceLocked]); a keyguard with none to ask (a swipe, or Smart Lock's trust) is no lock to
        // them.
        val locked = getSystemService(KeyguardManager::class.java)?.isDeviceLocked == true
        when (intent) {
            // Each launch its own `PendingIntent`: a one-shot one another tile or a notification made never matches.
            // From API 34 starting the app asks the person to unlock first, as any activity a tile starts on the Lock
            // Screen does. Before it the `Intent` overload starts the activity at once, behind a keyguard that holds a
            // credential and with nothing asking for it: the app opens from [ControlPress] once the person unlocks.
            "open" -> when {
                url.isNullOrEmpty() -> Controls.refuse(kind, "the control has no location")
                locked && Build.VERSION.SDK_INT < 34 -> launch(ControlPress.intent(this, kind, digest, url = url))
                else -> launch(Controls.location(this, url))
            }
            "foreground" -> when {
                locked && authentication == "always" -> Controls.refuse(kind, "the app records only in front, and the phone is locked")
                locked && Build.VERSION.SDK_INT < 34 -> launch(ControlPress.intent(this, kind, digest, opens = true))
                else -> launch(Controls.handoff(this, kind, digest))
            }
            else -> when {
                authentication == "always" -> Controls.press(this, kind, digest, SystemClock.elapsedRealtime(), unlocked = false)
                !locked -> Controls.press(this, kind, digest, SystemClock.elapsedRealtime(), unlocked = true)
                // `unlockAndRun`'s continuation waits for SystemUI to bind the tile again, by when the process may be
                // frozen; an activity the tile starts runs once the person unlocks, and presses where it is.
                else -> launch(ControlPress.intent(this, kind, digest))
            }
        }
    }

    /** Start the app's activity and close the shade: from API 34 by an immutable one-shot `PendingIntent`, which the
     * `Intent` overload is refused for. */
    private fun launch(intent: Intent) {
        if (Build.VERSION.SDK_INT >= 34) {
            val flags = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_ONE_SHOT
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, flags))
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}

/**
 * A control's press reaching the app (LLP 1126.004 D2, D5), on the main thread.
 *
 * A tile that opens the app ([handoff]) leaves a one-use record in this process and names it in the intent it starts,
 * which goes to `.Control`, an `<activity-alias>` with `android:exported="false"`. [receive] takes the record off the
 * intent and consumes it before anything else: a replayed intent, a recreated activity's or one in a process that
 * replaced the tile's finds none and is dropped. The press waits ([ready]) until the activity is resumed, has drawn a
 * frame of the running generation since it last started, the phone is unlocked and the data module is active, then
 * runs. A later press, the activity's stop, a data module that failed, or [BOUND_MS] from the click drops it. Once
 * running, a press is one call in the runner, and a later press, its activity's stop or the screen turning off end
 * that call ([end]); its bound ends it there too.
 */
internal object Controls {
    const val ALIAS = "dev.exact.app.Control"
    private const val TOKEN = "dev.exact.control.token"
    /** How long a press may take to reach an app ready to run it: the host's boot bound. */
    private const val BOUND_MS = 20_000L

    /** A tile click, bound to its deadline and superseded by the next click. */
    class Record(
        val token: String, val kind: String, val digest: String, val at: Long, val cold: Boolean,
        private val opens: Boolean = false, private val url: String? = null,
    ) {
        private val turn = pressed
        val remaining get() = at + BOUND_MS - SystemClock.elapsedRealtime()
        fun late() = remaining <= 0
        fun current() = turn == pressed && !late()
        val opensApp get() = opens || url != null

        fun unlocked(activity: android.app.Activity) {
            if (!interactive(activity)) return
            when {
                url != null -> activity.startActivity(location(activity, url))
                opens -> activity.startActivity(handoff(activity, kind, digest, at))
                else -> press(activity.applicationContext, kind, digest, at, unlocked = true, cold = cold)
            }
        }
    }
    private fun record(
        context: Context, kind: String, digest: String, at: Long = SystemClock.elapsedRealtime(), opens: Boolean = false, url: String? = null,
    ) = Record(java.util.UUID.randomUUID().toString(), kind, digest, at, !Exact.live, opens, url).also { click ->
        watchScreenOff(context, click.remaining) { if (click.current()) superseded("the screen turned off") }
    }

    /** The intent a URL control starts: the app at [url], through the notification alias. */
    fun location(context: Context, url: String?): Intent =
        Activations.intent(context, "", "", JSONArray(), url).setIdentifier(java.util.UUID.randomUUID().toString())
    /** The last tile press's record, until its intent arrives: a press supersedes the one before it. */
    private var issued: Record? = null
    /** The record an arrived intent named, until the activity it reached is ready to run it. */
    private var pending: Record? = null
    private var receiver: java.lang.ref.WeakReference<ExactActivity>? = null
    /** The record whose data module [ready] is waiting for. */
    private var asking: Record? = null
    /** The locked press waiting for its activity to take it. */
    private var unlocking: Record? = null
    private var unlockActivity: java.lang.ref.WeakReference<ControlPress>? = null
    private val main = Handler(Looper.getMainLooper())

    /** The intent a tile starts to press [kind] from the app in front, superseding any press waiting: its record's
     * token, also the intent's identifier, so no `PendingIntent` another press made matches it. Starting it asks the
     * person to unlock first. [at] is when the tile was pressed. */
    fun handoff(context: Context, kind: String, digest: String, at: Long = SystemClock.elapsedRealtime()): Intent {
        val record = record(context, kind, digest, at).also { issued = it }
        return Intent().setComponent(ComponentName(context.packageName, ALIAS)).setIdentifier(record.token)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(TOKEN, record.token)
    }

    /** The one-use token [ControlPress] runs a locked press by; one that [opens] the app hands it off from there, and
     * one with a [url] opens the app there. */
    fun unlockPress(context: Context, kind: String, digest: String, opens: Boolean, url: String?): String {
        return record(context, kind, digest, opens = opens, url = url).also { unlocking = it }.token
    }

    /** Take the record on receipt. Only this activity can finish it, until its deadline or the next click. */
    fun takeLocked(token: String, activity: ControlPress): Record? =
        unlocking?.takeIf { it.token == token }?.also {
            unlocking = null
            unlockActivity = java.lang.ref.WeakReference(activity)
        }

    /** The runner's call of the press now running, and the activity a press that opened the app reached; null for
     * none. Whatever ends the press ends this call ([end]). */
    private var running: Long? = null
    private var runningIn: java.lang.ref.WeakReference<ExactActivity>? = null

    /** End the running press now, saying [why]: the runner revokes it, and every call it made, its tasks' included, is
     * no longer the person's. */
    private fun end(why: String) {
        val id = running ?: return
        running = null
        runningIn = null
        if (Exact.pressLive(id)) journal("control: press $id ended: $why")
        // A press still queued behind other work is not yet live; the runner's cancel removes it too.
        Exact.cancelPress(id)
    }

    /** A press began as call [id], in [activity] where it opened the app: the screen turning off ends it. */
    private fun began(context: Context, id: Long?, activity: ExactActivity?) {
        running = id ?: return
        runningIn = activity?.let { java.lang.ref.WeakReference(it) }
        watchScreenOff(context, BOUND_MS) { if (running == id) end("the screen turned off") }
    }

    private fun watchScreenOff(context: Context, duration: Long, lost: () -> Unit) {
        val app = context.applicationContext
        val off = object : android.content.BroadcastReceiver() {
            override fun onReceive(c: Context, i: Intent) = lost()
        }
        val screen = android.content.IntentFilter(Intent.ACTION_SCREEN_OFF)
        if (Build.VERSION.SDK_INT >= 33) app.registerReceiver(off, screen, Context.RECEIVER_NOT_EXPORTED) else app.registerReceiver(off, screen)
        main.postDelayed({ app.unregisterReceiver(off) }, duration.coerceAtLeast(0))
    }

    /** Counts tile presses, so a press still waiting for its session knows a later one superseded it. */
    private var pressed = 0

    /** A tile was pressed: the press before it ends, and one still waiting for the app is dropped. */
    fun superseded(why: String = "another tile was pressed") {
        pressed += 1
        end(why)
        drop(why)
        unlocking?.let { journal("control:${it.kind}: $why") }
        unlocking = null
        unlockActivity?.get()?.takeUnless { it.isFinishing || it.isDestroyed }?.finish()
        unlockActivity = null
    }

    /** Drop the press waiting, if any, saying [why]. */
    private fun drop(why: String) {
        val earlier = pending ?: issued ?: return
        pending = null
        issued = null
        journal("control:${earlier.kind}: $why")
    }

    /** [activity] stopped: the press running in it ends, and one that reached it and waits is dropped, not run at a
     * later visit. */
    fun stopped(activity: ExactActivity) {
        if (runningIn?.get() === activity) end("the app left the front")
        if (receiver?.get() !== activity) return
        val record = pending ?: return
        pending = null
        journal("control:${record.kind}: the app left the front before the press ran, so it was dropped")
    }

    /** Whether [intent] carried a handoff: taken off it, and kept for [ready] when it names this process's last
     * record. */
    fun receive(activity: ExactActivity, intent: Intent): Boolean {
        val token = intent.getStringExtra(TOKEN) ?: return false
        intent.removeExtra(TOKEN)
        val record = issued?.takeIf { it.token == token }
        if (record != null) issued = null
        when {
            intent.component?.className != ALIAS -> journal("control: a press that did not come through the app's own tile was dropped")
            record == null || record.late() -> journal("control: a press already taken, superseded or from before this process was dropped")
            else -> {
                pending = record
                receiver = java.lang.ref.WeakReference(activity)
                main.postDelayed({ expire(record) }, record.at + BOUND_MS - SystemClock.elapsedRealtime())
                ready(activity)
            }
        }
        return true
    }

    /** Whether [activity] can run a handoff now: resumed, drawn since its start, and unlocked. */
    private fun eligible(activity: ExactActivity) =
        activity.resumedAndDrawn() && interactive(activity) && activity.getSystemService(KeyguardManager::class.java)?.isKeyguardLocked != true

    private fun interactive(context: Context) = context.getSystemService(android.os.PowerManager::class.java)?.isInteractive == true

    /** The activity may now be ready for a handoff: [eligible], its data module active. Checked again when the data
     * answers, and the bound with it. */
    fun ready(activity: ExactActivity) {
        val record = pending ?: return
        if (receiver?.get() !== activity || asking === record || !eligible(activity)) return
        val owner = Exact.owner ?: return
        asking = record
        // On the main thread, as every other change of [pending] is.
        owner.active { ok ->
            if (asking === record) asking = null
            if (pending !== record) return@active
            if (!ok) return@active drop("the app's data did not start, so the press was dropped")
            if (!eligible(activity)) return@active drop("the app left the front before the press ran, so it was dropped")
            if (record.late()) return@active expire(record)
            pending = null
            began(activity, Exact.press(record.kind, record.digest, foreground = true, cold = record.cold, hidden = false), activity)
        }
    }

    private fun expire(record: Record) {
        if (pending !== record) return
        pending = null
        journal("control:${record.kind}: the app was not ready in front within ${BOUND_MS / 1000} s, so the press was dropped")
    }

    /** A tile's press of a tool that runs where it is pressed: in the host that runs, or one booted for it. A session
     * not ready within [BOUND_MS] of the click at [at] ([SystemClock.elapsedRealtime]), a later tile press, or, with
     * [unlocked], a phone locked again meanwhile, drops it. */
    fun press(context: Context, kind: String, digest: String, at: Long, unlocked: Boolean, cold: Boolean = !Exact.live,
              session: (Context, () -> Boolean, (Boolean) -> Unit) -> Unit = Headless::session) {
        val turn = pressed
        var screenOff = false
        watchScreenOff(context, at + BOUND_MS - SystemClock.elapsedRealtime()) { screenOff = true }
        val live = { !screenOff && interactive(context) && SystemClock.elapsedRealtime() - at < BOUND_MS && pressed == turn }
        session(context, live) { ok ->
            if (!ok || !live()) {
                android.util.Log.w("exact", "control:$kind: no session ready for the press, or it lapsed or was superseded")
                return@session
            }
            if (unlocked && context.getSystemService(KeyguardManager::class.java)?.isDeviceLocked == true) {
                return@session journal("control:$kind: the phone locked again before the press ran, so it was dropped")
            }
            val hidden = Exact.reader?.get()?.resumedAndDrawn() != true
            began(context, Exact.press(kind, digest, foreground = false, cold = cold, hidden = hidden), null)
        }
    }

    /** A tile press refused before it reached the app. */
    fun refuse(kind: String, why: String) = journal("control:$kind: refused, $why")

    /** A line for the session's journal, or logcat in a process no host runs in yet. */
    private fun journal(line: String) {
        if (Exact.live) Exact.journal(line) else android.util.Log.i("exact", line)
    }
}

/**
 * A locked tile's press of a tool that runs where it is pressed (`required` and `local`): the tile starts this activity
 * once the person unlocks, and it presses and finishes, showing nothing, so the person stays where they were. Where it
 * is made behind the keyguard all the same (the `Intent` overload before API 34 starts it at once), it asks the system
 * to dismiss the keyguard and presses only once it has; there it also carries a locked press that opens the app, which
 * it hands off ([Controls.handoff]) once unlocked. Unexported, so only the app's own tile starts it.
 */
class ControlPress : android.app.Activity() {
    private val main = Handler(Looper.getMainLooper())

    override fun onCreate(state: android.os.Bundle?) {
        super.onCreate(state)
        val token = intent.getStringExtra(TOKEN)
        val press = token?.let { Controls.takeLocked(it, this) } ?: return finish()
        if (state != null) return finish()
        val keyguard = getSystemService(KeyguardManager::class.java) ?: return finish()
        // A recreated instance finishes at once (above); the one it replaced is destroyed, and its poll and callbacks
        // do nothing, so the press is dropped rather than run from it.
        fun pressed(lockPolls: Int = 0) {
            if (isDestroyed || isFinishing || isChangingConfigurations) return
            when {
                !press.current() -> finish()
                keyguard.isDeviceLocked && lockPolls > 0 -> main.postDelayed({ pressed(lockPolls - 1) }, 100)
                keyguard.isDeviceLocked -> finish()
                press.opensApp && keyguard.isKeyguardLocked -> main.postDelayed({ pressed() }, 100)
                else -> { press.unlocked(this); finish() }
            }
        }
        // The device reads locked for a moment after the unlock that started this activity: watched for a while before
        // asking for a dismissal the keyguard, already going, would never answer.
        fun unlocked(left: Int) {
            when {
                isDestroyed || isFinishing || isChangingConfigurations -> return
                !press.current() -> finish()
                !keyguard.isDeviceLocked -> pressed()
                left > 0 -> main.postDelayed({ unlocked(left - 1) }, 100)
                else -> {
                    keyguard.requestDismissKeyguard(this, object : KeyguardManager.KeyguardDismissCallback() {
                        override fun onDismissSucceeded() = pressed(20)
                        override fun onDismissCancelled() { if (!isDestroyed) finish() }
                        override fun onDismissError() { if (!isDestroyed) finish() }
                    })
                }
            }
        }
        main.postDelayed({ if (!isFinishing) finish() }, press.remaining.coerceAtLeast(0))
        unlocked(20)
    }

    override fun onDestroy() {
        main.removeCallbacksAndMessages(null)
        super.onDestroy()
    }

    internal companion object {
        private const val TOKEN = "dev.exact.control.token"

        fun intent(context: Context, kind: String, digest: String, opens: Boolean = false, url: String? = null): Intent {
            val token = Controls.unlockPress(context, kind, digest, opens, url)
            return Intent(context, ControlPress::class.java).setIdentifier(token)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
                .putExtra(TOKEN, token)
        }
    }
}
