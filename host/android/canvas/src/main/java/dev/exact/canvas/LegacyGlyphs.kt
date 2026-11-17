package dev.exact.canvas

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.fonts.Font
import java.lang.ref.ReferenceQueue
import java.lang.ref.WeakReference
import java.nio.ByteBuffer

/**
 * Glyph runs on Android 10 and 11, which have no `Canvas.drawGlyphs` (API 31) and no other public way to draw a glyph
 * by its id. Each glyph is drawn from its outline, read by exact2's Rust from the font file the [Font] maps
 * (`host/painted/src/canvas/legacy.rs`), the run's outlines filled as one [Path] with the run's paint (its blur too);
 * a colour glyph (an emoji's bitmap) as its image. A face's outlines are asked for once and kept, the most recently
 * drawn [OUTLINES] of each, while its reader holds the face. Entered only below API 31.
 */
internal object LegacyGlyphs {
    /** A glyph in ems from the pen, y down: its outline, or a colour image over [box]. */
    private class Glyph(val path: Path?, val image: Bitmap?, val box: RectF?)
    private val none = Glyph(null, null, null)

    /** Each face's outlines, by the face's identity: a [Font]'s own hash reads its whole buffer. A reader makes a
     * [Font] per face it is told, again for each reader, so a face is held weakly and its outlines go when the readers
     * and the rows recorded with it let it go. */
    private val faces = WeakIdentityMap<Font, Recent<Int, Glyph>>()

    /** A face's glyph, by the face's identity, which it does not keep alive: an image of a face let go is found by
     * nothing and leaves with the oldest. */
    private class Key(font: Font, val id: Int) {
        private val font = WeakReference(font)
        private val hash = 31 * System.identityHashCode(font) + id
        override fun equals(other: Any?) =
            other === this || other is Key && other.id == id && font.get().let { it != null && it === other.font.get() }
        override fun hashCode() = hash
    }

    /** Colour images, the most recently drawn [IMAGES] (each is its strike's size, about 70 KB for an emoji):
     * a picker of emoji would otherwise keep them all. A row recorded with one keeps its bitmap while it is kept. */
    private val images = Recent<Key, Glyph>(IMAGES)
    private const val IMAGES = 128
    /** Outlines kept per face: text that runs through more distinct glyphs, as browsing does, reads the oldest again. */
    private const val OUTLINES = 2048

    private fun known(font: Font, glyphs: Recent<Int, Glyph>, id: Int): Glyph {
        synchronized(glyphs) { glyphs[id] }?.let { return it }
        val key = Key(font, id)
        synchronized(images) { images[key] }?.let { return it }
        val made = make(font, id)
        if (made.image != null) synchronized(images) { images[key] = made } else synchronized(glyphs) { glyphs[id] = made }
        return made
    }

    fun draw(c: Canvas, ids: IntArray, positions: FloatArray, count: Int, font: Font, paint: Paint) {
        val glyphs = synchronized(faces) { faces.getOrPut(font) { Recent(OUTLINES) } }
        val size = paint.textSize; val skew = paint.textSkewX
        val run = Path(); val m = Matrix()
        for (g in 0 until count) {
            val glyph = known(font, glyphs, ids[g])
            val x = positions[2 * g]; val y = positions[2 * g + 1]
            if (glyph.path != null) {
                // As Skia sets a glyph: the em scaled to the size, skewed by the paint's synthetic italic, at the pen.
                m.setScale(size, size); m.postSkew(skew, 0f); m.postTranslate(x, y)
                run.addPath(glyph.path, m)
            } else if (glyph.image != null) {
                c.save(); c.translate(x, y); c.scale(size, size)
                c.drawBitmap(glyph.image, null, glyph.box!!, Paint(Paint.FILTER_BITMAP_FLAG).also { it.alpha = paint.alpha })
                c.restore()
            }
        }
        if (!run.isEmpty) c.drawPath(run, paint)
    }

    private fun make(font: Font, id: Int): Glyph {
        val axes = font.axes ?: emptyArray()
        val settings = IntArray(2 * axes.size)
        for ((k, a) in axes.withIndex()) {
            settings[2 * k] = a.tag.fold(0) { t, ch -> (t shl 8) or (ch.code and 0xFF) }
            settings[2 * k + 1] = java.lang.Float.floatToRawIntBits(a.styleValue)
        }
        val w = glyph(font.buffer, font.ttcIndex, settings, id) ?: return none
        fun f(i: Int) = java.lang.Float.intBitsToFloat(w[i])
        return when (w[0]) {
            1 -> {
                if (w[1] == 0) return none
                val p = Path()
                var i = 2
                repeat(w[1]) {
                    when (w[i]) {
                        0 -> { p.moveTo(f(i + 1), f(i + 2)); i += 3 }
                        1 -> { p.lineTo(f(i + 1), f(i + 2)); i += 3 }
                        2 -> { p.quadTo(f(i + 1), f(i + 2), f(i + 3), f(i + 4)); i += 5 }
                        3 -> { p.cubicTo(f(i + 1), f(i + 2), f(i + 3), f(i + 4), f(i + 5), f(i + 6)); i += 7 }
                        else -> { p.close(); i += 1 }
                    }
                }
                Glyph(p, null, null)
            }
            2 -> {
                val ppem = f(1); val left = w[2]; val top = w[3]; val width = w[4]; val height = w[5]
                if (width <= 0 || height <= 0) return none
                val image = Bitmap.createBitmap(w, 6, width, width, height, Bitmap.Config.ARGB_8888)
                Glyph(null, image, RectF(left / ppem, -top / ppem, (left + width) / ppem, (height - top) / ppem))
            }
            else -> none
        }
    }

    /** Glyph [id] of the face in [font] (collection index [index]) at the variation settings [axes] (tag, value bits
     * pairs), as `legacy.rs`'s `glyph_words` writes it, or null. */
    @JvmStatic private external fun glyph(font: ByteBuffer, index: Int, axes: IntArray, id: Int): IntArray?
}

/** The most recently used [limit] entries. Not synchronized. */
internal class Recent<K, V>(private val limit: Int) : LinkedHashMap<K, V>(16, 0.75f, true) {
    override fun removeEldestEntry(eldest: MutableMap.MutableEntry<K, V>) = size > limit
}

/** Values by their key's identity, each key held weakly: a key let go everywhere else is collected, and its value goes
 * at the next call. Not synchronized. */
internal class WeakIdentityMap<K : Any, V : Any> {
    /** A cleared reference equals only itself, so the queue's copy finds its entry. */
    private class Ref<K : Any>(key: K, queue: ReferenceQueue<K>?) : WeakReference<K>(key, queue) {
        private val hash = System.identityHashCode(key)
        override fun equals(other: Any?) = other === this || other is Ref<*> && get().let { it != null && it === other.get() }
        override fun hashCode() = hash
    }

    private val gone = ReferenceQueue<K>()
    private val map = HashMap<Ref<K>, V>()

    val size: Int get() = sweep().let { map.size }

    fun getOrPut(key: K, make: () -> V): V {
        sweep()
        map[Ref(key, null)]?.let { return it }
        return make().also { map[Ref(key, gone)] = it }
    }

    private fun sweep() {
        while (true) map.remove(gone.poll() ?: return)
    }
}
