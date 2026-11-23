package dev.exact.heavybench.views

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Outline
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.TextUtils
import android.text.style.AbsoluteSizeSpan
import android.util.TypedValue
import android.text.style.BackgroundColorSpan
import android.text.style.ForegroundColorSpan
import android.text.style.TypefaceSpan
import android.text.style.UnderlineSpan
import android.view.View
import android.view.ViewGroup
import android.view.ViewOutlineProvider
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView
import dev.exact.heavybench.Colors
import dev.exact.heavybench.Images
import dev.exact.heavybench.Message
import dev.exact.heavybench.Run
import java.util.concurrent.Future
import kotlin.math.min
import kotlin.math.roundToInt

private val SEMIBOLD = Typeface.create(Typeface.DEFAULT, 600, false)
private val ITALIC = Typeface.create(Typeface.DEFAULT, 400, true)
private const val WRAP = ViewGroup.LayoutParams.WRAP_CONTENT
private const val MATCH = ViewGroup.LayoutParams.MATCH_PARENT

/** One row (SPEC, "Row"): its views are made once; a bind sets them and hides what the message has none of. */
class Row(context: Context, private val contentWidth: Int) : LinearLayout(context) {
    var react: (Int) -> Unit = {}
    private val dp = resources.displayMetrics.density
    private fun px(v: Float) = (v * dp + 0.5f).toInt()

    private val avatar = Picture(context).apply { round(-1f) }
    private val author = text(15f, Color.BLACK, SEMIBOLD).apply { setSingleLine(); ellipsize = TextUtils.TruncateAt.END }
    private val time = text(13f, Colors.SECONDARY).apply { setSingleLine() }
    private val quote = QuoteBox(context)
    private val paragraphs = LinearLayout(context).apply { orientation = VERTICAL }
    private val photos = PhotoGrid(context)
    private val link = LinkCardView(context)
    private val reactions = Flow(context, px(6f))

    init {
        layoutParams = RecyclerView.LayoutParams(MATCH, WRAP)
        orientation = HORIZONTAL
        setPadding(px(16f), px(12f), px(16f), px(12f))
        addView(avatar, LayoutParams(px(40f), px(40f)))
        val content = LinearLayout(context).apply { orientation = VERTICAL }
        addView(content, LayoutParams(0, WRAP, 1f).apply { marginStart = px(12f) })
        content.addView(LinearLayout(context).apply {
            addView(author, LayoutParams(WRAP, WRAP).apply { weight = 0f })
            addView(time, LayoutParams(WRAP, WRAP).apply { marginStart = px(6f) })
        })
        content.addView(quote, LayoutParams(MATCH, WRAP).apply { topMargin = px(6f) })
        content.addView(paragraphs, LayoutParams(MATCH, WRAP).apply { topMargin = px(6f) })
        content.addView(photos, LayoutParams(MATCH, WRAP).apply { topMargin = px(8f) })
        content.addView(link, LayoutParams(MATCH, WRAP).apply { topMargin = px(8f) })
        content.addView(reactions, LayoutParams(MATCH, WRAP).apply { topMargin = px(8f) })
    }

    fun bind(m: Message, seconds: Int) {
        avatar.show(m.avatar, px(40f), px(40f))
        author.text = m.author
        time(m, seconds)
        quote.visibility = if (m.quote == null) GONE else VISIBLE
        m.quote?.let { quote.author.text = it.author; quote.excerpt.text = it.excerpt }
        while (paragraphs.childCount < m.paragraphs.size) paragraphs.addView(
            text(16f, Color.BLACK), LayoutParams(MATCH, WRAP).apply { if (paragraphs.childCount > 0) topMargin = px(8f) })
        for (i in 0 until paragraphs.childCount) {
            val v = paragraphs.getChildAt(i) as TextView
            if (i < m.paragraphs.size) { v.visibility = VISIBLE; v.text = paragraph(m.paragraphs[i]) } else v.visibility = GONE
        }
        val p = m.photos
        photos.visibility = if (p.isNullOrEmpty()) GONE else VISIBLE
        if (p.isNullOrEmpty()) photos.clear()
        else photos.bind(p.map { it.src }, if (p.size == 1) min(contentWidth * p[0].h / p[0].w, px(320f)) else 0)
        val card = m.link
        link.visibility = if (card == null) GONE else VISIBLE
        if (card == null) link.clear()
        else link.bind(card.thumb, card.site.uppercase(), card.title, card.description)
        val r = m.reactions
        reactions.visibility = if (r.isNullOrEmpty()) GONE else VISIBLE
        if (!r.isNullOrEmpty()) {
            while (reactions.childCount < r.size) reactions.addView(Chip(context, reactions.childCount))
            for (i in 0 until reactions.childCount) {
                val c = reactions.getChildAt(i) as Chip
                if (i < r.size) { c.visibility = VISIBLE; c.emoji.text = r[i].emoji; c.count.text = r[i].count.toString() } else c.visibility = GONE
            }
        }
    }

    fun time(m: Message, seconds: Int) {
        val ago = m.ago(seconds)
        time.text = ago
        contentDescription = "${m.author}, $ago"
    }

    private fun text(size: Float, color: Int, face: Typeface? = null) = TextView(context).apply {
        textSize = size; setTextColor(color); face?.let { typeface = it }
    }

    private fun paragraph(runs: List<Run>): CharSequence {
        val b = SpannableStringBuilder()
        for (run in runs) {
            val start = b.length
            b.append(run.t)
            fun span(what: Any) = b.setSpan(what, start, b.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            when (run.s) {
                "bold" -> span(TypefaceSpan(SEMIBOLD))
                "italic" -> span(TypefaceSpan(ITALIC))
                "code" -> { span(TypefaceSpan(Typeface.MONOSPACE)); span(AbsoluteSizeSpan(TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, 15f, resources.displayMetrics).roundToInt())); span(BackgroundColorSpan(Colors.FILL)) }
                "link" -> { span(ForegroundColorSpan(Colors.LINK)); span(UnderlineSpan()) }
                "mention" -> { span(TypefaceSpan(SEMIBOLD)); span(ForegroundColorSpan(Colors.LINK)) }
                "tag" -> span(ForegroundColorSpan(Colors.TAG))
            }
        }
        return b
    }

    /** An image box: #E5E5EA until its decode at the box's pixel size arrives. */
    inner class Picture(context: Context) : ImageView(context) {
        private var key: String? = null
        private var pending: Future<*>? = null

        init { scaleType = ScaleType.CENTER_CROP; setBackgroundColor(Colors.HAIRLINE) }

        fun round(radius: Float) {
            clipToOutline = true
            outlineProvider = object : ViewOutlineProvider() {
                override fun getOutline(v: View, o: Outline) =
                    if (radius < 0) o.setOval(0, 0, v.width, v.height) else o.setRoundRect(0, 0, v.width, v.height, radius)
            }
        }

        fun show(name: String, w: Int, h: Int) {
            val k = Images.key(name, w, h)
            if (k == key) return
            key = k
            pending?.cancel(true)
            pending = null
            val hit = Images.cached(k)
            setImageBitmap(hit)
            if (hit == null) pending = Images.load(context, name, w, h) { if (key == k) setImageBitmap(it) }
        }

        fun clear() {
            if (key == null) return
            key = null
            pending?.cancel(true)
            pending = null
            setImageBitmap(null)
        }
    }

    /** The quote (SPEC 3): a 3 pt #C7C7CC bar over #F2F2F7, radius 8, padding 8; the excerpt at most 2 lines. */
    inner class QuoteBox(context: Context) : LinearLayout(context) {
        val author = text(13f, Colors.LABEL2, SEMIBOLD)
        val excerpt = text(13f, Colors.LABEL2).apply { maxLines = 2; ellipsize = TextUtils.TruncateAt.END }
        private val bar = Paint().apply { color = Colors.QUOTE_BAR }

        init {
            orientation = VERTICAL
            setWillNotDraw(false)
            setPadding(px(11f), px(8f), px(8f), px(8f))
            background = GradientDrawable().apply { setColor(Colors.FILL); cornerRadius = 8 * dp }
            clipToOutline = true
            addView(author); addView(excerpt)
        }

        override fun onDraw(c: Canvas) = c.drawRect(0f, 0f, 3 * dp, height.toFloat(), bar)
    }

    /** One to four photos (SPEC 5), radius 12, gap 4. [single] is the one photo's height. */
    inner class PhotoGrid(context: Context) : ViewGroup(context) {
        private val views = List(4) { Picture(context) }
        private val gap = px(4f)
        private var names: List<String> = emptyList()
        private var single = 0

        init {
            views.forEach { addView(it) }
            clipToOutline = true
            outlineProvider = object : ViewOutlineProvider() {
                override fun getOutline(v: View, o: Outline) = o.setRoundRect(0, 0, v.width, v.height, 12 * dp)
            }
        }

        fun bind(names: List<String>, single: Int) {
            this.names = names; this.single = single
            // The boxes are fixed by the content width, so the decodes start now, before layout.
            frames(contentWidth).forEachIndexed { i, f -> views[i].show(names[i], f[2], f[3]) }
            views.forEachIndexed { i, v ->
                v.visibility = if (i < names.size) VISIBLE else GONE
                if (i >= names.size) v.clear()
            }
            requestLayout()
        }

        fun clear() = views.forEach { it.clear() }

        /** Each photo's x, y, width and height. */
        private fun frames(w: Int): List<IntArray> = when (names.size) {
            1 -> listOf(intArrayOf(0, 0, w, single))
            2 -> { val s = (w - gap) / 2; listOf(intArrayOf(0, 0, s, s), intArrayOf(s + gap, 0, w - s - gap, s)) }
            3 -> {
                val small = (w - gap) / 3; val big = w - gap - small; val h = (big - gap) / 2
                listOf(intArrayOf(0, 0, big, big), intArrayOf(big + gap, 0, small, h), intArrayOf(big + gap, h + gap, small, big - h - gap))
            }
            else -> {
                val s = (w - gap) / 2; val t = w - s - gap
                listOf(intArrayOf(0, 0, s, s), intArrayOf(s + gap, 0, t, s), intArrayOf(0, s + gap, s, s), intArrayOf(s + gap, s + gap, t, s))
            }
        }

        override fun onMeasure(wSpec: Int, hSpec: Int) {
            val w = MeasureSpec.getSize(wSpec)
            val f = frames(w)
            f.forEachIndexed { i, r -> views[i].measure(MeasureSpec.makeMeasureSpec(r[2], MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(r[3], MeasureSpec.EXACTLY)) }
            setMeasuredDimension(w, f.maxOf { it[1] + it[3] })
        }

        override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) =
            frames(r - l).forEachIndexed { i, f -> views[i].layout(f[0], f[1], f[0] + f[2], f[1] + f[3]) }
    }

    /** The link card (SPEC 6): a full-width × 140 thumbnail over site, title and description; 0.5 #D1D1D6 border. */
    inner class LinkCardView(context: Context) : LinearLayout(context) {
        private val thumb = Picture(context)
        private val site = text(12f, Colors.SECONDARY)
        private val title = text(15f, Color.BLACK, SEMIBOLD).apply { maxLines = 2; ellipsize = TextUtils.TruncateAt.END }
        private val description = text(13f, Colors.LABEL2).apply { maxLines = 2; ellipsize = TextUtils.TruncateAt.END }

        init {
            orientation = VERTICAL
            val shape = { fill: Int -> GradientDrawable().apply { setColor(fill); cornerRadius = 12 * dp } }
            background = shape(Color.WHITE)
            foreground = shape(Color.TRANSPARENT).apply { setStroke(maxOf(1, px(0.5f)), Colors.CARD_BORDER) }
            clipToOutline = true
            addView(thumb, LayoutParams(MATCH, px(140f)))
            addView(LinearLayout(context).apply {
                orientation = VERTICAL
                setPadding(px(10f), px(10f), px(10f), px(10f))
                addView(site); addView(title); addView(description)
            }, LayoutParams(MATCH, WRAP))
        }

        fun bind(image: String, site: String, title: String, description: String) {
            thumb.show(image, contentWidth, px(140f))
            this.site.text = site; this.title.text = title; this.description.text = description
        }

        fun clear() = thumb.clear()
    }

    /** A reaction chip (SPEC 7): emoji 14 + 4 + count 13 semibold on #F2F2F7, height 28, radius 14. */
    inner class Chip(context: Context, index: Int) : LinearLayout(context) {
        val emoji = text(14f, Color.BLACK)
        val count = text(13f, Colors.LABEL2, SEMIBOLD)

        init {
            gravity = android.view.Gravity.CENTER_VERTICAL
            setPadding(px(10f), 0, px(10f), 0)
            minimumHeight = px(28f)
            background = GradientDrawable().apply { setColor(Colors.FILL); cornerRadius = 14 * dp }
            addView(emoji)
            addView(count, LayoutParams(WRAP, WRAP).apply { marginStart = px(4f) })
            setOnClickListener { react(index) }
        }

        override fun onMeasure(wSpec: Int, hSpec: Int) =
            super.onMeasure(wSpec, MeasureSpec.makeMeasureSpec(px(28f), MeasureSpec.EXACTLY))
    }
}

/** Children in rows that wrap, [gap] apart both ways. */
class Flow(context: Context, private val gap: Int) : ViewGroup(context) {
    override fun onMeasure(wSpec: Int, hSpec: Int) {
        val width = MeasureSpec.getSize(wSpec)
        var x = 0; var y = 0; var line = 0
        for (i in 0 until childCount) {
            val c = getChildAt(i)
            if (c.visibility == GONE) continue
            c.measure(MeasureSpec.makeMeasureSpec(width, MeasureSpec.AT_MOST), MeasureSpec.UNSPECIFIED)
            if (x > 0 && x + c.measuredWidth > width) { x = 0; y += line + gap; line = 0 }
            x += c.measuredWidth + gap
            line = maxOf(line, c.measuredHeight)
        }
        setMeasuredDimension(width, y + line)
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        val width = r - l
        var x = 0; var y = 0; var line = 0
        for (i in 0 until childCount) {
            val c = getChildAt(i)
            if (c.visibility == GONE) continue
            if (x > 0 && x + c.measuredWidth > width) { x = 0; y += line + gap; line = 0 }
            c.layout(x, y, x + c.measuredWidth, y + c.measuredHeight)
            x += c.measuredWidth + gap
            line = maxOf(line, c.measuredHeight)
        }
    }
}
