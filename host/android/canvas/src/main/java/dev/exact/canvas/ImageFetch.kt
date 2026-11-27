package dev.exact.canvas

import android.annotation.SuppressLint
import android.content.Context
import android.net.http.HttpEngine
import android.net.http.HttpException
import android.net.http.UrlRequest
import android.net.http.UrlResponseInfo
import android.os.Build
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

/**
 * One remote picture's request through the platform's [HttpEngine] (Cronet in the OS, API 34 and later), which
 * multiplexes every picture of an origin over one HTTP/2 connection. exact2's download threads drive it through JNI
 * (`host/painted/src/canvas/http_engine.rs`), one blocking step at a time: they follow redirects, keep the disk cache
 * and enforce the caps themselves (`image/remote.rs`), so a redirect is answered as the response and the engine's
 * own HTTP cache stays off.
 */
@SuppressLint("NewApi") // Reached only where [install] said yes.
class ImageFetch private constructor() : UrlRequest.Callback {
    @JvmField var status = 0
    /** The response's headers as `name\nvalue\n` pairs. */
    @JvmField var headers = ""
    @JvmField var failure: String? = null
    private lateinit var request: UrlRequest
    /** A [UrlResponseInfo] (the head), an [Int] (bytes read), [DONE], or a [String] (the failure). */
    private val events = LinkedBlockingQueue<Any>()
    private val buffer = ByteBuffer.allocateDirect(CHUNK)
    private var ended = false

    /** Wait for the head; whether it came. */
    fun head(timeoutMs: Long): Boolean = when (val event = next(timeoutMs)) {
        is UrlResponseInfo -> {
            status = event.httpStatusCode
            headers = event.headers.asList.joinToString("") { "${it.key}\n${it.value}\n" }
            true
        }
        else -> false
    }

    /** Up to `max` bytes of the body into `into`: how many, 0 at its end, or -1 when it failed. */
    fun read(into: ByteArray, max: Int, timeoutMs: Long): Int {
        if (ended) return 0
        buffer.clear().limit(minOf(max, into.size, CHUNK))
        request.read(buffer)
        return when (val event = next(timeoutMs)) {
            is Int -> event.also { buffer.flip(); buffer.get(into, 0, it) }
            DONE -> 0.also { ended = true }
            else -> -1
        }
    }

    fun cancel() = request.cancel()

    private fun next(timeoutMs: Long): Any? {
        val event = events.poll(timeoutMs, TimeUnit.MILLISECONDS)
        if (event == null) {
            failure = "timed out"
            request.cancel()
        } else if (event is String) {
            failure = event
        }
        return event
    }

    // A redirect is the response: the caller follows it, and its body is empty.
    override fun onRedirectReceived(request: UrlRequest, info: UrlResponseInfo, location: String) {
        ended = true
        events.put(info)
    }
    override fun onResponseStarted(request: UrlRequest, info: UrlResponseInfo) { events.put(info) }
    override fun onReadCompleted(request: UrlRequest, info: UrlResponseInfo, buffer: ByteBuffer) {
        events.put(buffer.position())
    }
    override fun onSucceeded(request: UrlRequest, info: UrlResponseInfo) { events.put(DONE) }
    override fun onFailed(request: UrlRequest, info: UrlResponseInfo?, error: HttpException) {
        events.put(error.message ?: "failed")
    }
    override fun onCanceled(request: UrlRequest, info: UrlResponseInfo?) { events.put("canceled") }

    companion object {
        private const val CHUNK = 64 * 1024
        private val DONE = Any()
        @Volatile private var context: Context? = null
        /**
         * Built at the first picture, on its download thread: it loads the engine's native code. Null when the platform's
         * provider cannot load; exact2 then keeps its own transport.
         */
        private val engine by lazy {
            runCatching {
                HttpEngine.Builder(context!!)
                    .setEnableHttp2(true)
                    .setEnableBrotli(true)
                    .setEnableHttpCache(HttpEngine.Builder.HTTP_CACHE_DISABLED, 0)
                    .build()
            }.getOrNull()
        }
        /** The engine's callbacks only hand events over. */
        private val callbacks = Executors.newSingleThreadExecutor()

        /** Whether this platform has [HttpEngine]; below API 34 exact2 keeps its own transport. */
        @JvmStatic fun install(context: Context): Boolean {
            if (Build.VERSION.SDK_INT < 34) return false
            this.context = context
            return true
        }

        /** Whether the engine could be built. */
        @JvmStatic fun available(): Boolean = engine != null

        /** Start a GET of `url` with `headers` (`name\nvalue\n` pairs). */
        @JvmStatic fun start(url: String, headers: String): ImageFetch = ImageFetch().apply {
            val builder = engine!!.newUrlRequestBuilder(url, callbacks, this)
            headers.split('\n').chunked(2).forEach { if (it.size == 2) builder.addHeader(it[0], it[1]) }
            request = builder.build()
            request.start()
        }
    }
}
