package dev.exact.canvas

import android.app.Activity
import android.content.Context
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject

/**
 * A package's implementation of a module on Android (LLP 1133.001 D1, D5). The APK's build compiles each bound
 * implementation's Kotlin into the app and writes `exact.app.ExactExtensions`, an [ExtensionRoster] naming each
 * extension's class; the reader makes one per session at the extension's first call. Calls come from the runner as
 * requests (kind 11, [Requests]) and every one is on the main thread. An error is answered as `{"error": code}`.
 */
abstract class ExactExtension(val context: ExtensionContext) {
    /** A source's read or a send, by its declared name, with the keyword arguments the call wrote: answered once. */
    abstract fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit)

    /**
     * [later] with the agent's preferred value for [name], under the agent (LLP 1133.009 D10 item 11): the runner also
     * lays it over the answer. An implementation that is its own substitute overrides this one.
     */
    open fun later(name: String, args: JSONObject, preferred: Any?, reply: (JSONObject) -> Unit) = later(name, args, reply)

    /**
     * [later] under the call's [cause] (LLP 1133.023 D7a). An implementation whose answer depends on who called, or
     * that registers work acting for the call later (a track's subscription, a permission's answer, a prompt),
     * overrides this one and hands [cause] to that work.
     */
    open fun later(name: String, args: JSONObject, preferred: Any?, cause: Cause, reply: (JSONObject) -> Unit) =
        later(name, args, preferred, reply)

    /** The last reader of live source [name] is gone: stop what it started (LLP 1133.002 D5). */
    open fun release(name: String) {}

    /**
     * No reader of live source [name] reads it with [args] any longer (LLP 1133.009 D10 item 12); a reader asked again
     * has its new arguments first.
     */
    open fun release(name: String, args: JSONObject) {}

    /**
     * The agent's preferred values for this extension's names, by name, when it is bound under the agent and
     * whenever one changes (LLP 1133.008 D8 item 9): an implementation that is its own substitute answers by them.
     */
    open fun preferred(values: JSONObject) {}

    /** Refresh a substitute at the session clock before a drive reads its frames. */
    open fun clock() {}

    /** Handles a reply carried that the runner dropped, never the app's (LLP 1133.009 D10 item 10). */
    open fun dispose(ids: List<String>) {}

    /** The send numbered [request] ([Cause.request]) was let go: what it started stops (LLP 1133.016 D15
     * item 2). */
    open fun cancel(request: Int) {}

    /** The app went to the foreground or left it ([ExtensionContext.foreground], LLP 1133.009 D10 item 3). */
    open fun foreground(shown: Boolean) {}

    /** The wake [ExtensionContext.wake] raised for [id] ended with [outcome] (LLP 1133.017 D8), after its `ended`. */
    open fun woke(id: String, outcome: String) {}

    /** The activity is in front again: what changed in Settings meanwhile is read again (Android has no event). */
    open fun resumed() {}

    /** [Activity.onPause]: what only the activity in front may hold is let go, such as NFC reader mode, which
     * [android.nfc.NfcAdapter.enableReaderMode] admits only while the activity is resumed (LLP 1133.035 D16 item 5). */
    open fun paused() {}

    /**
     * Picture [source] of the interface's `img` scheme (`photos:<id>`, LLP 1133.019 D2, D11 item 1) for a box of
     * [width] by [height] device pixels at `object-fit` [fit], once the runner admitted it: [reply] once, from any
     * thread, with the picture upright and no larger than the box needs, or null and an error code (`NotFoundError`),
     * which empties the box. [owned]: admitted as the app's own item alone (D6), drawn only where the platform says it
     * is.
     */
    open fun picture(source: String, width: Int, height: Int, fit: String, owned: Boolean, reply: (android.graphics.Bitmap?, String?) -> Unit) =
        reply(null, "NotSupportedError")

    /** The state the runner holds of each permission this extension's interfaces declare, by name, told with
     * [preferred] under the agent (LLP 1133.019 §10). */
    open fun permissions(states: JSONObject) {}

    /**
     * The session ends: finish what must be finished, then [done] (LLP 1133.009 D10 item 1). Android's session lives
     * as long as its process, which ends without calling this.
     */
    open fun destroy(done: () -> Unit) = done()

    /**
     * Make view [tag], one of the package's `views`, for node [id] of [session] (LLP 1133.008 D8 item 1): the one
     * extension of the process makes every session's views, so it keeps their state by session, and [id] is what its
     * reports name. [emit] sends an event (`message`, `load`, …) with its text. A report can come before the view is
     * made, since the reader attaches views after first pixel.
     */
    open fun view(tag: String, session: String, id: String, context: Context, props: JSONObject, emit: (String, String) -> Unit): android.view.View =
        throw IllegalArgumentException("${javaClass.name} serves no view $tag")

    /** Every prop of a view [view] made, replaced. */
    open fun update(view: android.view.View, props: JSONObject) {}

    /** The view's node is gone. */
    open fun destroyView(view: android.view.View) {}
    /** The hosted view's activity lifecycle, separate from the session's foreground fact (LLP 1133.021). */
    open fun viewLifecycle(view: android.view.View, event: ViewLifecycle, state: android.os.Bundle?) {}

    /**
     * View [id] of [session] was displayed or hidden, by [cause], the commit that did it (LLP 1133.008 D8 item 2).
     */
    open fun displayed(session: String, id: String, shown: Boolean, cause: Cause) {}

    /** Prop [prop] of view [id], which binds a device, changed, by [cause], the commit that changed it. */
    open fun bound(session: String, id: String, prop: String, cause: Cause) {}

    /** [bound], with the value the commit set ([value], as JSON), by which an implementation pairs the report with the
     * prop its view is given, which reaches it separately. */
    open fun bound(session: String, id: String, prop: String, value: Any?, cause: Cause) = bound(session, id, prop, cause)

    /**
     * A text field whose `autocomplete` ends in `webauthn` was made ([present]) or went (LLP 1133.011 D10 item 4):
     * the real view, on which a conditional credential request is set. Every live one is told to an extension when it
     * is made.
     */
    open fun webauthnField(view: android.widget.EditText, present: Boolean) {}

    /**
     * The canvases of [surface], which this extension's input feeds ([ExtensionSurfaces]), were displayed or hidden,
     * by [cause], the commit that did it (LLP 1133.013 D6, D12 item 3). The lease: hidden, the input pauses; shown by
     * the app's commit, it resumes; shown by any other's, it stays paused. A start on a hidden canvas is told this
     * before the start.
     */
    open fun displayed(surface: String, shown: Boolean, cause: Cause) {}

    /** The display's angle changed ([ExtensionContext.orientation]). */
    open fun orientation(angle: Int) {}
}

/**
 * The authenticator entry (LLP 1133.010 D13): what an implementation whose interface declares `authenticator` serves,
 * called by the reader's presence backend ([Presence]) in this process on the main thread. The prompt has one owner,
 * the implementation; the key and its bytes stay the reader's.
 */
interface ExactAuthenticator {
    /**
     * Show the platform's prompt over the resumed activity for [authenticators] (`BiometricManager.Authenticators`),
     * with [reason] already translated, [crypto] its `CryptoObject` (null for a check alone); [origin] is who made the
     * call. [done] once, on the main thread: the authenticated object, or a failure `{"error": code, "message"}`.
     */
    fun authenticate(crypto: android.hardware.biometrics.BiometricPrompt.CryptoObject?, authenticators: Int, reason: String, origin: String,
                     done: (android.hardware.biometrics.BiometricPrompt.CryptoObject?, JSONObject?) -> Unit)

    /** A presence name's key builder, before generation: the key hatch (D11). What the backend owns it sets again after. */
    fun configure(builder: android.security.keystore.KeyGenParameterSpec.Builder) {}
}

/** The app's bound extensions by name, each with how to make it: what the build writes as `exact.app.ExactExtensions`. */
interface ExtensionRoster {
    val extensions: Map<String, (ExtensionContext) -> ExactExtension>
    /** Each view tag a package serves, with the extension that makes it (LLP 1133.001 D1). */
    val views: Map<String, String> get() = emptyMap()
    /** The extensions whose interface says `"start": "load"`: made after the reader's first frame rather than at their
     * first call (LLP 1133.015 D16 item 1). */
    val startAtLoad: List<String> get() = emptyList()
    /** Each extension's configuration as JSON, by extension ([ExtensionContext.config]). */
    val config: Map<String, String> get() = emptyMap()
}

/**
 * Who made a call (LLP 1133.023 D7a), fixed when it arrives: [ExactExtension.later] is handed it, and what the call
 * registers (a track's subscription, a permission's answer, a prompt) keeps it and acts under it however late it runs.
 * Work no call made has [none], which no guarded capability admits; nothing reads as the app's by default.
 */
class Cause internal constructor(
    /** `app`, `tool`, `wake`, `action` or `device` (LLP 1133.002 D12); `press`, a control's press the runner admitted
     * to start an audio capture (LLP 1126.004 D4); `none`, no call's. */
    val origin: String,
    /** The calling source's grants; a retry keeps this ceiling. */
    val authority: String = "",
    /**
     * Whether the call has a person's activation: the app made it within a few seconds of a press or a key the
     * session heard, HTML's [transient activation](https://html.spec.whatwg.org/multipage/interaction.html#transient-activation).
     * A timer's or an answer's code inside that window has it too, as on the web; a tool's or a wake's call never
     * does. It says nothing of whether an activity shows ([ExtensionContext.visible]).
     */
    val userActivation: Boolean = false,
    /** The control's press a `press` call is the capture of (LLP 1126.004 D4); null for any other. */
    val press: Long? = null,
    /** The number the runner gave the send, which a later [ExactExtension.cancel] names; null for a source (LLP
     * 1133.016 D15 item 2). */
    val request: Int? = null,
) {
    /** The person's act in the app, or its boot: what may ask, prompt or open a device in the foreground. */
    val app: Boolean get() = origin == "app"

    /** Whether [press] still runs, asked of the runner where a capture acquires the device: a call can wait on the
     * main thread past the press's end. */
    fun pressLive(): Boolean = press?.let { Exact.pressLive(it) } == true

    override fun toString() = origin

    /** The less privileged of this and [other], by [ORDER]: what two commits that both made something may do. */
    fun lower(other: Cause): Cause = if (ORDER.indexOf(other.origin) < ORDER.indexOf(origin)) other else this

    companion object {
        /** Origins, least privileged first; an unknown one ranks as `none`. */
        private val ORDER = listOf("none", "device", "wake", "tool", "action", "press", "app")

        /** No call's: a callback the host makes on its own. */
        @JvmField val none = Cause("none")

        /** A request's cause, as the runner writes it (`modules/bound.rs`); one that names no origin is [none]. */
        internal fun of(request: JSONObject): Cause {
            val origin = request.optString("origin").ifEmpty { return none }
            // Only an admitted capture call carries a press grant. A view hears the commit's ordinary origin.
            if (origin == "press" && request.has("view")) return Cause("action")
            return Cause(origin, request.optString("authority"), request.optBoolean("userActivation"),
                if (request.has("press")) request.getLong("press") else null,
                if (request.has("request")) request.getInt("request") else null)
        }
    }
}

/** What an extension may use of the host. */
class ExtensionContext internal constructor(
    val application: Context,
    private val name: String,
    /** A test's configuration in place of the generated roster's. */
    private val configured: JSONObject? = null,
) {
    /** The activity that shows the app now, if one does: a permission's rationale is read from it. */
    val activity: Activity? get() = Extensions.front()

    /** The trust with which the APK was built. */
    val development: Boolean get() = Setup.development

    /**
     * Under the agent, unless the drive bound this extension as in production (`--bind`): an implementation that is its
     * own substitute then opens no device and asks the OS for nothing (LLP 1133.006 D8).
     */
    val agent: Boolean = try { Native.underAgent(name) } catch (e: UnsatisfiedLinkError) { false }

    /**
     * The package's configuration (LLP 1133.021 §9 item 7): each key its interface's `exact.json` declares under
     * `config.android` that the app's `app.json` gives under `packages.<interface>.android`, a key typed `env` as the
     * variable's value when the APK was built. A key given no value is absent. Read at build: changing one is a new APK.
     */
    val config: JSONObject by lazy { configured ?: Extensions.config(name) }

    /** The session this instance belongs to. Android runs one session a process. */
    val session: String = "1"

    /** The session's label, the same at every launch, which what a package keeps for one session is keyed by: `main`,
     * the one session a process runs here, as an Apple app's window is. */
    val label: String = "main"

    /** The session's directories behind `app:/data`, `app:/cache` and `app:/tmp`: the app's own, or under the agent its
     * drive's store (a named one, `EXACT_AGENT_STORAGE`, kept across the drive's launches). */
    val data: java.io.File get() = root(0)
    val cache: java.io.File get() = root(1)
    val temporary: java.io.File get() = root(2)
    /**
     * Whether those directories are a drive's store and not the app's own, which holds under the agent whether or not
     * the drive bound this extension (`--bind`, [agent]). What an implementation keeps outside them (a job, a record
     * in the app's private files) it keeps apart from the app's by this, never by [agent]. A library with no such
     * entry point is not this build's, and the question throws: the app's storage is never assumed.
     */
    val scratch: Boolean get() = Native.scratchStorage()
    private fun root(index: Int): java.io.File =
        java.io.File(try { Native.appRoot(index) } catch (e: UnsatisfiedLinkError) { null } ?: application.cacheDir.path)

    /** The session's track table, which its implementations share (LLP 1133.009 D10 item 6). */
    val tracks: Tracks get() = Extensions.tracks

    /** The process's audio focus owner, through which a module that plays or captures claims focus (LLP 1133.012
     * D12 item 2). */
    val audioFocus: AudioFocus get() = AudioFocus

    /** Where an admitted `app:/` path names (LLP 1133.009 D10 item 4): the app's own directories, a drive's scratch tree
     * under the agent. Admission checked the path against the calling source's grants. Each of the three answers null
     * where a part of the path is a symbolic link, and is reached through none when it answers; nothing inside a
     * directory is checked, so a module empties one with [Files.removeTree], never [java.io.File.deleteRecursively]. */
    private fun place(path: String): java.io.File? = try { Native.appFile(path)?.let { java.io.File(it) } } catch (e: UnsatisfiedLinkError) { null }

    /** A file for the module to write ([place]): a regular file, or nothing there yet; null for a directory. */
    fun file(path: String): java.io.File? = place(path)?.takeUnless { it.isDirectory }

    /** A file for the module to read ([place]): a regular file, else null. */
    fun fileToRead(path: String): java.io.File? = place(path)?.takeIf { Files.regular(it) }

    /** A directory for the module to fill or empty ([place]): a directory, or nothing there yet; null for a file. */
    fun directory(path: String): java.io.File? = place(path)?.takeUnless { it.isFile }

    /** A content URI for the file an admitted `app:/` path names, which an intent may carry with a read grant
     * (`FLAG_GRANT_READ_URI_PERMISSION`), or null when it names none ([Files], LLP 1133.024 D22 item 2). */
    fun contentUri(path: String): android.net.Uri? = Files.uri(application, path)

    /** Whether the activity is resumed. */
    val resumed: Boolean get() = Extensions.resumedNow

    /** Whether the activity shows, resumed or paused under a dialog: Android lets an app with a visible activity start
     * another (["Restrictions on starting activities from the background"](https://developer.android.com/guide/components/activities/background-starts)),
     * as Credential Manager starts its sheet. */
    val visible: Boolean get() = Extensions.visibleNow

    /**
     * [go] with the activity once it is resumed, on the main thread: at once when it is; asked while it shows but is
     * paused, as it is when a permission dialog's answer arrives (`onRequestPermissionsResult` comes before
     * `onResume`), at the resume. For a prompt the platform keeps only over the app's own top activity: SystemUI
     * dismisses a `BiometricPrompt` whose app is not the top activity of a visible task (LLP 1133.010 D8). [failed]
     * has `InvalidStateError` with no activity showing, `AbortError` when the app stopped before the resume. The
     * function returned withdraws a wait, answering neither.
     */
    fun whenResumed(failed: (String) -> Unit, go: (Activity) -> Unit): () -> Unit = Extensions.whenResumed(failed, go)

    /** Whether the app is in the foreground, as the runner's admission reads it ([ExactExtension.foreground]). */
    var foreground = true
        internal set

    /** The session's clock, ms: the agent's under the agent (LLP 1133.009 D10 item 2). */
    fun now(): Double = Native.now()

    /** Run once on the session clock, including a drive's virtual clock. The returned function cancels it. */
    fun after(ms: Double, body: () -> Unit): () -> Unit {
        val reader = (activity as? ExactActivity)?.hatches(required = true) ?: throw TrackError("InvalidStateError")
        val ticket = reader.register(0, 6, ms.toString(), Call.Fired(body))
        return { ticket.stop() }
    }

    /** The page's canvases, whose input an extension may feed (LLP 1133.013 D6). */
    val surfaces = ExtensionSurfaces()

    /** Claims on the host's foreground services, which keep a capability running while the app is hidden (LLP
     * 1133.033 D2). */
    val foregroundService = ForegroundServices(application, name) { agent }

    /**
     * The display's angle as the web's `screen.orientation.angle` (0, 90, 180, 270; LLP 1133.013 D12 item 4), from
     * `Display.getRotation`: [ExactExtension.orientation] is told each change.
     */
    val orientation: Int get() = Extensions.angle(activity)

    /** The readers of [topic] are asked again (LLP 1016.002). */
    fun changed(topic: String) = Exact.changed(topic)

    /**
     * Raise the interface's event [event] with [args] (LLP 1133.004 D6): delivered in order, once, with the device's
     * origin. Its one channel to the runner is [changed], as on Apple (`exact-event:`,
     * `runner/src/runner/modules/events.rs`); the sequence number keeps two alike from coalescing. Main thread.
     */
    fun emit(event: String, args: JSONArray) = Extensions.emit(name, event, args)

    /**
     * Under the agent, what the implementation discloses of its own objects for `state.extensions` (LLP 1133.008 D8
     * item 9): listed as its `implementation`, each field replacing the one of its name said before. Through
     * [changed], as on Apple (`exact-state:`, `runner/src/runner/modules/disclosed.rs`). Nothing outside the agent.
     */
    fun state(fields: JSONObject) {
        if (agent) changed("exact-state:" + JSONObject().put("extension", name).put("state", fields))
    }

    /** The process's pads (LLP 1133.022 D10 item 1), which a world canvas reads too. */
    val pads = ExtensionPads(application)

    /** Each [wake]'s `ended`, by id, until the runner tells its outcome. */
    internal val ended = HashMap<String, (String) -> Unit>()

    /**
     * Raise the interface's wake event [event] with [args] for the platform's own [id] (LLP 1133.017 D8): the events
     * of one id are one wake, run with the wake's origin, and [ended] hears its outcome once, on the main thread:
     * `completed`, `expired`, `superseded`, `refused`, or `unheard` when no handler hears it. With no activity
     * showing, `completed` says the store's writes are on disk too, and a wake whose writes are not is `expired`.
     * Its one channel to the runner is [changed], as on Apple (`exact-wake:`, `runner/src/runner/wakes.rs`).
     */
    fun wake(event: String, args: JSONArray, id: String, ended: (String) -> Unit = {}) {
        this.ended[id] = ended
        changed("exact-wake:" + JSONObject().put("extension", name).put("event", event).put("args", args).put("id", id))
    }

    /** The platform ended custody of this wake, including one still queued for delivery. */
    fun expireWake(id: String) {
        changed("exact-wake-expired:" + JSONObject().put("extension", name).put("id", id))
    }

    /** Every `img` showing picture [source] is emptied and fires `error`, and no reply in flight draws (LLP 1133.019
     * D2): the photo left what the app may read. */
    fun revokePicture(source: String) = Exact.pictureChanged(source, true)

    /** Picture [source] is asked for again: the photo was edited. */
    fun changedPicture(source: String) = Exact.pictureChanged(source, false)

    fun log(line: String) { Log.i("exact", line) }

    /** A line for the session's journal, which the agent's `logs` reads. Main thread. */
    fun journal(line: String) = Exact.journal(line)

    /**
     * The intent a tap on what this extension posts starts (a notification's content intent or action, LLP 1133.004
     * D6; LLP 1133.004.000 D4): the app's unexported alias, which comes forward and opens [navigate] as a link's
     * location, or, without one, raises the interface's event [event] with [args] with a person's origin, once the
     * session can hear it. A `PendingIntent` made from it is the app's alone to send.
     */
    fun activation(event: String, args: JSONArray, navigate: String? = null): android.content.Intent =
        Activations.intent(application, name, event, args, navigate)

    /**
     * The platform's dialog for [permissions], in one request (`Activity.requestPermissions`), then [done], on the main
     * thread: when the result comes, or when the activity resumes without it (a result lost to recreation, LLP 1133.002
     * D9). Overlapping requests queue in order; identical permission sets share a result. [done] receives only the
     * names the platform answered. An empty set means no result, so callers must not record a denial. With no
     * activity in front and no request pending, nothing is asked and [done] runs at once, as it does for a [cause]
     * that is not the app's own: only the person's act in the app asks (LLP 1133.002 D12).
     */
    fun requestPermissions(cause: Cause, permissions: Array<String>, done: (Set<String>) -> Unit) =
        if (!cause.app) done(emptySet()) else Extensions.ask(permissions, done)

    /**
     * Runtime permission [name]'s state, from the one history every module reads (LLP 1133.008 D8 item 7): `granted`;
     * `denied` once a request was refused and the platform owes no rationale after it, so it will not ask again; else
     * `prompt`. A grant in Settings clears the history.
     */
    fun permission(name: String): PermissionState {
        val granted = application.checkSelfPermission(name) == android.content.pm.PackageManager.PERMISSION_GRANTED
        val history = application.getSharedPreferences("exact.permissions", Context.MODE_PRIVATE)
        if (granted && history.getBoolean(name, false)) history.edit().remove(name).apply()
        val rationale = !granted && activity?.shouldShowRequestPermissionRationale(name) == true
        val permanent = !granted && history.getBoolean(name, false)
        return PermissionState(if (granted) "granted" else if (permanent) "denied" else "prompt", rationale, permanent)
    }

    /**
     * Start [intent] for its result (`Activity.startActivityForResult`), then [done] on the main thread with the
     * result code and the result's data (LLP 1133.018 D13 item 2): a picker's or an editor's answer. A result that
     * reaches an activity made again meanwhile settles the call, since the call is the process's.
     *
     * The activity starts over the app's resumed activity. Asked while that one shows but is paused, as it is when a
     * permission dialog's result arrives (`onRequestPermissionsResult` comes before `onResume`), the start waits for
     * the resume; the app stopping first ends the wait. When nothing starts, [failed] has why, as a DOMException name:
     * `InvalidStateError` with no activity showing, `AbortError` when the app stopped before the start,
     * `NotFoundError` when no app handles [intent], `NotAllowedError` when the platform refused the start (a
     * `SecurityException`: `ACTION_IMAGE_CAPTURE` from an app that declares CAMERA and does not hold it), and
     * `UnknownError` for any other refusal.
     *
     * The function returned withdraws a start still waiting for the resume, answering neither [failed] nor [done], and
     * says whether it did: a call let go ([ExactExtension.cancel]) withdraws its start so that no sheet opens for it.
     */
    fun startActivityForResult(intent: android.content.Intent, failed: (String) -> Unit, done: (Int, android.content.Intent?) -> Unit) =
        Extensions.start(intent, failed, done)

    /** [startActivityForResult], where a start that did not happen is [done] with `Activity.RESULT_CANCELED`. */
    fun startActivityForResult(intent: android.content.Intent, done: (Int, android.content.Intent?) -> Unit) =
        Extensions.start(intent, { done(Activity.RESULT_CANCELED, null) }, done)

    /** [startActivityForResult] for a [android.content.IntentSender]: a Play services resolution's (Google Pay's sheet). */
    fun startIntentSenderForResult(sender: android.content.IntentSender, done: (Int, android.content.Intent?) -> Unit) =
        Extensions.start(sender, { done(Activity.RESULT_CANCELED, null) }, done)

    /**
     * The app's handler [handler] run for the work [handle] names (LLP 1133.016 D15 item 1): one the call that made
     * the handle listed, run by the runner as a fresh answer whose origin is at most `tool`. [reply] gets
     * `{"text": JSON}` or `{"error", "message"}`, on the main thread.
     */
    fun invoke(handle: String, handler: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        val body = JSONObject().put("extension", name).put("handle", handle).put("handler", handler).put("args", args)
        val id = try { Exact.invoke(body.toString()) } catch (e: UnsatisfiedLinkError) { -1 }
        if (id < 0) return reply(JSONObject().put("error", "InvalidStateError").put("message", "no session runs"))
        // The answer lands on the host's turn; the main thread reads it a frame at a time.
        val main = android.os.Handler(android.os.Looper.getMainLooper())
        lateinit var poll: Runnable
        poll = Runnable {
            val answer = try { Native.invoked(id) } catch (e: UnsatisfiedLinkError) { "{\"error\":\"InvalidStateError\"}" }
            if (answer != null) reply(JSONObject(answer)) else main.postDelayed(poll, 16)
        }
        main.post(poll)
    }

    /**
     * Whether the embedder lets the app leave for [url] (LLP 1133.024 D22 item 7), [how] `browser` (a page over the
     * app) or `external` (another app): asked before a page or a hand-off by URL opens. The stock activity lets every
     * one go; an embedding app sets [ExtensionContext.leaving] to decide. Main thread.
     */
    fun leave(url: String, how: String): Boolean = leaving?.invoke(url, how) ?: true

    companion object {
        /** The embedder's decision on leaving the app ([leave]); null lets every URL go. */
        @JvmStatic var leaving: ((String, String) -> Boolean)? = null

        /** A receiver's presentation while the activity shows. The session hears it after first frame with device origin. Main thread. */
        @JvmStatic fun emitWhileVisible(extension: String, event: String, args: JSONArray, hold: Int) {
            if (Extensions.visibleNow) Activations.emit(extension, event, args, hold)
        }

        /** [ExtensionContext.activation] for extension [extension] with no session: a receiver's, which posts what an
         * extension scheduled while the app was not running. */
        @JvmStatic fun activation(context: Context, extension: String, event: String, args: JSONArray, navigate: String? = null): android.content.Intent =
            Activations.intent(context, extension, event, args, navigate)

        /**
         * The intent a notification's action starts (LLP 1133.004 stage 2): [event] of [extension] with [args], the
         * name of [action] (a declared action's record) and the reply written into `args[chosen]`, raised only while
         * the plan declares [action]; the notification [tag], [id] is closed. Activities only: Android shows a
         * background action in the foreground (LLP 1133.004.000 D5). An action with a text field reads its reply
         * under [Activations.REPLY].
         */
        @JvmStatic fun action(context: Context, extension: String, event: String, args: JSONArray, chosen: Int, action: JSONObject, tag: String, id: Int): android.content.Intent =
            Activations.action(context, extension, event, args, chosen, action, tag, id)

        /** The `RemoteInput` key of an action's text field. */
        const val REPLY = Activations.REPLY

        /**
         * The app's session for a platform callback that may run with no activity (a job, a worker, a receiver;
         * LLP 1133.017 D7, D10): [ready] hears, on the main thread, whether the process's host has booted, its data
         * module is active and its `"start": "load"` extensions are made. Where no activity has set the process up,
         * the host boots here with no view, hidden as a stopped activity's is, and an activity made later adopts
         * it; with no first pixel to wait for, its data module activates at once. A wake raised in it
         * ([wake]) ends `completed` only once the store's writes are on disk, so the caller may give its job back at
         * the wake's outcome. [active] is checked before boot and before extensions load: an expired
         * platform callback may cancel its pending launch.
         */
        @JvmStatic fun session(context: Context, active: () -> Boolean = { true }, ready: (Boolean) -> Unit) =
            Headless.session(context, active, ready)
    }

    /** [requestPermissions] for [name], keeping the history [permission] reads. */
    fun askPermission(cause: Cause, name: String, done: () -> Unit) = requestPermissions(cause, arrayOf(name)) { answered ->
        val a = activity
        if (name in answered && permission(name).state != "granted" && a != null && !a.shouldShowRequestPermissionRationale(name)) {
            application.getSharedPreferences("exact.permissions", Context.MODE_PRIVATE).edit().putBoolean(name, true).apply()
        }
        done()
    }
}

/** The process's gamepads as a module reads them ([ExtensionContext.pads]): each by its slot, the index a world
 * canvas knows it by. Main thread. */
class ExtensionPads internal constructor(private val application: Context) {
    /** The connected pads by index, with their identities. */
    fun list(): List<Pair<Int, PadDevice>> = Pads.attach(application).list()
    /** The pad whose button last went down; null when none has been used since it connected. */
    val current: Int? get() = Pads.current.takeIf { it >= 0 }
    /** Pad [index]'s latest standard reading: 17 buttons, then 4 axes; null for no pad. */
    fun reading(index: Int): FloatArray? = Pads.values(index)
    /** [f] is told each change: a pad came or went, or another is in use. Its [owner] ends it with [unwatch]. */
    fun watch(owner: Any, f: () -> Unit) { Pads.attach(application).watch(owner, f) }
    fun unwatch(owner: Any) = Pads.unwatch(owner)
}

/** The page's canvases, as an extension feeds them ([ExtensionContext.surfaces]). */
class ExtensionSurfaces internal constructor() {
    /**
     * The input of surface [surface]'s canvas: what is written there the host delivers to the live canvas of that
     * surface once a display frame, before the frame's input reaches its module, every sample in order. Null when the
     * page places no canvas of that surface, or the library predates canvas inputs.
     */
    fun input(surface: String): SurfaceInput? = try {
        if (Native.surfaceHas(surface)) SurfaceInput(surface) else null
    } catch (e: UnsatisfiedLinkError) { null }
}

/**
 * A canvas's input that an extension writes ([ExtensionSurfaces.input]), from any thread: each call waits for nothing.
 * The host keeps about a second at the fastest rate a phone samples; past it the oldest samples go, and the next
 * sample of their sensor delivered counts them in its `dropped`.
 */
class SurfaceInput internal constructor(val surface: String) {
    /**
     * One sample of [sensor] (a Generic Sensor name: `gravity`, `gyroscope`, …): x, y, z, and w for an orientation's
     * quaternion, finite, in the frame the input asked for. [dropped] counts the samples lost before it (a pause); [at]
     * is the session clock's time (ms, [ExtensionContext.now]) when it was taken, which also stamps the canvas's keys
     * and touches.
     */
    fun sample(sensor: String, values: DoubleArray, dropped: Int = 0, at: Double) = write(sensor, values, dropped, at, false)

    /** The input of [sensor] ended (`stopSensorInput`, the canvas gone): its last sample no longer stands. */
    fun end(sensor: String, at: Double) = write(sensor, DoubleArray(0), 0, at, true)

    private fun write(sensor: String, values: DoubleArray, dropped: Int, at: Double, ended: Boolean) {
        if (Native.sensorInput(surface, sensor, values, dropped, at, ended)) Exact.sensorFrame()
    }
}

/** A runtime permission's state and the platform's words for it ([ExtensionContext.permission]). */
class PermissionState(val state: String, val shouldShowRationale: Boolean, val permanentlyDenied: Boolean)

/** The process's extensions: made at their first call, they outlive the activities that show the app. */
internal object Extensions {
    private const val RESULTS = 0x5000
    private val written: ExtensionRoster? by lazy {
        val type = try { Class.forName("exact.app.ExactExtensions") } catch (e: ClassNotFoundException) { return@lazy null }
        type.getDeclaredConstructor().newInstance() as ExtensionRoster
    }
    private val roster: Map<String, (ExtensionContext) -> ExactExtension> get() = written?.extensions ?: emptyMap()
    fun config(name: String): JSONObject = written?.config?.get(name)?.let { JSONObject(it) } ?: JSONObject()
    private val made = HashMap<String, ExactExtension>()
    /** The one session's track table. */
    val tracks = Tracks()
    /** The events raised so far ([ExtensionContext.emit]). */
    private var emitted = 0
    fun emit(extension: String, event: String, args: JSONArray) {
        Exact.changed("exact-event:" + JSONObject().put("extension", extension).put("event", event).put("args", args).put("seq", ++emitted))
    }
    private var application: Context? = null
    /** The activity is resumed ([resumed], [paused]). */
    private var foreground = false
    /** The activity shows: started and not stopped ([started], [stopped]). */
    private var visible = false
    /** The activity that shows the app; a test sets its own. */
    internal var front: () -> Activity? = { Exact.reader?.get() }
    private val permissions = PermissionRequests { names, code ->
        val activity = front()
        if (activity == null) false else { activity.requestPermissions(names, code); true }
    }

    /** The display's angle last told the extensions ([ExtensionContext.orientation]). */
    private var told = -1

    /** [activity]'s display's rotation in degrees, as the web's `screen.orientation.angle`. */
    fun angle(activity: Activity?): Int = when (activity?.display?.rotation) {
        android.view.Surface.ROTATION_90 -> 90
        android.view.Surface.ROTATION_180 -> 180
        android.view.Surface.ROTATION_270 -> 270
        else -> 0
    }

    /** A display changed: the extensions made are told a new angle. */
    private val displays = object : android.hardware.display.DisplayManager.DisplayListener {
        override fun onDisplayAdded(id: Int) {}
        override fun onDisplayRemoved(id: Int) {}
        override fun onDisplayChanged(id: Int) {
            val angle = angle(Exact.reader?.get())
            if (angle == told) return
            told = angle
            made.values.forEach { it.orientation(angle) }
        }
    }

    /** The process's host is set up, by the activity [context] or with none: the host binds the names of the
     * extensions the APK carries. */
    fun bind(context: Context) {
        val app = context.applicationContext
        if (application == null) {
            val dm = app.getSystemService(android.hardware.display.DisplayManager::class.java)
            dm?.registerDisplayListener(displays, android.os.Handler(android.os.Looper.getMainLooper()))
            told = angle(context as? Activity)
            application = app
        }
        Presence.application = app
        Activities.application = app
        AudioFocus.application = app
        if (roster.isNotEmpty()) Exact.bindExtensions(roster.keys.joinToString("\n"))
    }

    /**
     * Request [id]'s call, the runner's body: `{"extension", "source"|"send", "args", "preferred"?, "userActivation"?}`, `{"extension",
     * "release", "args"?}`, `{"extension", "preferred"}`, `{"extension", "dispose"}`, `{"extension", "foreground"}`, or
     * `{"extension", "view", "displayed"|"bound", "origin"}` (a package view's report, LLP 1133.008 D8 item 2),
     * `{"extension", "surface", "displayed", "origin"}` (a canvas its input feeds, LLP 1133.013 D6),
     * `{"extension", "picture", "width", "height", "fit", "owned"}` (LLP 1133.019 D11 item 1), answered with its
     * pixels ([Exact.picture]), `{"extension", "cancel"}` (a send let go, LLP 1133.016 D15 item 2), or `{"extension",
     * "woke", "outcome"}` (a wake [ExtensionContext.wake] raised ended, LLP 1133.017 D8).
     */
    fun clock() { for (extension in made.values.toList()) extension.clock() }

    fun call(id: Int, body: String, module: ModuleSession? = ModuleSession.current) {
        val answered = java.util.concurrent.atomic.AtomicBoolean()
        fun refuse(error: String = "NotSupportedError", message: String? = null) {
            if (answered.compareAndSet(false, true)) Exact.refuseModule(id, error, message)
        }
        fun failed(e: Exception) {
            Log.e("exact", "extension call $id: $e")
            refuse("Error", e.message)
        }
        val reply = { answer: JSONObject ->
            if (answered.compareAndSet(false, true)) {
                try { Exact.answer(id, moduleReplyJson(answer)) }
                catch (_: org.json.JSONException) { Exact.refuseModule(id, "Error", "the reply was not JSON") }
            }
        }
        val request = try { JSONObject(body) } catch (e: org.json.JSONException) { return refuse(message = e.message) }
        try {
            if (!request.has("extension")) {
                if (module == null) return refuse()
                return module.later(id, request)
            }
            val name = request.opt("extension") as? String ?: return refuse()
            clock()
            val extension = extension(name) ?: return refuse()
            val source = request.opt("source").takeUnless { it === JSONObject.NULL }
            val send = request.opt("send").takeUnless { it === JSONObject.NULL }
            if ((source != null && source !is String) || (send != null && send !is String)) return refuse()
            val declared = (source as? String)?.ifEmpty { null } ?: send as? String
            val released = request.opt("release") as? String
            when {
                request.has("view") -> {
                    val view = request.opt("view") as? String ?: return refuse()
                    val cause = Cause.of(request)
                    if (request.has("displayed")) extension.displayed(extension.context.session, view, request.opt("displayed") as? Boolean ?: return refuse(), cause)
                    if (request.has("bound")) extension.bound(extension.context.session, view, request.opt("bound") as? String ?: return refuse(), request.opt("value"), cause)
                    reply(JSONObject())
                }
                request.has("surface") && request.has("displayed") -> {
                    extension.displayed(request.opt("surface") as? String ?: return refuse(), request.opt("displayed") as? Boolean ?: return refuse(), Cause.of(request))
                    reply(JSONObject())
                }
                request.has("picture") -> {
                    val source = request.opt("picture") as? String ?: return refuse()
                    fun pixels(key: String): Int? {
                        val n = request.optDouble(key)
                        return n.toInt().takeIf { it.toDouble() == n }
                    }
                    val width = pixels("width") ?: return refuse()
                    val height = pixels("height") ?: return refuse()
                    extension.picture(source, width, height, request.optString("fit", "fill"), request.optBoolean("owned")) { bitmap, error ->
                        if (answered.compareAndSet(false, true)) {
                            val pixels = bitmap?.let(::software)
                            Exact.picture(id, pixels, if (pixels == null) error ?: "UnknownError" else null)
                        }
                    }
                }
                !released.isNullOrEmpty() -> {
                    val args = request.optJSONObject("args")
                    if (args != null) extension.release(released, args) else extension.release(released)
                    reply(JSONObject())
                }
                !declared.isNullOrEmpty() -> {
                    val cause = Cause.of(request)
                    fun call() {
                        try { extension.later(declared, request.optJSONObject("args") ?: JSONObject(), request.opt("preferred"), cause, reply) }
                        catch (e: Exception) { failed(e) }
                    }
                    // A call that would ask and is no person's in-app act (LLP 1133.002 D12): refused while the
                    // implementation's own status could lead to a dialog, as Apple's host refuses it. The status is
                    // read under the call's cause.
                    val admit = request.optJSONObject("admit")
                    if (admit == null) call()
                    else {
                        val statusName = admit.opt("status") as? String ?: return refuse()
                        val field = admit.opt("field") as? String ?: return refuse()
                        extension.later(statusName, JSONObject(), null, cause) { status ->
                            // A missing status, prompt, or Android's retryable denial may lead to a dialog.
                            val state = status.opt(field) as? String
                            if (!state.isNullOrEmpty() && state != "prompt" && state != "denied") call()
                            else reply(JSONObject().put("error", admit.optString("denied").ifEmpty { "NotAllowedError" }).put("message", "person-required"))
                        }
                    }
                }
                request.has("preferred") -> {
                    val preferred = request.optJSONObject("preferred") ?: return refuse()
                    request.optJSONObject("permissions")?.let { extension.permissions(it) }
                    extension.preferred(preferred)
                    reply(JSONObject())
                }
                request.has("cancel") -> {
                    val number = request.opt("cancel")
                    if (number !is Int && (number !is Long || number !in 0..0xffff_ffffL)) return refuse()
                    extension.cancel((number as Number).toInt())
                    reply(JSONObject())
                }
                request.has("dispose") -> {
                    val ids = request.optJSONArray("dispose") ?: return refuse()
                    extension.dispose((0 until ids.length()).map { ids.opt(it) as? String ?: return refuse() })
                    reply(JSONObject())
                }
                request.has("woke") -> {
                    val woke = request.opt("woke") as? String ?: return refuse()
                    // With no activity showing, Android may kill the process once the wake's job is given back: what
                    // the handler committed is on disk first, or the wake did not complete.
                    val outcome = request.optString("outcome", "completed")
                        .let { if (it == "completed" && !visible && !Exact.drainStore()) "expired" else it }
                    extension.context.ended.remove(woke)?.invoke(outcome)
                    extension.woke(woke, outcome)
                    reply(JSONObject())
                }
                request.has("foreground") -> {
                    val shown = request.opt("foreground") as? Boolean ?: return refuse()
                    extension.context.foreground = shown
                    extension.foreground(shown)
                    reply(JSONObject())
                }
                else -> refuse()
            }
        } catch (e: Exception) {
            failed(e)
        }
    }

    /** [bitmap] as the host copies it: software `ARGB_8888` in sRGB, drawn into one when it is not. */
    private fun software(bitmap: android.graphics.Bitmap): android.graphics.Bitmap? {
        val argb = android.graphics.Bitmap.Config.ARGB_8888
        val srgb = android.graphics.ColorSpace.get(android.graphics.ColorSpace.Named.SRGB)
        // A hardware bitmap's pixels are the GPU's: copied out first.
        val soft = if (bitmap.config == android.graphics.Bitmap.Config.HARDWARE) bitmap.copy(argb, false) ?: return null else bitmap
        if (soft.config == argb && soft.colorSpace?.let { it == srgb } != false) return soft
        val out = android.graphics.Bitmap.createBitmap(soft.width, soft.height, argb, true, srgb)
        android.graphics.Canvas(out).drawBitmap(soft, 0f, 0f, null)
        return out
    }

    /** Whether the activity is resumed ([resumed], [paused]). */
    val resumedNow: Boolean get() = foreground

    /** Whether the activity shows, resumed or paused under a dialog ([started], [stopped]): what Android's own rule for
     * a start from the foreground reads, an app with a visible activity. */
    val visibleNow: Boolean get() = visible

    /** The extension that serves the authenticator entry (LLP 1133.010 D13), made if need be. */
    fun authenticator(): ExactAuthenticator? = roster.keys.firstNotNullOfOrNull { extension(it) as? ExactAuthenticator }

    /** Extension [name], made at its first call or view. */
    /** The reader drew its first frame: each extension declared `"start": "load"` is made now (LLP 1133.015 D16 item
     * 1), once for the process, as an extension outlives the activities that show the app. */
    fun firstFrame() {
        written?.startAtLoad?.forEach { extension(it) }
    }

    private fun extension(name: String): ExactExtension? = made[name]
        ?: roster[name]?.let { make -> application?.let { make(ExtensionContext(it, name)) } }?.also { made[name] = it; webauthn.forEach { f -> it.webauthnField(f, true) } }

    /** The live fields whose `autocomplete` ends in `webauthn` ([Fields]). */
    private val webauthn = LinkedHashSet<android.widget.EditText>()

    fun webauthnField(view: android.widget.EditText, present: Boolean) {
        if (if (present) !webauthn.add(view) else !webauthn.remove(view)) return
        made.values.forEach { it.webauthnField(view, present) }
    }

    /** The app's module views with each package's beside them, each made by its extension (LLP 1133.001 D1). */
    fun withViews(modules: Map<String, NativeModule>): Map<String, NativeModule> {
        val views = written?.views ?: emptyMap()
        return if (views.isEmpty()) modules else modules + views.mapValues { (tag, name) -> PackageView(tag, name) }
    }

    /** A package's view, as [Natives] makes a module's: the process's one extension makes it for its session. */
    private class PackageView(val tag: String, val name: String) : NativeModule {
        private val emits = HashMap<android.view.View, (String, String) -> Unit>()
        override fun create(context: Context, props: JSONObject) = create(context, props, 0)
        override fun create(context: Context, props: JSONObject, id: Int): android.view.View {
            val extension = extension(name) ?: throw IllegalStateException("no extension $name for $tag")
            lateinit var made: android.view.View
            made = extension.view(tag, extension.context.session, id.toString(), context, props) { kind, text -> emits[made]?.invoke(kind, text) }
            return made
        }
        override fun update(view: android.view.View, props: JSONObject) { extension(name)?.update(view, props) }
        override fun events(view: android.view.View, emit: (String, String) -> Unit) { emits[view] = emit }
        override fun lifecycle(view: android.view.View, event: ViewLifecycle, state: android.os.Bundle?) {
            extension(name)?.viewLifecycle(view, event, state)
        }
        override fun destroy(view: android.view.View) { emits.remove(view); extension(name)?.destroyView(view) }
    }

    fun ask(names: Array<String>, done: (Set<String>) -> Unit) = permissions.ask(names, done)

    /** The calls waiting on an activity's result ([ExtensionContext.startActivityForResult]), by request code. */
    private val results = HashMap<Int, (Int, android.content.Intent?) -> Unit>()
    private var started = 0

    /** A start for a result: [go] starts it from the activity with its request code. */
    private class Start(val go: (Activity, Int) -> Unit, val failed: (String) -> Unit, val done: (Int, android.content.Intent?) -> Unit)

    /** The starts asked while the activity showed paused, made at the next resume ([ExtensionContext.startActivityForResult]). */
    private val waiting = ArrayList<Start>()

    fun start(intent: android.content.Intent, failed: (String) -> Unit, done: (Int, android.content.Intent?) -> Unit) =
        start(Start({ activity, code -> activity.startActivityForResult(intent, code) }, failed, done))

    fun start(sender: android.content.IntentSender, failed: (String) -> Unit, done: (Int, android.content.Intent?) -> Unit) =
        start(Start({ activity, code -> activity.startIntentSenderForResult(sender, code, null, 0, 0, 0) }, failed, done))

    /** Made or refused now, or waiting for the resume; the function returned withdraws a waiting one. */
    private fun start(start: Start): () -> Boolean {
        if (front() == null || !visible) { start.failed("InvalidStateError"); return { false } }
        if (!foreground) { waiting += start; return { waiting.remove(start) } }
        launch(start)
        return { false }
    }

    private fun launch(start: Start) {
        val activity = front() ?: return start.failed("InvalidStateError")
        // Request codes of their own range, apart from permission requests (0x4000–0x4fff).
        val code = RESULTS + (started++ % 0x1000)
        results[code] = start.done
        val refusal = try {
            start.go(activity, code)
            return
        } catch (e: android.content.ActivityNotFoundException) {
            "NotFoundError"
        } catch (e: SecurityException) {
            "NotAllowedError"
        } catch (e: android.content.IntentSender.SendIntentException) {
            // The sender's PendingIntent was cancelled.
            "InvalidStateError"
        } catch (e: RuntimeException) {
            Log.w("exact", "an extension's activity did not start", e)
            "UnknownError"
        }
        results.remove(code)
        start.failed(refusal)
    }

    /** [Activity.onActivityResult]: the answer of the call that started it, if this process made one. */
    fun activityResult(code: Int, result: Int, data: android.content.Intent?) { results.remove(code)?.invoke(result, data) }

    /** A call of [ExtensionContext.whenResumed] that waits. */
    private class Resume(val failed: (String) -> Unit, val go: (Activity) -> Unit)

    /** The calls waiting for the next resume ([ExtensionContext.whenResumed]). */
    private val resumes = ArrayList<Resume>()

    fun whenResumed(failed: (String) -> Unit, go: (Activity) -> Unit): () -> Unit {
        val activity = front()
        if (activity == null || !visible) { failed("InvalidStateError"); return {} }
        if (foreground) { go(activity); return {} }
        val wait = Resume(failed, go)
        resumes += wait
        return { resumes.remove(wait) }
    }

    /** [Activity.onRequestPermissionsResult]: the answer to the request up. */
    fun permissionsResult(code: Int, names: Array<String>, grants: IntArray) = permissions.result(code, names, grants)

    /** [Activity.onStart]. */
    fun started() { visible = true }

    /** [Activity.onResume]: the starts and calls that waited for it are made, then a permission result that never came
     * (the activity was made again) is read as the state now. */
    fun resumed() {
        foreground = true
        val starts = waiting.toList()
        waiting.clear()
        starts.forEach(::launch)
        val calls = resumes.toList()
        resumes.clear()
        calls.forEach { call -> front()?.let(call.go) ?: call.failed("InvalidStateError") }
        permissions.resumed()
        made.values.forEach { it.resumed() }
    }

    /** [Activity.onPause]. */
    fun paused() {
        foreground = false
        permissions.paused()
        made.values.forEach { it.paused() }
    }

    /** [Activity.onStop] of the activity that shows the app: the starts and calls waiting for a resume end, unless it
     * stops to be made again for a configuration change ([changing]), whose successor makes them. */
    fun stopped(changing: Boolean) {
        visible = false
        if (changing) return
        val starts = waiting.toList()
        waiting.clear()
        starts.forEach { it.failed("AbortError") }
        val calls = resumes.toList()
        resumes.clear()
        calls.forEach { it.failed("AbortError") }
    }
}
