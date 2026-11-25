package exact.app

import dev.exact.canvas.ExactApp
import dev.exact.canvas.ExactModule
import dev.exact.canvas.ExactReply
import org.json.JSONObject

/** Discovered by its APK name. The device-facing entries run on different threads. */
class Module : ExactModule() {
    init {
        if (armInConstructor) context.after(5.0) { context.changed("built") }
        check(!broken) { "constructor failed" }
        made = this
    }
    var ends = 0
    var appWords: ExactApp? = null
    var pending: ExactReply? = null
    var thread: Thread? = null
    override fun call(request: JSONObject): JSONObject {
        thread = Thread.currentThread()
        if (request.has("throw")) throw AssertionError("call failed")
        if (request.has("default")) return super.call(request)
        return request
    }
    override fun later(request: JSONObject, reply: ExactReply) {
        when {
            request.has("wait") -> pending = reply
            request.has("throw") -> throw NotImplementedError("later failed")
            else -> super.later(request, reply)
        }
    }
    override fun app(app: ExactApp) { appWords = app }
    override fun destroy() { ends++; context.changed("destroyed") }

    companion object {
        lateinit var made: Module
        var broken = false
        var armInConstructor = false
    }
}
