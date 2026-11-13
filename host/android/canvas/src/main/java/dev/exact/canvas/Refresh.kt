package dev.exact.canvas

import android.animation.ValueAnimator
import android.content.Context
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Outline
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.view.View
import android.view.ViewOutlineProvider
import android.view.animation.LinearInterpolator
import android.view.animation.PathInterpolator
import android.widget.FrameLayout
import kotlin.math.exp
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/**
 * Material's pull to refresh (LLP 1115.001 D12) over a scroller that hears `refresh`: Compose Material 3's
 * `PullToRefreshBox` with its default indicator, its numbers from that source (`PullToRefresh.kt`, 1.3). The list
 * does not move. A disc 40 dp across, in `surfaceContainerHigh` at elevation 3 dp, comes down from the scroller's
 * top edge at half the finger's way; past the threshold, 80 dp, it goes on with tension, to at most twice that. On it
 * an arc with an arrow in `onSurfaceVariant` grows and turns with the pull, faint until the threshold. Let go past
 * the threshold, the scroller's `refresh` is sent, the disc settles at the threshold and its arc turns as Material's
 * indeterminate indicator does, until the app's `refreshing` ends; let go short of it, the disc goes back up.
 * [Scrolling] says how far the finger pulled and when it lets go.
 */
internal class Refresh(context: Context) : FrameLayout(context) {
    private val dp = resources.displayMetrics.density
    private val threshold = 80 * dp
    private val size = 40 * dp

    /** How far down the disc is, in thresholds: up to 2. */
    var fraction = 0f
        private set
    /** The arc turns: the app is refreshing. */
    var spinning = false
        private set
    /** Its arc's animator runs. */
    val turning get() = disc.turns
    /** It is going back up ([hide]). */
    var leaving = false
        private set

    private val disc = Disc()
    private var settle: ValueAnimator? = null

    init {
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
        clipChildren = true
        addView(disc, LayoutParams(size.toInt(), size.toInt()))
        disc.elevation = 3 * dp
        disc.outlineProvider = object : ViewOutlineProvider() {
            override fun getOutline(view: View, outline: Outline) = outline.setOval(0, 0, view.width, view.height)
        }
        place()
    }

    /** The finger has pulled [distance] px past the scroller's start. */
    fun pull(distance: Float) {
        settle?.cancel(); settle = null
        val adjusted = max(0f, distance) * DRAG
        val offset = if (adjusted <= threshold) adjusted else {
            // How far past the threshold, as Material's tension has it: at most another threshold.
            val linear = (adjusted / threshold - 1f).coerceIn(0f, 2f)
            threshold + threshold * (linear - linear.pow(2) / 4f)
        }
        fraction = offset / threshold
        place()
    }

    /** The scroller's `refresh` was sent: the disc settles at the threshold and its arc turns. */
    fun refreshing() {
        spinning = true
        to(1f) {}
        disc.spin()
    }

    /** The pull let go short of the threshold, or the refresh is over: the disc goes back up, and then [done]. */
    fun hide(done: () -> Unit) {
        if (leaving) return
        leaving = true
        to(0f) { spinning = false; disc.still(); done() }
    }

    /** Material's spring for the disc (Compose's default: critically damped, stiffness 1,500), to [target]. */
    private fun to(target: Float, then: () -> Unit) {
        settle?.cancel()
        val from = fraction
        settle = ValueAnimator.ofFloat(0f, 1f).apply {
            duration = SETTLE_MS
            interpolator = LinearInterpolator()
            addUpdateListener {
                val t = it.animatedFraction * SETTLE_MS / 1000f * OMEGA
                fraction = from + (target - from) * (1f - (1f + t) * exp(-t))
                place()
            }
            addListener(object : android.animation.AnimatorListenerAdapter() {
                private var cancelled = false
                override fun onAnimationCancel(animation: android.animation.Animator) { cancelled = true }
                override fun onAnimationEnd(animation: android.animation.Animator) {
                    if (cancelled) return
                    fraction = target; place(); settle = null; then()
                }
            })
            start()
        }
    }

    private fun place() {
        disc.translationX = (width - size) / 2f
        disc.translationY = fraction * threshold - size
        // No shadow above the edge before the disc shows.
        disc.visibility = if (fraction > 0f) VISIBLE else INVISIBLE
        disc.invalidate()
    }

    /** Taken away (its list went, or its window): nothing of it goes on moving, or holds it. */
    override fun onDetachedFromWindow() {
        settle?.cancel(); settle = null
        disc.still()
        super.onDetachedFromWindow()
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        super.onLayout(changed, left, top, right, bottom)
        place()
    }

    /** The disc and what is drawn on it. */
    private inner class Disc : View(context) {
        private val night = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
        private val roles = Palette.read(context)
        private val ink = roles[(if (night) Palette.ROLES else 0) + ON_SURFACE_VARIANT]
        private val ground = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = roles[(if (night) Palette.ROLES else 0) + SURFACE_CONTAINER_HIGH] }
        private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeWidth = STROKE * dp; color = ink }
        private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = ink }
        private val arrow = Path()
        private val arc = RectF()
        private var turning: ValueAnimator? = null
        private var turned = 0L

        fun spin() {
            if (turning != null) return
            // With animations off the arc rests at its longest, as Material's indicator draws a still frame.
            if (!ValueAnimator.areAnimatorsEnabled()) { invalidate(); return }
            turning = ValueAnimator.ofFloat(0f, 1f).apply {
                duration = CYCLE_MS * 5
                repeatCount = ValueAnimator.INFINITE
                interpolator = LinearInterpolator()
                addUpdateListener { turned = it.currentPlayTime % (CYCLE_MS * 5); invalidate() }
                start()
            }
        }

        fun still() { turning?.cancel(); turning = null; turned = 0L }

        /** Whether the arc turns now: [spin]'s animator runs. */
        val turns get() = turning != null

        override fun onDraw(canvas: Canvas) {
            val c = width / 2f
            canvas.drawCircle(c, c, c, ground)
            val r = ARC * dp + STROKE * dp / 2f
            arc.set(c - r, c - r, c + r, c + r)
            if (spinning) { spinner(canvas); return }
            // The arc and its arrow as the pull has them (Material's `ArrowValues`, from SwipeRefreshLayout's).
            val p = fraction
            val adjusted = (min(1f, p) - 0.4f).coerceAtLeast(0f) * 5f / 3f
            val linear = (p - 1f).coerceIn(0f, 2f)
            val tension = linear - linear.pow(2) / 4f
            val rotation = (-0.25f + 0.4f * adjusted + tension) * 0.5f
            val start = rotation * 360f
            val end = (rotation + adjusted * 0.8f) * 360f
            val scale = min(1f, adjusted)
            val alpha = if (p >= 1f) 255 else (0.3f * 255).toInt()
            stroke.alpha = alpha; fill.alpha = alpha
            canvas.save()
            canvas.rotate(rotation, c, c)
            canvas.drawArc(arc, start, end - start, false, stroke)
            val w = ARROW_W * dp * scale
            arrow.reset()
            arrow.moveTo(0f, 0f)
            arrow.lineTo(w, 0f)
            arrow.lineTo(w / 2f, ARROW_H * dp * scale)
            arrow.close()
            arrow.offset(r + c - w / 2f, c + STROKE * dp / 2f)
            canvas.rotate(end, c, c)
            canvas.drawPath(arrow, fill)
            canvas.restore()
        }

        /** Material's indeterminate indicator, as `host/painted/src/lower.rs` has Compose move it: in cycles of 1,332
         * ms the head runs 290° ahead of the tail and the tail catches up, each eased, while the arc turns 286°. */
        private fun spinner(canvas: Canvas) {
            stroke.alpha = 255
            val cycles = turned / CYCLE_MS
            val u = (turned % CYCLE_MS).toFloat() / CYCLE_MS
            val head = if (turning == null) HEAD else HEAD * EASE.getInterpolation(min(1f, u * 2f))
            val tail = if (turning == null || u < 0.5f) 0f else HEAD * EASE.getInterpolation(u * 2f - 1f)
            val start = (cycles * (TURN + HEAD)) % 360f + TURN * u + tail - 90f
            canvas.drawArc(arc, start, max(0.1f, head - tail), false, stroke)
        }
    }

    private companion object {
        /** Material's: the disc moves at half the finger's way. */
        const val DRAG = 0.5f
        const val STROKE = 2.5f
        const val ARC = 5.5f
        const val ARROW_W = 10f
        const val ARROW_H = 5f
        const val SETTLE_MS = 300L
        /** The spring's natural frequency, rad/s: the square root of its stiffness. */
        const val OMEGA = 38.73f
        const val CYCLE_MS = 1332L
        const val HEAD = 290f
        const val TURN = 286f
        val EASE = PathInterpolator(0.4f, 0f, 0.2f, 1f)
        /** Their places in [Palette.read]'s scheme. */
        const val ON_SURFACE_VARIANT = 5
        const val SURFACE_CONTAINER_HIGH = 8
    }
}
