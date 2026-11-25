package exact.clipboard

import android.content.ClipData
import android.content.ContentResolver
import android.content.Context
import android.net.Uri
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.View
import android.view.ViewTreeObserver
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.lang.ref.WeakReference
import java.nio.file.Files
import java.util.ArrayDeque
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.SynchronousQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

private val ui = Handler(Looper.getMainLooper())
private fun providerPool(size: Int = 4) = ThreadPoolExecutor(0, size, 10, TimeUnit.SECONDS, SynchronousQueue())

// A blocked open cannot occupy a metadata worker or a worker for the other representation.
// readText has its own four workers in Clipboard.kt.
private val nativeTypes = providerPool(2)
private val streamTypes = providerPool(2)
private val textOpens = providerPool()
private val extraTextOpens = providerPool(3)
private val imageOpens = providerPool()
private val imageReader = AtomicReference<Contents?>()

internal class ItemRead(context: ExtensionContext, private val view: View, clip: ClipData, private val reply: (JSONObject) -> Unit) :
    ViewTreeObserver.OnWindowFocusChangeListener, View.OnAttachStateChangeListener {
    private val deadline = SystemClock.uptimeMillis() + PROVIDER_TIMEOUT_MS
    private val contents = Contents(context.application, context.temporary, clip, deadline, WeakReference(this))
    private val timeout = Runnable { settle() }
    private var done = false

    fun start() {
        view.viewTreeObserver.addOnWindowFocusChangeListener(this)
        view.addOnAttachStateChangeListener(this)
        ui.postAtTime(timeout, deadline)
        if (!contents.start()) settle()
    }

    fun settle() = end(true)
    fun cancel() = end(false)
    override fun onWindowFocusChanged(focus: Boolean) { if (!focus) end(true, lostFocus = true) }
    override fun onViewDetachedFromWindow(view: View) = end(true, lostFocus = true)
    override fun onViewAttachedToWindow(view: View) {}

    private fun end(answer: Boolean, lostFocus: Boolean = false) {
        if (done) return
        done = true
        ui.removeCallbacks(timeout)
        if (view.viewTreeObserver.isAlive) view.viewTreeObserver.removeOnWindowFocusChangeListener(this)
        view.removeOnAttachStateChangeListener(this)
        val allowed = answer && !lostFocus && view.hasWindowFocus()
        val result = contents.finish(allowed)
        if (answer) reply(if (allowed) result else denied())
    }
}

private class Contents(
    private val app: Context, private val directory: File, clip: ClipData, private val deadline: Long,
    private val read: WeakReference<ItemRead>,
) {
    private val types = (0 until clip.description.mimeTypeCount).map { clip.description.getMimeType(it) }
    private val imageTypes = types.filter { it.startsWith("image/") && it != "image/*" }
    private val imagesOnly = types.all { it.startsWith("image/") }
    private val textAdvertised = offersText(clip.description)
    private val advertised = clip.description.hasMimeType("image/*")
    private val rows = List(clip.itemCount) { i ->
        val item = clip.getItemAt(i)
        // addItem(Item) leaves the initial description alone. In a mixed clip each item supplies its own text.
        val inline = item.text?.takeIf { clip.itemCount > 1 || textAdvertised || item.uri != null }?.toString()
        Row(item.uri?.takeIf { it.scheme == ContentResolver.SCHEME_CONTENT }, inline)
    }
    private class Row(val uri: Uri?, var text: String?) {
        val formats = mutableSetOf<String>()
        var png: String? = null
        var imageFailure: JSONObject? = null
        var outstanding = 0
    }
    private var textLeft = MAX_TEXT
    private var pngLeft = MAX_IMAGE_BYTES
    private var batch: File? = null
    private var failure: JSONObject? = null
    private var limit: JSONObject? = null
    private var busy = false
    private var frozen = false
    private var expired = false
    // Keep waiting work with the clipboard snapshot. At the deadline we can drop it all
    // without walking an executor queue, even when a provider ignores cancellation.
    private inner class Work(val pool: ThreadPoolExecutor, val extra: ThreadPoolExecutor = pool) {
        val waiting = ArrayDeque<ProviderWork>()
        val capacity = if (pool === extra) pool.maximumPoolSize else 1 + extra.maximumPoolSize
        val active = mutableSetOf<CancellationSignal>()
        var workers = 0
        private val yieldSlots = Runnable {
            val blocked = synchronized(this@Contents) { if (waiting.isEmpty()) emptyList() else active.toList() }
            blocked.forEach { it.cancel() }
        }

        // If every open is blocked and more items are waiting, give the remaining batches
        // a share of the same deadline. A provider that ignores cancellation retains its slot.
        private fun shareTime() {
            ui.removeCallbacks(yieldSlots)
            if (pool === nativeTypes || pool === streamTypes || waiting.isEmpty() || active.size != workers || workers == 0) return
            val batches = 1 + (waiting.size + workers - 1) / workers
            val now = SystemClock.uptimeMillis()
            ui.postAtTime(yieldSlots, now + ((deadline - now) / batches).coerceAtLeast(1))
        }

        fun submit(task: ProviderWork) {
            if (frozen) return
            waiting.addLast(task)
            if (workers < capacity) {
                val executor = if (workers == 0) pool else extra
                workers++
                try {
                    executor.execute(::drain)
                } catch (_: RejectedExecutionException) {
                    workers--
                    if (workers == 0) {
                        waiting.clear()
                        unreadable()
                    }
                }
            }
            shareTime()
        }

        fun stop(): List<CancellationSignal> {
            waiting.clear()
            ui.removeCallbacks(yieldSlots)
            return active.toList()
        }

        private fun drain() {
            while (true) {
                val signal = CancellationSignal()
                val task = synchronized(this@Contents) {
                    if (frozen || waiting.isEmpty() || SystemClock.uptimeMillis() >= deadline) {
                        if (waiting.isNotEmpty()) expired = true
                        waiting.clear()
                        workers--
                        if (!frozen && work.none { it.workers != 0 }) ui.post { read.get()?.settle() }
                        return
                    }
                    active.add(signal)
                    waiting.removeFirst().also { shareTime() }
                }
                try {
                    checkTime(signal, deadline)
                    task(signal)
                } catch (e: Exception) {
                    synchronized(this@Contents) {
                        if (!frozen && !signal.isCanceled) when (e) {
                            is OverLimit -> limit = refused("DataError", e.message!!)
                            is SecurityException -> failure = denied()
                            is android.os.OperationCanceledException -> if (SystemClock.uptimeMillis() < deadline) unreadable() else expired = true
                            else -> unreadable()
                        }
                    }
                } finally {
                    synchronized(this@Contents) { active.remove(signal) }
                }
            }
        }
    }
    private val native = Work(nativeTypes)
    private val streams = Work(streamTypes)
    // One primary slot per list read preserves main's capacity for four independent text pastes.
    private val texts = Work(textOpens, extraTextOpens)
    private val images = Work(imageOpens)
    private val work = listOf(native, streams, texts, images)

    @Synchronized fun start(): Boolean {
        for (row in rows) row.text?.let(::countText)
        if (limit != null) return false
        val uris = rows.filter { it.uri != null }
        if (uris.isEmpty()) return false
        val sole = uris.singleOrNull()?.uri
        // A grant makes its provider visible in this profile. A stale image URI with no installed
        // provider has no representation. A user-qualified URI may belong to another profile.
        if (!textAdvertised && sole != null && sole.authority?.contains('@') == false &&
            app.packageManager.resolveContentProvider(sole.authority!!, 0) == null) return false
        if (advertised && !claimImages()) return false
        for ((i, row) in rows.withIndex()) {
            val uri = row.uri ?: continue
            if (row.text == null) texts.submit { signal ->
                val bytes = synchronized(this) { textLeft }
                uriText(app, uri, bytes, i == 0 && textAdvertised, signal, deadline)?.let { value ->
                    synchronized(this) {
                        if (!frozen && !signal.isCanceled && SystemClock.uptimeMillis() < deadline) {
                            countText(value)
                            if (limit == null) row.text = value
                        }
                    }
                }
            }
            // Each typed open asks the provider for that format. Only a sole URI may use the raw fallback.
            if (sole != null || imagesOnly) for (type in imageTypes) offer(row, uri, type)
            imageFormats(app, uri, advertised && sole != null,
                deadline, native::submit, streams::submit) { type -> offer(row, uri, type) }
        }
        return work.any { it.workers != 0 }
    }

    private fun claimImages(): Boolean {
        if (imageReader.get() === this || imageReader.compareAndSet(null, this)) return true
        busy = true
        return false
    }

    @Synchronized private fun offer(row: Row, uri: Uri, type: String) {
        if (frozen || row.png != null || !row.formats.add(type) || !claimImages()) return
        row.outstanding++
        images.submit { signal ->
            try {
                if (synchronized(this) { row.png != null }) return@submit
                if (!readable(app, uri, image = true)) return@submit
                val file = uriImage(app, uri, type, signal, deadline)
                try {
                    synchronized(this) {
                        checkTime(signal, deadline)
                        if (!frozen && row.png == null) keep(row, file)
                    }
                } finally {
                    file.delete()
                }
            } catch (e: Exception) {
                synchronized(this) {
                    if (!frozen) {
                        val error = when (e) {
                            is InvalidImage, is OverLimit -> refused("DataError", e.message!!)
                            is SecurityException -> denied()
                            else -> refused("NotReadableError", "clipboard image unavailable")
                        }
                        if (row.imageFailure == null || error.optString("error") == "DataError") row.imageFailure = error
                    }
                }
            } finally {
                synchronized(this) { row.outstanding-- }
            }
        }
    }

    private fun countText(value: String) {
        if (value.length <= textLeft) {
            textLeft -= value.toByteArray().size
            if (textLeft >= 0) return
        }
        limit = refused("DataError", "text/plain is over 1 MiB")
    }

    private fun keep(row: Row, file: File) {
        val size = file.length()
        if (size > pngLeft) {
            limit = refused("DataError", "image/png is over 64 MiB")
            return
        }
        val folder = batch ?: Files.createTempDirectory(app.cacheDir.toPath(), "clipboard-").toFile().also { batch = it }
        if (!file.renameTo(File(folder, file.name))) throw IOException("could not stage clipboard image")
        pngLeft -= size.toInt()
        row.png = "app:/tmp/${folder.name}/${file.name}"
    }

    private fun unreadable() { if (failure == null) failure = refused("NotReadableError", "could not read clipboard contents") }

    fun finish(allowed: Boolean): JSONObject {
        val pending = synchronized(this) {
            frozen = true
            imageReader.compareAndSet(this, null)
            work.flatMap { it.stop() }
        }
        pending.forEach { it.cancel() }
        val folder = batch
        var published = false
        try {
            if (!allowed) return denied()
            limit?.let { return it }
            if (busy) return refused("NotReadableError", "another clipboard image read is in progress")
            published = folder != null && (directory.isDirectory || directory.mkdirs()) &&
                folder.renameTo(File(directory, folder.name))
            val items = JSONArray()
            for (row in rows) {
                val record = JSONObject()
                row.text?.let { record.put("text/plain", it) }
                if (published) row.png?.let { record.put("image/png", it) }
                if (record.length() != 0) items.put(record)
            }
            if (items.length() == 0) {
                if (rows.any { it.outstanding > 0 }) return refused("NotReadableError", "clipboard image unavailable")
                rows.firstNotNullOfOrNull { it.imageFailure }?.let { return it }
                if (folder != null || advertised && (expired || pending.isNotEmpty()) || rows.any { it.formats.isNotEmpty() })
                    return refused("NotReadableError", "clipboard image unavailable")
                failure?.let { return it }
            }
            return JSONObject().put("exact:answer", items)
        } finally {
            if (!published) folder?.deleteRecursively()
        }
    }
}
