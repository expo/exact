// @exact/browser-automation on Android (LLP 1020.002 D9, §11 "Android as built"): each call goes to the reader
// (`FrameAutomations`), which finds the app's live frame by its `id`. The runner has admitted the call under
// `browser.automation` before it reaches here.
package exact.browserautomation

import android.os.Handler
import android.os.Looper
import java.util.UUID
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.FrameAutomations
import org.json.JSONObject

class BrowserAutomationModule(context: ExtensionContext) : ExactExtension(context) {
    private val main = Handler(Looper.getMainLooper())
    private val unsent = ArrayDeque<JSONObject>()
    private val session = UUID.randomUUID().toString()
    private val receive: (JSONObject) -> Unit = ::post
    private var busy = false
    private var reported = false
    private var ended = false

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) =
        FrameAutomations.run(name, args.put("channelSession", session), receive, reply)

    private fun post(message: JSONObject) {
        if (ended) return
        val overflow = unsent.size >= 32
        if (overflow) unsent.removeFirst()
        unsent.addLast(message)
        if (overflow && !reported) {
            reported = true
            context.journal("script.message: dropped oldest unsent messages (queue limit 32)")
        }
        drain()
    }

    private fun drain() {
        if (busy || unsent.isEmpty()) return
        busy = true
        main.post {
            if (ended) return@post
            val args = JSONObject().put("params", unsent.removeFirst())
            if (unsent.isEmpty()) reported = false
            context.invoke("script.message", "script.message", args) { main.postDelayed({ busy = false; drain() }, 50) }
        }
    }

    override fun destroy(done: () -> Unit) {
        ended = true
        FrameAutomations.release(session)
        unsent.clear()
        done()
    }
}
