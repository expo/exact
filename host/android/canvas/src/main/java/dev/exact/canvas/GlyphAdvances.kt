package dev.exact.canvas

import android.graphics.fonts.Font
import android.graphics.fonts.FontFamily
import java.nio.ByteBuffer
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.text.PositionedGlyphs
import android.graphics.text.TextRunShaper
import android.os.Build
import android.util.LruCache
import java.io.File
import java.util.Locale

/** Advances of a resolved Parley run in device pixels (LLP 1085.000 §8).
 * The host calls on its shaping thread. Each thread owns its paints. */
internal object GlyphAdvances {
    private data class Face(val id: Long, val index: Int, val axes: String)
    private data class Resolved(val typeface: Typeface, val memoryFont: Font? = null)
    private val faces = LruCache<Face, Resolved>(32)

    private fun face(key: Face, path: String, bytes: ByteBuffer?): Resolved {
        faces.get(key)?.let { return it }
        val disk = if (path.isEmpty()) null else try {
            Typeface.Builder(File(path)).setTtcIndex(key.index).setFontVariationSettings(key.axes).build()
        } catch (_: java.io.IOException) { null }
        val typeface = if (disk != null) Resolved(disk) else {
            requireNotNull(bytes)
            val owned = ByteBuffer.allocateDirect(bytes.remaining()).apply { put(bytes); flip() }
            val font = Font.Builder(owned).setTtcIndex(key.index).setFontVariationSettings(key.axes).build()
            Resolved(Typeface.CustomFallbackBuilder(FontFamily.Builder(font).build()).setStyle(font.style).build(), font)
        }
        faces.put(key, typeface)
        return typeface
    }
    private val paints = ThreadLocal.withInitial { Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG) }

    @JvmStatic fun hasOrigins() = Build.VERSION.SDK_INT >= 31

    @JvmStatic fun measure(id: Long, path: String, bytes: ByteBuffer?, index: Int, axes: String, features: String, locale: String,
                           size: Float, text: CharArray, rtl: Boolean, extra: Int, glyphs: IntArray?, spans: IntArray?): FloatArray {
        val paint = paints.get()!!
        val resolved = face(Face(id, index, axes), path, bytes)
        paint.typeface = resolved.typeface
        paint.fontFeatureSettings = features.ifEmpty { null }
        paint.textLocale = if (locale.isEmpty()) Locale.getDefault() else Locale.forLanguageTag(locale)
        paint.textSize = size
        // The first two slots carry the CSS tab's space and the discretionary hyphen.
        val base = 2 + text.size
        val widths = FloatArray(base)
        paint.getTextRunAdvances(text, 0, text.size, 0, text.size, rtl, widths, 2)
        if (extra and 1 != 0) widths[0] = paint.getRunAdvance(" ", 0, 1, 0, 1, false, 1)
        if (extra and 2 != 0) widths[1] = paint.getRunAdvance("-", 0, 1, 0, 1, false, 1)
        if (Build.VERSION.SDK_INT >= 31 && glyphs != null) {
            fun matches(run: PositionedGlyphs, first: Int, end: Int): Boolean {
                if (run.glyphCount() != end - first) return false
                var lastFont: Font? = null
                for (i in 0 until run.glyphCount()) {
                    if (run.getGlyphId(i) != glyphs[first + i]) return false
                    val font = run.getFont(i)
                    if (font !== lastFont) {
                        val sameSource = resolved.memoryFont?.let { font.sourceIdentifier == it.sourceIdentifier }
                            ?: (font.file?.path == path)
                        if (!sameSource || font.ttcIndex != index) return false
                        lastFont = font
                    }
                }
                return true
            }
            fun shape(start: Int, end: Int) =
                TextRunShaper.shapeTextRun(text, start, end - start, 0, text.size, 0f, 0f, rtl, paint)
            val run = shape(0, text.size)
            // When the widths already reproduce every origin, no glyph-specific
            // geometry is needed. A device-table adjustment can move even a singleton.
            if (spans == null && run.glyphCount() == glyphs.size) {
                var pen = 0f
                var i = 0
                while (i < glyphs.size && kotlin.math.abs(pen - run.getGlyphX(i)) <= .001f) {
                    pen += widths[2 + (if (rtl) glyphs.lastIndex - i else i)]
                    i++
                }
                if (i == glyphs.size) return widths
            }
            val result = widths.copyOf(base + glyphs.size)
            if (matches(run, 0, glyphs.size)) {
                for (i in glyphs.indices) result[base + i] = run.getGlyphX(i)
            } else {
                // A different substitution elsewhere in this run need not discard
                // the origins of a cluster that agrees. Shape it in its full context.
                result.fill(Float.NaN, base)
                val prefix = FloatArray(text.size + 1)
                for (i in text.indices) prefix[i + 1] = prefix[i] + result[2 + i]
                fun copyOrigins(first: Int, end: Int, from: Int, to: Int) {
                    val cluster = shape(from, to)
                    val width = prefix[to] - prefix[from]
                    if (!matches(cluster, first, end) || kotlin.math.abs(cluster.advance - width) > .01f) return
                    val origin = if (rtl) prefix.last() - prefix[to] else prefix[from]
                    for (i in first until end) result[base + i] = origin + cluster.getGlyphX(i - first)
                }
                if (spans != null) {
                    for (s in spans.indices step 4) copyOrigins(spans[s], spans[s + 1], spans[s + 2], spans[s + 3])
                } else {
                    for (i in glyphs.indices) {
                        val from = if (rtl) glyphs.lastIndex - i else i
                        copyOrigins(i, i + 1, from, from + 1)
                    }
                }
            }
            return result
        }
        return widths
    }
}
