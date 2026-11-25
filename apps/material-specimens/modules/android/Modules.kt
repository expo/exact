package exact.app

import android.content.Context
import android.graphics.text.TextRunShaper
import android.os.Build
import android.graphics.Typeface
import android.text.Layout
import android.util.Log
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import androidx.compose.foundation.text.BasicText
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.sp
import dev.exact.canvas.NativeModule
import dev.exact.canvas.NativeModules
import org.json.JSONArray
import org.json.JSONObject

class Modules : NativeModules {
    override val modules: Map<String, NativeModule> = mapOf("text-reference" to TextReference)
}

internal object TextReference : NativeModule {
    @JvmStatic external fun lines(text: String, size: Float, width: Float, scale: Float, family: Int, tabular: Boolean, glyphs: Boolean): FloatArray

    @JvmStatic external fun clusterFont(assets: String): Int
    @JvmStatic external fun document(revision: Int): Long
    private val benchmark = lazy {
        Thread {
            val times = (0 until 15).map { document(it) }.drop(3).sorted()
            Log.i("ExactAdvances", "documentNs=" + times.joinToString(","))
        }.start()
    }

    override fun create(context: Context, props: JSONObject): View =
        LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }

    override fun update(view: View, props: JSONObject) {
        if (view.tag == null) ClusterReference.report(view)
        benchmark.value
        val box = view as LinearLayout
        if (box.tag == props.toString()) return
        box.tag = props.toString()
        (box.getChildAt(0) as? ComposeView)?.disposeComposition()
        box.removeAllViews()
        val text = props.getString("sample")
        val size = props.getDouble("sp").toFloat()
        val room = props.optInt("room", -1)
        val family = props.optInt("family", 0)
        val tabular = props.optBoolean("tabular", false)
        val scale = view.resources.displayMetrics.density
        val width = if (room < 0) 12000 else (room * scale + .5f).toInt()
        val fonts = when (family) {
            3 -> Typeface.SERIF to FontFamily.Serif
            5 -> Typeface.MONOSPACE to FontFamily.Monospace
            else -> Typeface.DEFAULT to FontFamily.Default
        }
        val features = if (tabular) "tnum" else null
        fun reference(simple: Boolean = false, face: Typeface = fonts.first): TextView {
            val v = TextView(view.context)
            v.setPadding(0, 0, 0, 0)
            v.setTextSize(TypedValue.COMPLEX_UNIT_SP, size)
            v.typeface = face
            v.fontFeatureSettings = features
            v.text = text
            if (simple) v.breakStrategy = Layout.BREAK_STRATEGY_SIMPLE
            v.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY), 0)
            v.layout(0, 0, width, v.measuredHeight)
            return v
        }
        fun metrics(layout: Layout) = JSONArray().apply {
            for (i in 0 until layout.lineCount) put(layout.getLineWidth(i).toDouble()).put(layout.getLineEnd(i))
        }
        val tv = reference()
        val result = JSONObject().put("sample", text).put("sp", size.toDouble()).put("room", room)
            .put("family", family).put("tabular", tabular)
            .put("textView", metrics(tv.layout)).put("simple", metrics(reference(true).layout))

        val exact = lines(text, size, if (room < 0) -1f else width.toFloat(), scale, family, tabular, false)
        result.put("exact", JSONArray(exact.toList()))
        fun comparePositions(tv: TextView) {
            val drawn = lines(text, size, -1f, scale, family, tabular, true).toList().chunked(4)
            // These LTR singletons each have a caret at their pen. A GPOS
            // placement offset moves ink without moving that caret.
            if (tv.layout.getParagraphDirection(0) > 0 && drawn.distinctBy { it[3] }.size == drawn.size) {
                var pen = 0f
                val max = drawn.maxOfOrNull { (_, _, advance, start) ->
                    val delta = kotlin.math.abs(pen - tv.layout.getPrimaryHorizontal(start.toInt()))
                    pen += advance
                    delta
                } ?: 0f
                result.put("caretError", max.toDouble())
                if (Build.VERSION.SDK_INT < 31 && text.all { it.code <= 0x2014 })
                    ClusterReference.outlineError(text, tv.paint, drawn)?.let { result.put("outlineError", it.toDouble()) }
            }
            if (tabular) {
                result.put("digitWidths", JSONArray(drawn.map { it[2] }))
                val native = (0 until text.length).map { tv.layout.getPrimaryHorizontal(it + 1) - tv.layout.getPrimaryHorizontal(it) }
                result.put("nativeDigits", JSONArray(native))
            }
            if (Build.VERSION.SDK_INT >= 31) {
                val native = TextRunShaper.shapeTextRun(text, 0, text.length, 0, text.length,
                    0f, 0f, tv.layout.getParagraphDirection(0) < 0, tv.paint)
                if (native.glyphCount() == drawn.size && drawn.indices.all { native.getGlyphId(it) == drawn[it][0].toInt() }) {
                    val delta = drawn.indices.maxOfOrNull { kotlin.math.abs(native.getGlyphX(it) - drawn[it][1]) } ?: 0f
                    result.put("glyphError", delta.toDouble())
                }
            }
        }
        val compose = ComposeView(view.context)
        box.addView(compose, width, -2)
        box.addView(tv, width, -2)
        compose.setContent {
            BasicText(text, style = TextStyle(fontSize = size.sp, fontFamily = fonts.second, fontFeatureSettings = features),
                softWrap = room >= 0, onTextLayout = { layout ->
                    val widths = JSONArray()
                    for (i in 0 until layout.lineCount) {
                        widths.put((layout.getLineRight(i) - layout.getLineLeft(i)).toDouble()).put(layout.getLineEnd(i))
                    }
                    result.put("compose", widths)
                    fun hosted(v: View): EditText? {
                        if (v is EditText && v.text.toString() == text) return v
                        if (v is ViewGroup) for (i in 0 until v.childCount) hosted(v.getChildAt(i))?.let { return it }
                        return null
                    }
                    fun report(remaining: Int) {
                        if (box.tag != props.toString()) return
                        val field = hosted(view.rootView)
                        if (field?.layout != null && field.textSize == tv.textSize) {
                            result.put("editText", metrics(field.layout))
                            val oracle = if (family == 0) tv else reference(face = field.typeface)
                            if (family != 0) result.put("sameFace", metrics(oracle.layout))
                            if (room < 0) comparePositions(oracle)
                            Log.i("ExactAdvances", result.toString())
                        } else if (remaining > 0) view.postOnAnimation { report(remaining - 1) }
                        else Log.e("ExactAdvances", "Hosted field missing for $text at $size sp")
                    }
                    view.post { report(60) }
                })
        }
    }

    override fun destroy(view: View) { ((view as LinearLayout).getChildAt(0) as? ComposeView)?.disposeComposition() }
}
