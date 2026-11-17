package exact.backgroundtransfer

import org.json.JSONObject

/** The import-selected JNI uses fetch's transport and the host's secret backend. */
internal object NativeTransfer {
    init { System.loadLibrary("exactcanvas") }
    @JvmStatic external fun call(request: ByteArray): ByteArray
    fun exchange(request: JSONObject): JSONObject = JSONObject(String(call(request.toString().toByteArray(Charsets.UTF_8)), Charsets.UTF_8))
}

/**
 * One exchange's handle on the transport. It is opened when the exchange is admitted, before the
 * blocking call, so a cancellation that comes first still reaches it.
 */
internal class TransferIO(private val exchange: (JSONObject) -> JSONObject = NativeTransfer::exchange) : AutoCloseable {
    private var handle: Long? = exchange(JSONObject().put("op", "open")).getLong("handle")
    @Synchronized fun cancel() { handle?.let { exchange(JSONObject().put("op", "cancel").put("handle", it)) } }
    @Synchronized fun progress(): JSONObject? = handle?.let { exchange(JSONObject().put("op", "progress").put("handle", it)) }
    fun run(request: JSONObject): JSONObject {
        val id = synchronized(this) { handle } ?: return JSONObject().put("failure", "cancelled")
        return exchange(request.put("op", "run").put("handle", id))
    }
    @Synchronized override fun close() {
        handle?.let { exchange(JSONObject().put("op", "close").put("handle", it)) }
        handle = null
    }
}
