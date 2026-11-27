package exact.app

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PathMeasure
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.text.TextRunShaper
import android.os.Build
import android.util.Log
import android.view.View
import android.widget.TextView
import java.io.File
import org.json.JSONArray
import org.json.JSONObject

internal object ClusterReference {
    // Compare the full native outline with the same glyphs at Exact’s origins.
    // A space has no outline. Its boundary is covered by the caret comparison.
    fun outlineError(text: String, paint: Paint, drawn: List<List<Float>>): Float? {
        fun outline(start: Int, end: Int): PathMeasure {
            val path = Path()
            paint.getTextPath(text, start, end, 0f, 0f, path)
            return PathMeasure(path, false)
        }
        val native = outline(0, text.length)
        val whole = FloatArray(2)
        val single = FloatArray(2)
        var max = 0f
        var more = native.length > 0
        for ((i, glyph) in drawn.withIndex()) {
            val start = glyph[3].toInt()
            val end = drawn.getOrNull(i + 1)?.get(3)?.toInt() ?: text.length
            val local = outline(start, end)
            if (local.length == 0f) continue
            do {
                if (!more || kotlin.math.abs(native.length - local.length) > .01f ||
                    !native.getPosTan(0f, whole, null) || !local.getPosTan(0f, single, null) ||
                    kotlin.math.abs(whole[1] - single[1]) > .01f) return null
                max = maxOf(max, kotlin.math.abs(whole[0] - single[0] - glyph[1]))
                more = native.nextContour()
            } while (local.nextContour())
        }
        return if (more) null else max
    }

    fun report(view: View) {
        val root = File(view.context.filesDir, "app")
        val family = TextReference.clusterFont(root.path)
        val scale = view.resources.displayMetrics.density
        val paint = TextView(view.context).paint.apply {
            typeface = Typeface.createFromFile(File(root, "assets/ClusterProbe.ttf"))
            textSize = 16 * scale
            color = Color.BLACK
        }
        // Each glyph is a rectangle with its left edge at the glyph origin.
        // The mark over B sits above the base, so a second scanline isolates it.
        val bitmap = Bitmap.createBitmap(512, 96, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val baseline = 64
        fun starts(y: Int) = (0 until bitmap.width).filter { x ->
            Color.alpha(bitmap.getPixel(x, y)) >= 128 &&
                (x == 0 || Color.alpha(bitmap.getPixel(x - 1, y)) < 128)
        }
        val probes = listOf("A", "fi", "B\u0301", "e\u0301", "BCB", "D", "G", "U", "J", "P")
        for (text in probes + if (Build.VERSION.SDK_INT >= 31) listOf("BXB") else emptyList()) {
            bitmap.eraseColor(Color.TRANSPARENT)
            canvas.drawText(text, 0f, baseline.toFloat(), paint)
            val ink = starts(baseline - 3) + starts(baseline - (paint.textSize * .8f).toInt())
            val drawn = TextReference.lines(text, 16f, -1f, scale, family, false, true).toList().chunked(4)
            val result = JSONObject().put("text", text).put("ink", JSONArray(ink))
                .put("ids", JSONArray(drawn.map { it[0].toInt() })).put("x", JSONArray(drawn.map { it[1] }))
                .put("advances", JSONArray(drawn.map { it[2] }))
                .put("nativeWidth", paint.getRunAdvance(text, 0, text.length, 0, text.length, false, text.length).toDouble())
            if (Build.VERSION.SDK_INT >= 31) {
                val native = TextRunShaper.shapeTextRun(text, 0, text.length, 0, text.length, 0f, 0f, false, paint)
                result.put("nativeIds", JSONArray((0 until native.glyphCount()).map { native.getGlyphId(it) }))
                result.put("nativeX", JSONArray((0 until native.glyphCount()).map { native.getGlyphX(it) }))
            }
            Log.i("ExactClusters", result.toString())
        }
    }
}
