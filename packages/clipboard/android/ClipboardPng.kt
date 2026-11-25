package exact.clipboard

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Rect
import java.io.DataOutputStream
import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.util.zip.CRC32

private const val PNG = -8552249625308161526L // 89 50 4e 47 0d 0a 1a 0a
private val IHDR = tag("IHDR")
private val acTL = tag("acTL")
private val fcTL = tag("fcTL")
private val IDAT = tag("IDAT")
private val fdAT = tag("fdAT")
private val IEND = tag("IEND")
private val colorChunks = listOf("PLTE", "tRNS", "gAMA", "cHRM", "sRGB", "iCCP", "cICP").map(::tag)
private fun tag(name: String) = ByteBuffer.wrap(name.toByteArray()).int
private fun malformed(): Nothing = throw InvalidImage("invalid PNG animation")

// Android decoders can return the APNG poster even when it is outside the animation.
// In that case, turn the first frame's fdAT chunks into a still PNG, then place it on the canvas.
internal fun decodeFirstFrame(source: File, check: () -> Unit, decode: (File) -> Bitmap): Bitmap {
    RandomAccessFile(source, "r").use { input ->
        if (input.length() < 33 || input.readLong() != PNG) return decode(source)
        if (input.readInt() != 13 || input.readInt() != IHDR) malformed()
        val header = ByteArray(13).also(input::readFully)
        if (input.readInt() != checksum(IHDR, header)) malformed()
        val size = ByteBuffer.wrap(header)
        val width = size.int
        val height = size.int
        checkPixels(width, height)
        var animated = false
        var poster = 0L
        while (input.filePointer < input.length()) {
            check()
            val start = input.filePointer
            val n = input.chunkLength()
            val type = input.readInt()
            val end = start + 12 + n
            when (type) {
                acTL -> animated = true
                IDAT -> {
                    if (!animated) return decode(source)
                    if (poster == 0L) poster = start
                }
                fcTL -> {
                    if (poster == 0L) return decode(source)
                    if (n != 26) malformed()
                    val control = ByteArray(n).also(input::readFully)
                    if (input.readInt() != checksum(fcTL, control)) malformed()
                    val rect = ByteBuffer.wrap(control)
                    val w = rect.getInt(4)
                    val h = rect.getInt(8)
                    val x = rect.getInt(12)
                    val y = rect.getInt(16)
                    if (w <= 0 || h <= 0 || x < 0 || y < 0 || x.toLong() + w > width || y.toLong() + h > height) malformed()
                    size.putInt(0, w).putInt(4, h)
                    val still = File.createTempFile("clipboard-frame-", null, source.parentFile)
                    val frame = try {
                        still.outputStream().buffered().use {
                            writeFrame(DataOutputStream(it), header, input, poster, end, check)
                        }
                        decode(still)
                    } finally {
                        still.delete()
                    }
                    if (w == width && h == height) return frame
                    try {
                        check()
                        val canvas = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
                        Canvas(canvas).drawBitmap(frame, null, Rect(x, y, x + w, y + h), null)
                        return canvas
                    } finally {
                        frame.recycle()
                    }
                }
            }
            input.seek(end)
        }
        if (animated) malformed()
        return decode(source)
    }
}

private fun RandomAccessFile.chunkLength(): Int {
    val n = readInt()
    if (n < 0 || filePointer + 8 + n > length()) malformed()
    return n
}

private fun writeFrame(out: DataOutputStream, header: ByteArray, input: RandomAccessFile, poster: Long, frame: Long, check: () -> Unit) {
    out.writeLong(PNG)
    writeChunk(out, IHDR, header)
    val buffer = ByteArray(32 * 1024)
    fun copy(count: Int, crc: CRC32? = null, original: CRC32? = null) {
        var left = count
        while (left > 0) {
            check()
            val n = minOf(left, buffer.size)
            input.readFully(buffer, 0, n)
            out.write(buffer, 0, n)
            crc?.update(buffer, 0, n)
            original?.update(buffer, 0, n)
            left -= n
        }
    }
    input.seek(33)
    while (input.filePointer < poster) {
        check()
        val n = input.chunkLength()
        val kind = input.readInt()
        if (kind in colorChunks) {
            out.writeInt(n)
            out.writeInt(kind)
            copy(n + 4)
        } else input.seek(input.filePointer + n + 4)
    }
    input.seek(frame)
    while (input.filePointer < input.length()) {
        check()
        val n = input.chunkLength()
        val kind = input.readInt()
        if (kind == fcTL || kind == IEND) break
        if (kind == fdAT) {
            if (n < 4) malformed()
            val sequence = input.readInt()
            val original = CRC32().apply { update(ByteBuffer.allocate(8).putInt(fdAT).putInt(sequence).array()) }
            val crc = CRC32().apply { update("IDAT".toByteArray()) }
            out.writeInt(n - 4)
            out.writeInt(IDAT)
            copy(n - 4, crc, original)
            if (input.readInt() != original.value.toInt()) malformed()
            out.writeInt(crc.value.toInt())
        } else input.seek(input.filePointer + n + 4)
    }
    writeChunk(out, IEND, byteArrayOf())
}

private fun checksum(type: Int, data: ByteArray) = CRC32().apply {
    update(ByteBuffer.allocate(4).putInt(type).array())
    update(data)
}.value.toInt()

private fun writeChunk(out: DataOutputStream, type: Int, data: ByteArray) {
    out.writeInt(data.size)
    out.writeInt(type)
    out.write(data)
    out.writeInt(checksum(type, data))
}
