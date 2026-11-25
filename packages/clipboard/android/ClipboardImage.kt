package exact.clipboard

import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import android.os.CancellationSignal
import java.io.File
import java.io.IOException
import java.io.OutputStream

internal const val MAX_IMAGE_BYTES = 64 * 1024 * 1024
private const val MAX_PIXELS = 64_000_000L
internal fun checkPixels(width: Int, height: Int) {
    if (width <= 0 || height <= 0) throw InvalidImage("invalid clipboard image dimensions")
    if (width.toLong() * height > MAX_PIXELS)
        throw OverLimit("clipboard image is over 64,000,000 pixels")
}
private fun imageLimit(): Nothing = throw OverLimit("image/png is over 64 MiB")

internal typealias ProviderWork = (CancellationSignal) -> Unit

internal fun imageFormats(app: Context, uri: Uri, imageLabel: Boolean, deadline: Long,
                          lookupNative: (ProviderWork) -> Unit, lookupStreams: (ProviderWork) -> Unit, offer: (String) -> Unit) {
    val resolver = app.contentResolver
    val empty = java.util.concurrent.atomic.AtomicInteger()
    fun noType() { if (imageLabel && empty.incrementAndGet() == 2) offer("*/*") }
    fun authorized(signal: CancellationSignal): Boolean {
        if (!readable(app, uri, image = true)) return false
        checkTime(signal, deadline)
        return true
    }

    // A provider's native type and its converted streams are independent Binder calls.
    lookupNative { signal ->
        if (authorized(signal)) {
            val native = resolver.getType(uri)
            checkTime(signal, deadline)
            if (native == null) noType() else if (native.startsWith("image/")) offer(native)
        }
    }
    lookupStreams { signal ->
        if (authorized(signal)) {
            val alternatives = resolver.getStreamTypes(uri, "image/*").orEmpty().filter { it.startsWith("image/") }
            checkTime(signal, deadline)
            if (alternatives.isEmpty()) noType() else alternatives.forEach(offer)
        }
    }
}

// Opening and copying four provider files can overlap. Only one full-size bitmap is decoded at a time.
private val decoder = java.util.concurrent.Semaphore(1, true)

internal fun uriImage(app: Context, uri: Uri, type: String, signal: CancellationSignal, deadline: Long): File {
    checkTime(signal, deadline)
    val descriptor = (if (type == "*/*") app.contentResolver.openAssetFileDescriptor(uri, "r", signal)
        else app.contentResolver.openTypedAssetFileDescriptor(uri, type, null, signal))
        ?: throw IOException("clipboard provider returned no image")
    val source = descriptor.use {
        checkTime(signal, deadline)
        if (it.length > MAX_IMAGE_BYTES) imageLimit()
        // Snapshot the provider once. Pipes cannot rewind, and a reopened URI can name different pixels.
        val file = File.createTempFile("clipboard-source-", null, app.cacheDir)
        try {
            file.outputStream().use { out -> copyProvider(it, out, MAX_IMAGE_BYTES, signal, deadline, ::imageLimit) }
            file
        } catch (e: Throwable) {
            file.delete()
            throw e
        }
    }
    try {
        checkTime(signal, deadline)
        val time = (deadline - android.os.SystemClock.uptimeMillis()).coerceAtLeast(0)
        if (!decoder.tryAcquire(time, java.util.concurrent.TimeUnit.MILLISECONDS)) throw android.os.OperationCanceledException()
        try {
            return sanitizedPNG(source, app.cacheDir, MAX_IMAGE_BYTES, signal, deadline)
        } finally {
            decoder.release()
        }
    } finally {
        source.delete()
    }
}

internal fun sanitizedPNG(source: File, directory: File, remaining: Int, signal: CancellationSignal, deadline: Long): File {
    if (source.length() > MAX_IMAGE_BYTES) imageLimit()
    checkTime(signal, deadline)
    val bitmap = try {
        decodeFirstFrame(source, { checkTime(signal, deadline) }) { frame ->
            ImageDecoder.decodeBitmap(ImageDecoder.createSource(frame)) { decoder, info, _ ->
                checkPixels(info.size.width, info.size.height)
                checkTime(signal, deadline)
                decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            }
        }
    } catch (e: ImageDecoder.DecodeException) {
        throw InvalidImage("could not decode clipboard image")
    } catch (_: java.io.EOFException) {
        throw InvalidImage("incomplete clipboard image")
    } catch (_: OutOfMemoryError) {
        throw InvalidImage("insufficient memory to decode clipboard image")
    }
    try {
        checkTime(signal, deadline)
        if (!directory.isDirectory && !directory.mkdirs() && !directory.isDirectory) throw IOException("temporary storage unavailable")
        val png = File.createTempFile("clipboard-", ".png", directory)
        try {
            png.outputStream().use { file ->
                // Bitmap.compress can report a stream failure as false. Preserve the reason for the refusal.
                var written = 0
                var overflow = false
                val bounded = object : OutputStream() {
                    override fun write(b: Int) = write(byteArrayOf(b.toByte()), 0, 1)
                    override fun write(b: ByteArray, off: Int, len: Int) {
                        checkTime(signal, deadline)
                        if (len > remaining - written) {
                            overflow = true
                            imageLimit()
                        }
                        file.write(b, off, len)
                        written += len
                    }
                }
                val ok = bitmap.compress(Bitmap.CompressFormat.PNG, 100, bounded)
                if (overflow) imageLimit()
                if (!ok) throw IOException("could not encode clipboard image")
            }
            checkTime(signal, deadline)
            return png
        } catch (e: Throwable) {
            png.delete()
            throw e
        }
    } catch (_: OutOfMemoryError) {
        throw InvalidImage("insufficient memory to encode clipboard image")
    } finally {
        bitmap.recycle()
    }
}
