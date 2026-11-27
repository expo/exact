package dev.exact.canvas

import android.app.Activity
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import org.json.JSONException
import org.json.JSONStringer
import org.json.JSONObject
import java.lang.ref.WeakReference
import java.util.concurrent.atomic.AtomicBoolean

/** An app or game's data module: define class Module : ExactModule() in package exact.app. */
abstract class ExactModule {
    val context: ExactModuleContext = ModuleSession.constructionContext()

    /** A cheap read on the calling thread, inside the data source's budget. */
    open fun call(request: JSONObject): JSONObject = throw NoModuleCall()

    /** Start work on the main thread. The first reply, from any thread, answers the promise. */
    open fun later(request: JSONObject, reply: ExactReply) { reply.send(call(request)) }

    /** The root's data words at connection and on change, on the session thread. */
    open fun app(app: ExactApp) {}
    open fun destroy() {}
}

private class NoModuleCall : UnsupportedOperationException("the app's module answers no native.call")

/** The session's application Context, the activity in front if one is, clock and topic notifications. */
class ExactModuleContext internal constructor(val application: Context, private val session: ModuleSession) {
    /**
     * The activity that shows the app now, read on each access, as [ExtensionContext.activity] is: its window takes
     * `FLAG_SECURE`, `screenBrightness` and a screen-capture callback (expo/exact#1596). Null before the first activity
     * is made (a process a job or widget started), after the last one is destroyed, and between a destroyed activity
     * and its replacement. A configuration change that the manifest's `configChanges` lists keeps the activity and its
     * window; one that remakes it gives a new window without what was set on the old one. While the app is stopped this
     * is the stopped activity: its flags still hold, so `FLAG_SECURE` keeps the recents thumbnail blank. Use it on main
     * ([ExactModule.later]); [ExactModule.call] runs on the caller's thread. Read it when needed; don't keep it.
     */
    val activity: Activity? get() = Extensions.front()
    val agent: Boolean get() = agentCarried()
    fun now(): Double = Native.now()
    fun changed(topic: String) = session.hand(OwnerReplies.Handed.Changed(topic))
    fun after(ms: Double, body: () -> Unit) = session.after(now() + (ms.takeIf { it.isFinite() } ?: 0.0).coerceAtLeast(0.0), body)
}

class ExactReply internal constructor(private val id: Int, session: ModuleSession) {
    private val session = WeakReference(session)
    private val answered = AtomicBoolean()
    fun send(value: JSONObject) {
        val json = try { moduleReplyJson(value) } catch (_: JSONException) { return fail("the reply was not JSON") }
        if (answered.compareAndSet(false, true)) session.get()?.hand(OwnerReplies.Handed.Answer(id, json))
    }
    fun fail(message: String) = refuse("Error", message)
    internal fun refuse(name: String, message: String?) {
        if (answered.compareAndSet(false, true)) session.get()?.hand(OwnerReplies.Handed.Refused(id, name, message))
    }

    // A retained reply may answer on any thread. ART can identify an abandoned one only at collection.
    protected fun finalize() { fail("the module dropped the reply") }
}

// Module callbacks take an object. The single-key envelope carries a scalar or list back to data code.
internal fun moduleReplyJson(value: JSONObject): String =
    moduleJson(if (value.length() == 1 && value.has("exact:answer")) value.get("exact:answer") else value)

// JSONStringer also quotes scalar replies and reports invalid values instead of returning null.
private fun moduleJson(value: Any): String {
    val array = JSONStringer().array().value(value).endArray().toString()
    return array.substring(1, array.lastIndex)
}

/** Owned by the Canvas host. Only the reader's asynchronous requests need the current session. */
internal class ModuleSession private constructor(private val owner: Owner?) {
    private lateinit var module: ExactModule
    private val main = Handler(Looper.getMainLooper())
    private val callbacks = HashMap<Int, () -> Unit>()
    private var nextTimer = 0
    private val ended = AtomicBoolean()
    private var opening: MutableList<OwnerReplies.Handed>? = ArrayList()

    fun hand(value: OwnerReplies.Handed) {
        synchronized(this) {
            if (ended.get()) return
            opening?.let { it.add(value); return }
            if (owner != null) owner.hand(value)
            else main.post { if (!ended.get()) { OwnerReplies.deliver(value); Exact.turned() } }
        }
    }

    fun after(due: Double, body: () -> Unit) {
        val id = synchronized(callbacks) {
            if (ended.get()) return
            (++nextTimer).also { callbacks[it] = body }
        }
        hand(OwnerReplies.Handed.After(id, due))
    }

    fun timer(id: Int) { synchronized(callbacks) { callbacks.remove(id) }?.invoke() }
    fun app(json: String, built: Boolean) {
        val words = JSONObject(json)
        module.app(ExactApp(words.keys().asSequence().associateWith(words::getString), built))
    }

    fun call(body: String): String = try {
        val request = JSONObject(body)
        if (request.opt("extension") is String && request.opt("sync") is String) Extensions.sync(request)
        else "=" + moduleJson(module.call(request))
    } catch (e: NoModuleCall) { "!NotSupportedError: ${e.message}" }
    catch (e: Throwable) { "!" + (e.message ?: e.javaClass.simpleName) }

    fun later(id: Int, request: JSONObject) {
        if (ended.get()) return
        val reply = ExactReply(id, this)
        try { module.later(request, reply) }
        catch (_: NoModuleCall) { reply.refuse("NotSupportedError", "the app's module answers no native.later") }
        catch (e: Throwable) { reply.fail(e.message ?: e.javaClass.simpleName) }
    }

    fun end() {
        if (!ended.compareAndSet(false, true)) return
        synchronized(Companion) {
            if (owner != null) { if (owner.module === this) owner.module = null }
            else if (readerModule === this) readerModule = null
        }
        synchronized(callbacks) { callbacks.clear() }
        // Finish on the same looper as later, after any call already running there.
        main.post {
            try { module.destroy() } catch (e: Throwable) { Log.e("exact", "destroying exact.app.Module", e) }
        }
    }

    companion object {
        @Volatile private var readerModule: ModuleSession? = null
        val current: ModuleSession? get() = Exact.owner.let { if (it == null) readerModule else it.module }
        private val constructing = ThreadLocal<ExactModuleContext>()
        fun constructionContext(): ExactModuleContext = checkNotNull(constructing.get()) { "the session constructs exact.app.Module" }

        /** A package's synchronous form for a session with no app module ([Extensions.sync]). */
        @JvmStatic fun sync(body: String): String = try { Extensions.sync(JSONObject(body)) }
        catch (e: Exception) { "!" + (e.message ?: e.javaClass.simpleName) }

        @JvmStatic fun open(context: Context): ModuleSession? = try {
            val type = Class.forName("exact.app.Module", false, context.classLoader).asSubclass(ExactModule::class.java)
            val owner = Exact.owner
            val session = ModuleSession(owner)
            constructing.set(ExactModuleContext(context.applicationContext, session))
            try {
                session.module = type.getDeclaredConstructor().newInstance()
            } finally {
                constructing.remove()
            }
            // Each Canvas owner has one host. A reader without an owner runs that host on main.
            synchronized(this) {
                if (owner != null) {
                    owner.module?.end()
                    owner.module = session
                } else {
                    readerModule?.end()
                    readerModule = session
                }
            }
            synchronized(session) {
                val queued = session.opening.orEmpty()
                session.opening = null
                queued.forEach(session::hand)
            }
            session
        } catch (_: ClassNotFoundException) { null }
        catch (e: Exception) { Log.e("exact", "cannot make exact.app.Module", e); null }
        catch (e: LinkageError) { Log.e("exact", "cannot load exact.app.Module", e); null }
    }
}
