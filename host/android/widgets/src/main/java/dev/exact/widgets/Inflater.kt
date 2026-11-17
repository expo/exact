package dev.exact.widgets

import android.app.PendingIntent
import android.content.Context
import android.content.res.ColorStateList
import android.text.SpannableString
import android.text.Spanned
import android.text.style.AbsoluteSizeSpan
import android.text.style.TextAppearanceSpan
import android.util.TypedValue.COMPLEX_UNIT_DIP
import android.widget.RemoteViews


/** Paint the launcher's views cannot draw yet (LLP 1126.003 D2): the render fails, and the provider shows the last good
 * entry or its placeholder rather than drawing the widget otherwise. */
internal class Unsupported(what: String) : Exception("not drawn on Android yet: $what")

/** Where a widget's taps go (LLP 1126.003 D6): a link's URL, else the app. */
internal interface Links {
    fun open(): PendingIntent
    fun link(href: String, index: Int): PendingIntent
}

/**
 * A snapshot tree as `RemoteViews` (LLP 1126.003 D2, D3): every view placed flat in the widget's content by margin and
 * size, nesting only for a box that clips or a link's box; one single-line `TextView` per kernel line and colour, at
 * the kernel's baseline; colour roles as the module's colour resources, which the launcher resolves.
 */
@android.annotation.TargetApi(31)
internal class Inflater(private val context: Context, private val links: Links, private val nowMs: Double, private val elapsedMs: Long) {
    private val pkg = context.packageName
    private val density = context.resources.displayMetrics.density
    private var linkIndex = 0

    /** The widget: [root] laid out at the launcher's size less [inset] a side, the system's content margin. */
    fun widget(root: Node, inset: Float): RemoteViews {
        val views = RemoteViews(pkg, R.layout.exact_widget_root)
        // The entry's root background is the widget's (LLP 1126 D5), else the surface the layout has.
        root.background?.let { paint(views, R.id.exact_widget_fill, it, 1f) }
        // A link at the root takes the whole widget's tap, as `widgetURL` does on iOS.
        views.setOnClickPendingIntent(android.R.id.background, root.href?.let { links.link(it, linkIndex++) } ?: links.open())
        // The launcher reapplies an update to the views it has, where `addView` appends: the content starts empty.
        views.removeAllViews(R.id.exact_widget_content)
        contents(views, R.id.exact_widget_content, root, inset, inset, 1, 1f, isRoot = true)
        return views
    }

    private fun contents(parent: RemoteViews, parentId: Int, node: Node, ox: Float, oy: Float, depth: Int, opacity: Float, isRoot: Boolean = false) {
        node.picture.firstOrNull()?.let { throw Unsupported(it) }
        node.image?.let { throw Unsupported("image") }
        val x = ox + node.x
        val y = oy + node.y
        // Opacity multiplied into each leaf equals CSS's group opacity only where the group's leaves do not overlap.
        if (node.opacity < 1f && overlaps(node)) throw Unsupported("opacity over overlapping content")
        val alpha = opacity * node.opacity
        if (!isRoot && (node.clip || node.href != null)) {
            if (depth >= MAX_DEPTH) throw Unsupported("boxes nested deeper than $MAX_DEPTH")
            // A radius clips only a box that clips (CSS's overflow): a link's box that does not clips nothing.
            val box = RemoteViews(pkg, if (node.clip) R.layout.exact_widget_box else R.layout.exact_widget_link)
            place(box, R.id.exact_widget_view, x, y, node.width, node.height)
            if (node.clip) box.setViewOutlinePreferredRadius(R.id.exact_widget_view, node.radius, COMPLEX_UNIT_DIP)
            node.href?.let { box.setOnClickPendingIntent(R.id.exact_widget_view, links.link(it, linkIndex++)) }
            own(box, R.id.exact_widget_view, node, 0f, 0f, depth + 1, alpha, describe = true)
            parent.addView(parentId, box)
            return
        }
        own(parent, parentId, node, x, y, depth, alpha, describe = !isRoot, isRoot = isRoot)
    }

    /** The node's own paint and content at (x, y) in `parentId`'s box, then its children. */
    private fun own(parent: RemoteViews, parentId: Int, node: Node, x: Float, y: Float, depth: Int, alpha: Float, describe: Boolean, isRoot: Boolean = false) {
        if (!isRoot) node.background?.let { background ->
            val fill = RemoteViews(pkg, R.layout.exact_widget_paint)
            place(fill, R.id.exact_widget_view, x, y, node.width, node.height)
            fill.setViewOutlinePreferredRadius(R.id.exact_widget_view, node.radius, COMPLEX_UNIT_DIP)
            paint(fill, R.id.exact_widget_view, background, alpha)
            parent.addView(parentId, fill)
        }
        node.border?.let { border ->
            if (node.radius > 0f) throw Unsupported("a rounded border")
            val w = border.width
            for ((ex, ey, ew, eh) in listOf(
                floatArrayOf(x, y, node.width, w), floatArrayOf(x, y + node.height - w, node.width, w),
                floatArrayOf(x, y, w, node.height), floatArrayOf(x + node.width - w, y, w, node.height),
            )) {
                val edge = RemoteViews(pkg, R.layout.exact_widget_paint)
                place(edge, R.id.exact_widget_view, ex, ey, ew, eh)
                paint(edge, R.id.exact_widget_view, border.color, alpha)
                parent.addView(parentId, edge)
            }
        }
        var described = !describe
        val description = node.label ?: node.lines.joinToString(" ") { it.text }.ifEmpty { null }
        // Contract's countdown stops at zero and its timer is an absolute duration; a `Chronometer` does neither, so it
        // draws a countdown only while its instant is ahead and the kernel's text after (LLP 1126.003 D6).
        val countdown = node.time?.takeIf { (it.format == "timer" || it.format == "countdown") && (it.format == "timer" || it.datetime > nowMs) }
        for ((i, line) in node.lines.withIndex()) {
            if (countdown != null && i == 0) {
                parent.addView(parentId, chronometer(line, countdown, x, y, alpha))
                described = true
                continue
            }
            for (group in groups(line)) {
                val shown = spanned(line, group)
                val text = text(shown, group, line.rtl, node.heading != null)
                if (!described && description != null) {
                    text.setContentDescription(R.id.exact_widget_view, description)
                    described = true
                }
                val drawn = measure(shown, group.first().size, line.rtl)
                val width = group.last().x + group.last().width - group.first().x
                place(text, R.id.exact_widget_view, x + group.first().x, y + line.baseline - drawn.baseline,
                    maxOf(width, drawn.width) + SLACK, drawn.height)
                parent.addView(parentId, text)
            }
        }
        node.progress?.let { progress ->
            val bar = RemoteViews(pkg, R.layout.exact_widget_progress)
            place(bar, R.id.exact_widget_view, x, y, node.width, node.height)
            val max = (progress.max * SCALE).toInt().coerceAtLeast(1)
            bar.setProgressBar(R.id.exact_widget_view, max, (progress.value * SCALE).toInt().coerceIn(0, max), false)
            color(bar, R.id.exact_widget_view, "setProgressTintList", progress.color)
            if (!described && node.label != null) bar.setContentDescription(R.id.exact_widget_view, node.label)
            parent.addView(parentId, bar)
        }
        for (child in node.children) contents(parent, parentId, child, x, y, depth, alpha)
    }

    /** Whether any two of the boxes [node] paints overlap: its own background and each descendant's paint or text. */
    private fun overlaps(node: Node): Boolean {
        val boxes = mutableListOf<FloatArray>()
        fun collect(n: Node, ox: Float, oy: Float) {
            val x = ox + n.x
            val y = oy + n.y
            if (n.background != null || n.border != null || n.lines.isNotEmpty() || n.progress != null) {
                boxes.add(floatArrayOf(x, y, x + n.width, y + n.height))
            }
            n.children.forEach { collect(it, x, y) }
        }
        collect(node, -node.x, -node.y)
        for (i in boxes.indices) for (j in i + 1 until boxes.size) {
            val a = boxes[i]
            val b = boxes[j]
            if (a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]) return true
        }
        return false
    }

    /** A line's runs, split where the colour changes: a view's text colour is one, so a role stays a resource. */
    private fun groups(line: Line): List<List<Run>> {
        val out = mutableListOf<MutableList<Run>>()
        for (run in line.runs) {
            if (run.letterSpacing != 0f) throw Unsupported("letter-spacing")
            val last = out.lastOrNull()
            if (last != null && last.last().color == run.color) last.add(run) else out.add(mutableListOf(run))
        }
        // Each piece is shaped alone: a split must not join right-to-left letters or fall inside a word.
        for (i in 1 until out.size) {
            val at = out[i].first().start
            if (line.rtl || !(line.text[at - 1].isWhitespace() || line.text[at].isWhitespace())) {
                throw Unsupported("a colour change inside a word or a right-to-left line")
            }
        }
        return out
    }

    /** A colour's run of a line, each run's weight, style, digits and size a span (D2). */
    private fun spanned(line: Line, group: List<Run>): SpannableString {
        val start = group.first().start
        val shown = SpannableString(line.text.substring(start, group.last().end))
        for (run in group) {
            val (a, b) = run.start - start to run.end - start
            shown.setSpan(TextAppearanceSpan(context, style(run)), a, b, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            shown.setSpan(AbsoluteSizeSpan(run.size.toPx(), false), a, b, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
        return shown
    }

    private fun text(shown: CharSequence, group: List<Run>, rtl: Boolean, heading: Boolean): RemoteViews {
        val views = RemoteViews(pkg, if (heading) R.layout.exact_widget_heading else R.layout.exact_widget_text)
        views.setTextViewTextSize(R.id.exact_widget_view, COMPLEX_UNIT_DIP, group.first().size)
        views.setTextViewText(R.id.exact_widget_view, shown)
        if (rtl) views.setInt(R.id.exact_widget_view, "setTextDirection", android.view.View.TEXT_DIRECTION_RTL)
        group.first().color?.let { color(views, R.id.exact_widget_view, "setTextColor", it) }
        return views
    }

    /** What the launcher's `TextView` draws, in dp: its baseline below its top, its line's width and its height. */
    private class Drawn(val baseline: Float, val width: Float, val height: Float)

    /** The text layout measured here, with the device's own fonts and framework, as the launcher will lay it out: a
     * fallback font's taller line (CJK, Arabic) then moves the view, not the baseline the kernel chose. */
    private val measurer by lazy {
        (android.view.LayoutInflater.from(context).inflate(R.layout.exact_widget_text, null) as android.widget.TextView).apply {
            // A laid-out view relays its text out on the next `setText`, which reads its layout parameters.
            layoutParams = android.view.ViewGroup.LayoutParams(android.view.ViewGroup.LayoutParams.WRAP_CONTENT, android.view.ViewGroup.LayoutParams.WRAP_CONTENT)
        }
    }

    private fun measure(shown: CharSequence, size: Float, rtl: Boolean): Drawn {
        val view = measurer
        view.setTextSize(COMPLEX_UNIT_DIP, size)
        view.textDirection = if (rtl) android.view.View.TEXT_DIRECTION_RTL else android.view.View.TEXT_DIRECTION_ANY_RTL
        view.text = shown
        val unbounded = android.view.View.MeasureSpec.makeMeasureSpec(0, android.view.View.MeasureSpec.UNSPECIFIED)
        view.measure(unbounded, unbounded)
        view.layout(0, 0, view.measuredWidth, view.measuredHeight)
        return Drawn(view.baseline / density, view.layout.getLineWidth(0) / density, view.measuredHeight / density)
    }

    private fun chronometer(line: Line, time: Time, x: Float, y: Float, alpha: Float): RemoteViews {
        val views = RemoteViews(pkg, R.layout.exact_widget_chronometer)
        val run = line.runs.first()
        // The launcher's clock is `elapsedRealtime`: the instant is that far from now on it.
        views.setChronometer(R.id.exact_widget_view, elapsedMs + (time.datetime - nowMs).toLong(), null, true)
        views.setChronometerCountDown(R.id.exact_widget_view, time.datetime > nowMs)
        views.setTextViewTextSize(R.id.exact_widget_view, COMPLEX_UNIT_DIP, run.size)
        run.color?.let { color(views, R.id.exact_widget_view, "setTextColor", it) }
        if (alpha < 1f) views.setFloat(R.id.exact_widget_view, "setAlpha", alpha)
        // The kernel reserved the format's widest value (LLP 1126 D6): the view takes that box, and room for the minus
        // sign a countdown shows past its instant until the next render.
        val drawn = measure(android.text.TextUtils.concat("\u2212", spanned(line, line.runs)), run.size, false)
        place(views, R.id.exact_widget_view, x + line.x, y + line.baseline - drawn.baseline, maxOf(line.width, drawn.width) + SLACK, drawn.height)
        return views
    }

    private fun style(run: Run): Int {
        val weight = ((run.weight + 50) / 100).coerceIn(1, 9)
        return STYLES[((weight - 1) * 2 + (if (run.italic) 1 else 0)) * 2 + (if (run.tabular) 1 else 0)]
    }

    private fun paint(views: RemoteViews, id: Int, c: Color, alpha: Float) = color(views, id, "setImageTintList", c, alpha)

    private fun color(views: RemoteViews, id: Int, method: String, c: Color, alpha: Float = 1f) {
        val role = c.role
        if (role != null && role in ROLES.indices) views.setColorStateList(id, method, ROLES[role])
        else views.setColorStateList(id, method, ColorStateList.valueOf(c.light), ColorStateList.valueOf(c.dark))
        val shown = alpha * (if (role != null) c.alpha / 100f else 1f)
        if (shown < 1f) views.setFloat(id, "setAlpha", shown)
    }

    private fun place(views: RemoteViews, id: Int, x: Float, y: Float, width: Float, height: Float) {
        views.setViewLayoutMargin(id, RemoteViews.MARGIN_LEFT, x, COMPLEX_UNIT_DIP)
        views.setViewLayoutMargin(id, RemoteViews.MARGIN_TOP, y, COMPLEX_UNIT_DIP)
        views.setViewLayoutWidth(id, width.coerceAtLeast(0f), COMPLEX_UNIT_DIP)
        views.setViewLayoutHeight(id, height.coerceAtLeast(0f), COMPLEX_UNIT_DIP)
    }

    private fun Float.toPx(): Int = Math.round(this * density)

    companion object {
        /** `RemoteViews` refuses a hierarchy deeper than ten; the root and its content are two. */
        const val MAX_DEPTH = 8

        /** Room past a line's advance, so a launcher whose measure rounds up does not ellipsize it. */
        const val SLACK = 2f

        /** A progress bar's integer steps per unit of its value. */
        private const val SCALE = 1000f

        private val ROLES = intArrayOf(
            R.color.exact_widget_role_0, R.color.exact_widget_role_1, R.color.exact_widget_role_2,
            R.color.exact_widget_role_3, R.color.exact_widget_role_4, R.color.exact_widget_role_5,
            R.color.exact_widget_role_6, R.color.exact_widget_role_7, R.color.exact_widget_role_8,
            R.color.exact_widget_role_9, R.color.exact_widget_role_10, R.color.exact_widget_role_11,
            R.color.exact_widget_role_12, R.color.exact_widget_role_13, R.color.exact_widget_role_14,
            R.color.exact_widget_role_15, R.color.exact_widget_role_16, R.color.exact_widget_role_17,
            R.color.exact_widget_role_18, R.color.exact_widget_role_19, R.color.exact_widget_role_20,
            R.color.exact_widget_role_21, R.color.exact_widget_role_22, R.color.exact_widget_role_23,
            R.color.exact_widget_role_24, R.color.exact_widget_role_25, R.color.exact_widget_role_26,
            R.color.exact_widget_role_27, R.color.exact_widget_role_28, R.color.exact_widget_role_29,
            R.color.exact_widget_role_30, R.color.exact_widget_role_31, R.color.exact_widget_role_32,
            R.color.exact_widget_role_33, R.color.exact_widget_role_34, R.color.exact_widget_role_35,
        )

        /** `ExactWidgetText.W<weight>[.Italic][.Tabular]`, by weight, then italic, then tabular. */
        private val STYLES = intArrayOf(
            R.style.ExactWidgetText_W100, R.style.ExactWidgetText_W100_Tabular, R.style.ExactWidgetText_W100_Italic, R.style.ExactWidgetText_W100_Italic_Tabular,
            R.style.ExactWidgetText_W200, R.style.ExactWidgetText_W200_Tabular, R.style.ExactWidgetText_W200_Italic, R.style.ExactWidgetText_W200_Italic_Tabular,
            R.style.ExactWidgetText_W300, R.style.ExactWidgetText_W300_Tabular, R.style.ExactWidgetText_W300_Italic, R.style.ExactWidgetText_W300_Italic_Tabular,
            R.style.ExactWidgetText_W400, R.style.ExactWidgetText_W400_Tabular, R.style.ExactWidgetText_W400_Italic, R.style.ExactWidgetText_W400_Italic_Tabular,
            R.style.ExactWidgetText_W500, R.style.ExactWidgetText_W500_Tabular, R.style.ExactWidgetText_W500_Italic, R.style.ExactWidgetText_W500_Italic_Tabular,
            R.style.ExactWidgetText_W600, R.style.ExactWidgetText_W600_Tabular, R.style.ExactWidgetText_W600_Italic, R.style.ExactWidgetText_W600_Italic_Tabular,
            R.style.ExactWidgetText_W700, R.style.ExactWidgetText_W700_Tabular, R.style.ExactWidgetText_W700_Italic, R.style.ExactWidgetText_W700_Italic_Tabular,
            R.style.ExactWidgetText_W800, R.style.ExactWidgetText_W800_Tabular, R.style.ExactWidgetText_W800_Italic, R.style.ExactWidgetText_W800_Italic_Tabular,
            R.style.ExactWidgetText_W900, R.style.ExactWidgetText_W900_Tabular, R.style.ExactWidgetText_W900_Italic, R.style.ExactWidgetText_W900_Italic_Tabular,
        )
    }
}
