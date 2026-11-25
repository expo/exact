package dev.exact.canvas

import android.net.LocalServerSocket
import android.net.LocalSocket
import android.os.Process
import android.os.SystemClock
import android.graphics.Rect
import android.view.MotionEvent
import android.view.View
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.FutureTask
import java.io.IOException
import java.util.concurrent.CountDownLatch

internal fun agentCarried(): Boolean {
    val mode = android.system.Os.getenv("EXACT_AGENT")
    return mode == "1" || mode == "live"
}

/**
 * The agent API's carrier in the reader (LLP 1012; LLP 1107 D1's `--reader`): `scripts/agent.mjs android --reader`
 * drives the app this activity shows by the Linux host's own protocol, one JSON line a request and one its answer.
 * The lines cross a socket in the abstract namespace, [name], which adb forwards (`adb forward tcp:<port>
 * localabstract:<name>`); each request is answered on exact2's thread ([Exact.agent], [Native.agent]).
 *
 * An activity attaches it at the reader's driver seam ([ExactActivity.drive]) when its launch asks for one (the
 * shell's `agent=<name>` extra). Only adb's own user (shell, or root) is served: any app on the phone can reach an
 * abstract socket, and a driver holds the whole app.
 *
 * The first line out is the handler's `ready`, written after the first frame following successful startup, even
 * with an empty page or only bars. A `screenshot` is not answered here: the picture is the device's, and the carrier
 * takes it (`screencap`).
 */
class AgentDrive(name: String) : Drive {
    private val server = LocalServerSocket(name)
    private val presented = CountDownLatch(1)
    private val entered = CountDownLatch(1)

    init {
        Thread({ serve() }, "exact-agent").apply { isDaemon = true }.start()
    }

    override fun contentReady() = presented.countDown()
    override fun windowEntered() = entered.countDown()

    private fun serve() {
        while (true) {
            val socket = try { server.accept() } catch (e: IOException) { return }
            try { socket.use { talk(it) } } catch (e: IOException) { /* the driver left */ }
        }
    }

    private fun talk(socket: LocalSocket) {
        val uid = socket.peerCredentials.uid
        if (uid != Process.SHELL_UID && uid != Process.ROOT_UID) return
        val lines = socket.inputStream.bufferedReader(Charsets.UTF_8)
        val out = socket.outputStream
        fun say(line: String) { out.write((line + "\n").toByteArray(Charsets.UTF_8)); out.flush() }
        presented.await()
        say(Exact.agent("{\"op\":\"ready\"}"))
        var finger: AgentModuleFinger? = null
        try {
            while (true) {
                val line = lines.readLine() ?: return
                if (line.isBlank()) continue
                val request = runCatching { JSONObject(line) }.getOrNull()
                val phase = request?.optString("phase").orEmpty()
                val held = finger
                if (request?.optString("op") == "moduleMotion") {
                    val reply = main { held?.confirm(request.optString("error")) ?: problem("no pending module touch") }
                    if (reply.has("error") || held?.finished == true) { main { held?.close() }; finger = null }
                    say(tagged(reply))
                } else if (request?.optString("op") == "tap" && held != null) {
                    if (phase !in setOf("move", "hold", "up", "cancel")) { say(tagged(problem("a contact is already down"))); continue }
                    val reply = main { held.phase(phase, request) }
                    if (reply.has("error")) { main { held.close() }; finger = null }
                    say(tagged(reply))
                } else if (AX.containsMatchIn(line)) say(ax(line))
                else if (request?.has("scrollDrag") == true && request.has("probe")) {
                    say(Exact.agent(request.put("longPressMs", android.view.ViewConfiguration.getLongPressTimeout()).toString()))
                } else {
                    val answer = Exact.agent(line)
                    val aim = if (request?.optString("op") == "tap") JSONObject(answer) else null
                    val binding = aim?.optJSONObject("moduleContact")
                    if (binding == null || request == null) {
                        say(answer); continue
                    }
                    // The window manager transforms input coordinates during the activity's entrance.
                    if (!entered.await(5, java.util.concurrent.TimeUnit.SECONDS)) {
                        say(tagged(problem("the activity has not finished entering its window"))); continue
                    }
                    val reply = main {
                        val activity = Exact.reader?.get() ?: return@main problem("module view has no activity")
                        val allocation = ModuleBinding(binding.getInt("id"), binding.getInt("slot"),
                            binding.getInt("generation"), binding.getLong("session"))
                        val target = activity.view.natives?.taker(allocation.id, allocation.slot, allocation.generation, allocation.session)
                            ?: return@main problem("module view is detached, disabled or replaced")
                        val contact = AgentModuleFinger(activity.view, allocation, target, aim.getJSONArray("at"),
                            request.getInt("id"), plain = !request.has("phase"))
                        val result = contact.start()
                        if (!result.has("error")) finger = contact
                        result
                    }
                    say(tagged(reply))
                }
            }
        } finally { finger?.let { main { it.close() } } }
    }

    /**
     * `tree --ax` (LLP 1080.002; LLP 1115.001 D13): the accessibility tree Android has for the app, which exact2's
     * own handler cannot answer (it has no window). The reader answers a driver as it answers a service: exact2 is
     * asked for the tree, and each node's own `AccessibilityNodeInfo` is read back on the main thread
     * ([ExactActivity.ax]), stamped with the runner's epoch, incarnation and clock as every reply is.
     */
    private fun ax(line: String): String {
        val activity = Exact.reader?.get() ?: return Exact.agent(line)
        val limit = try { org.json.JSONObject(line).optInt("limit", 500) } catch (e: org.json.JSONException) { 500 }
        main { activity.axAsk() }
        // A turn of exact2's thread: it publishes the tree, which the main thread takes before the read below.
        val tags = org.json.JSONObject(Exact.agent("{\"op\":\"tags\"}"))
        val ax = main { activity.ax(limit) }
        return org.json.JSONObject().put("epoch", tags.opt("epoch")).put("incarnation", tags.opt("incarnation"))
            .put("clock", tags.opt("clock")).put("ax", ax).toString()
    }

    private fun tagged(reply: JSONObject): String {
        if (reply.has("moduleMotion")) return reply.toString()
        val tags = JSONObject(Exact.agent("{\"op\":\"tags\"}"))
        for (key in listOf("generation", "epoch", "incarnation", "clock")) reply.put(key, tags.opt(key))
        return reply.toString()
    }

    private fun <T> main(f: () -> T): T {
        val task = FutureTask(f)
        android.os.Handler(android.os.Looper.getMainLooper()).post(task)
        return task.get()
    }

    private companion object {
        /** A request for the platform's accessibility tree: `{"op":"tree","ax":true,…}`. */
        val AX = Regex("""^\s*\{(?=.*"op"\s*:\s*"tree")(?=.*"ax"\s*:\s*true)""")
    }
}

private fun problem(why: String) = JSONObject().put("error", why)

/** The carrier sends each edge through Android's input dispatcher. The reader acknowledges the addressed allocation. */
internal class AgentModuleFinger(private val reader: ExactView, private val allocation: ModuleBinding, val target: View,
    at: JSONArray, private val id: Int, private val plain: Boolean) {
    private var x = at.getDouble(0).toFloat()
    private var y = at.getDouble(1).toFloat()
    private val scale = Device.scale(reader.context)
    private val display = reader.display?.displayId
    private var sequence = reader.contacts.sequence
    private var downTime = 0L
    private var expected = -1
    private var observed = false
    private var admitted = false
    private var delivered = -1
    private var refusal: String? = null
    var scrolled = false
    var finished = false
        private set

    fun start(): JSONObject {
        if (display == null || !available()) return problem("module view is detached, disabled or covered")
        if (reader.touchDown || (reader.context as? ExactActivity)?.windowTouch == true) return problem("a finger is already down")
        val visible = Rect()
        if (!reader.getLocalVisibleRect(visible) || !visible.contains((x * scale).toInt(), (y * scale).toInt()))
            return problem("module view is outside the visible window")
        return motion(MotionEvent.ACTION_DOWN)
    }

    fun matches(event: MotionEvent): Boolean {
        if (event.actionMasked != expected || kotlin.math.abs(event.x - x * scale) > 1f ||
            kotlin.math.abs(event.y - y * scale) > 1f || sequence != reader.contacts.sequence) return false
        observed = true
        if (expected == MotionEvent.ACTION_DOWN) downTime = event.downTime
        return true
    }

    fun offered(to: View, event: MotionEvent) {
        if (event.downTime == downTime) received(to, MotionEvent.ACTION_DOWN)
    }

    fun received(to: View?, action: Int) {
        if (to !== target) return
        if (action == MotionEvent.ACTION_DOWN) admitted = true
        delivered = action
    }

    fun phase(phase: String, request: JSONObject): JSONObject {
        if (expected >= 0) return problem("previous module touch has no receipt")
        if (sequence != reader.contacts.sequence) return problem("another contact replaced the module touch")
        if (!available()) return problem("module view was detached or covered during the contact")
        return when (phase) {
            "hold" -> {
                val ms = request.optDouble("ms", 0.0)
                if (ms !in 0.0..10000.0) problem("contact duration must be in 0...10000ms")
                else reply(phase).put("waitMs", ms)
            }
            "move" -> {
                val relative = request.has("dx") || request.has("dy")
                if (relative == (request.has("x") || request.has("y"))) return problem("contact move needs exactly x/y or dx/dy")
                val tx = if (relative) x + request.optDouble("dx").toFloat() else request.optDouble("x").toFloat()
                val ty = if (relative) y + request.optDouble("dy").toFloat() else request.optDouble("y").toFloat()
                if (!tx.isFinite() || !ty.isFinite()) return problem("contact move needs two finite coordinates")
                x = tx; y = ty
                motion(MotionEvent.ACTION_MOVE)
            }
            "up" -> motion(MotionEvent.ACTION_UP)
            else -> motion(MotionEvent.ACTION_CANCEL)
        }
    }

    fun confirm(error: String): JSONObject {
        val action = expected
        if (action < 0) return problem("no pending module touch")
        expected = -1
        if (error.isNotEmpty()) return problem(error)
        if (action == MotionEvent.ACTION_DOWN) {
            if (observed) sequence = reader.contacts.sequence
            val pending = observed && reader.contacts.awaiting(target)
            if (!admitted && !pending) {
                refusal = when {
                    !observed -> "Android did not deliver the down to the reader; the module is covered or not hittable"
                    else -> "module view did not accept the down"
                }
                return motion(MotionEvent.ACTION_CANCEL)
            }
            return if (plain) motion(MotionEvent.ACTION_UP) else reply("down")
        }
        if (action == MotionEvent.ACTION_CANCEL || action == MotionEvent.ACTION_UP) finished = true
        refusal?.let { return problem(it) }
        if (!observed) return problem("Android did not deliver the contact to the reader")
        if (plain && scrolled) return problem("the scroller claimed the module touch")
        if (action != MotionEvent.ACTION_CANCEL && !scrolled && (!admitted || delivered != action))
            return problem("module view did not receive " + MotionEvent.actionToString(action))
        return if (plain) reply("up").apply { remove("phase"); remove("contact"); put("tapped", id) }
            else reply(when (action) { MotionEvent.ACTION_MOVE -> "move"; MotionEvent.ACTION_UP -> "up"; else -> "cancel" })
    }

    fun close() {
        reader.contacts.pendingDrive = null
        if (!finished && downTime != 0L && sequence == reader.contacts.sequence && reader.touchDown) {
            val e = MotionEvent.obtain(downTime, SystemClock.uptimeMillis(), MotionEvent.ACTION_CANCEL, x * scale, y * scale, 0)
            try { reader.dispatchTouchEvent(e) } finally { e.recycle() }
        }
        finished = true
    }

    private fun available() = reader.isAttachedToWindow && reader.isShown && target.isAttachedToWindow && target.isShown &&
        reader.natives?.taker(allocation.id, allocation.slot, allocation.generation, allocation.session) === target

    private fun reply(phase: String) = JSONObject().put("phase", phase).put("contact", !finished)
        .put("at", JSONArray(listOf(x, y))).put("delivery", "platform")
        .put("native", when {
            scrolled -> "Android scroll"
            !admitted && reader.contacts.awaiting(target) -> "pending gesture arbitration"
            else -> "View.dispatchTouchEvent"
        })

    private fun motion(action: Int): JSONObject {
        val location = IntArray(2).also(reader::getLocationOnScreen)
        expected = action; observed = false; delivered = -1; scrolled = false
        reader.contacts.pendingDrive = this
        return JSONObject().put("moduleMotion", MotionEvent.actionToString(action).removePrefix("ACTION_"))
            .put("display", display).put("x", location[0] + x * scale).put("y", location[1] + y * scale)
    }
}
