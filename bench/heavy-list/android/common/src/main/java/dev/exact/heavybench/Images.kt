package dev.exact.heavybench

import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.os.Handler
import android.os.Looper
import android.util.LruCache
import java.util.concurrent.Executors
import java.util.concurrent.Future
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/**
 * The bundled JPEGs, decoded off the main thread at the aspect-fill pixel size of the box that shows them (never the
 * 1600 px original) into hardware bitmaps, with a cache of 300 as the iOS apps keep (SPEC, "Photos").
 */
object Images {
    private val cache = LruCache<String, Bitmap>(300)
    private val workers = Executors.newFixedThreadPool(2) { Thread(it, "images").apply { priority = Thread.NORM_PRIORITY - 1 } }
    private val main = Handler(Looper.getMainLooper())

    fun key(name: String, w: Int, h: Int) = "$name@${w}x$h"

    fun cached(key: String): Bitmap? = cache.get(key)

    /** Decodes [name] to cover [w]×[h] pixels and calls [done] on the main thread; cancel the future to drop it. */
    fun load(context: Context, name: String, w: Int, h: Int, done: (Bitmap) -> Unit): Future<*> {
        val key = key(name, w, h)
        val assets = context.applicationContext.assets
        return workers.submit {
            val bitmap = cache.get(key) ?: ImageDecoder.decodeBitmap(ImageDecoder.createSource(assets, "images/$name")) { d, info, _ ->
                val iw = info.size.width; val ih = info.size.height
                val fill = min(1f, max(w.toFloat() / iw, h.toFloat() / ih))
                d.setTargetSize(ceil(iw * fill).toInt(), ceil(ih * fill).toInt())
            }.also { cache.put(key, it) }
            if (!Thread.currentThread().isInterrupted) main.post { done(bitmap) }
        }
    }
}
