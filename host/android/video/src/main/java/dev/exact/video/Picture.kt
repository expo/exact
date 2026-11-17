package dev.exact.video

import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/** A rect in the content box's device pixels. */
internal data class Rect(val left: Float, val top: Float, val width: Float, val height: Float)

/**
 * Where a video's picture goes in its box (LLP 1042.001 D5, D6): the rect `object-fit` makes of it, centred as
 * `object-position`'s default is, and the viewport the rendition is capped by.
 */
internal object Picture {
    /** The picture a [vw]×[vh] video makes in a [bw]×[bh] box under [fit]; the box while the size is unknown. */
    fun rect(fit: String, bw: Int, bh: Int, vw: Int, vh: Int): Rect {
        if (vw <= 0 || vh <= 0 || bw <= 0 || bh <= 0) return Rect(0f, 0f, bw.toFloat(), bh.toFloat())
        val contain = min(bw.toFloat() / vw, bh.toFloat() / vh)
        val (sx, sy) = when (fit) {
            "fill" -> bw.toFloat() / vw to bh.toFloat() / vh
            "cover" -> max(bw.toFloat() / vw, bh.toFloat() / vh).let { it to it }
            "none" -> 1f to 1f
            "scale-down" -> min(1f, contain).let { it to it }
            else -> contain to contain
        }
        val w = vw * sx; val h = vh * sy
        return Rect((bw - w) / 2, (bh - h) / 2, w, h)
    }

    /**
     * The viewport `DefaultTrackSelector` caps the rendition by (D6), or null for Media3's display default (`none`, or
     * a box with no size yet). `cover` is the whole scaled frame, of which the box shows part; `scale-down` is
     * `contain`'s box, since a rendition larger than `contain` shows no more of itself.
     */
    fun viewport(fit: String, bw: Int, bh: Int, vw: Int, vh: Int): Pair<Int, Int>? {
        if (bw <= 0 || bh <= 0 || fit == "none") return null
        if (fit != "cover" || vw <= 0 || vh <= 0) return bw to bh
        val s = max(bw.toFloat() / vw, bh.toFloat() / vh)
        return (vw * s).roundToInt() to (vh * s).roundToInt()
    }

    /** `preferredMaximumResolution`'s "WxH", or null. */
    fun resolution(text: String?): Pair<Int, Int>? {
        val parts = text?.lowercase()?.split('x') ?: return null
        if (parts.size != 2) return null
        val w = parts[0].trim().toIntOrNull() ?: return null
        val h = parts[1].trim().toIntOrNull() ?: return null
        return if (w > 0 && h > 0) w to h else null
    }
}
