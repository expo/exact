package exact.push

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

/** FCM owns the process until its service callback returns. Boot and handler work share that ten-second window. */
internal class PushMessage(private val app: Context, private val data: Map<String, String>, private val ended: () -> Unit) {
    private val main = Handler(Looper.getMainLooper())
    private val deadline = SystemClock.elapsedRealtime() + 10_000
    private val id = "push.${UUID.randomUUID()}"
    private var context: ExtensionContext? = null
    private var finished = false
    private val timeout = Runnable { finish(expired = true) }

    fun start() {
        main.post {
            if (finished) return@post
            if (SystemClock.elapsedRealtime() >= deadline) return@post finish(expired = true)
            main.postDelayed(timeout, maxOf(0, deadline - SystemClock.elapsedRealtime()))
            ExtensionContext.session(app, { !finished && SystemClock.elapsedRealtime() < deadline }) { ready ->
                if (finished) return@session
                if (!ready || SystemClock.elapsedRealtime() >= deadline) return@session finish(expired = true)
                val module = PushModule.live?.get() ?: return@session finish(expired = true)
                context = module.context
                val event = JSONObject().put("data", JSONObject(data).toString())
                module.context.wake("push", JSONArray().put(event), id) { finish() }
            }
        }
    }

    /** The service's own wait can end before the main looper gets its deadline callback. */
    fun expire() { main.post { finish(expired = true) } }

    private fun finish(expired: Boolean = false) {
        if (finished) return
        finished = true
        main.removeCallbacks(timeout)
        if (expired) context?.let { it.expireWake(id) }
        context = null
        ended()
    }
}
