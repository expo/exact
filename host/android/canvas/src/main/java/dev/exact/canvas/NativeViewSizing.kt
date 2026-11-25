package dev.exact.canvas

import android.util.SizeF
import android.view.View
import android.view.View.MeasureSpec
import android.widget.FrameLayout
import org.json.JSONArray
import org.json.JSONObject
import kotlin.math.roundToInt

/** A module view's preferred content size, in dp (LLP 1024). Each binding belongs to one use of the view. */
class NativeViewSizing internal constructor(
    private val view: View,
    private val send: (SizeF?, () -> Boolean) -> Unit,
) : View.OnAttachStateChangeListener {
    init { view.addOnAttachStateChangeListener(this) }
    private val main = android.os.Handler(view.context.mainLooper)
    private var attachment = 0L
    private var live = true
    private var detached = false
    private var queued = false
    private var reported = false
    private var delivered = false
    private var preferred: SizeF? = null
    private var sent: SizeF? = null
    internal var wraps = false
        private set

    /** Follow Android's wrap_content measurement. Call on the main thread when the module binds its view. */
    fun wrapContent() {
        wraps = true
        view.requestLayout()
    }

    /** Report from any thread. Null clears the preference. A later report in the frame supersedes this one. */
    @Synchronized fun intrinsicSize(size: SizeF?) {
        if (!live || detached || size != null && !(size.width.isFinite() && size.height.isFinite() && size.width > 0 && size.height > 0)) return
        if (reported && preferred == size) return
        reported = true
        preferred = size
        schedule()
    }

    @Synchronized private fun schedule() {
        if (queued) return
        queued = true
        val version = attachment
        main.post { synchronized(this) {
            if (live && queued && attachment == version) view.postOnAnimation(flush)
        } }
    }

    private val flush = Runnable {
        synchronized(this) {
            queued = false
            if (live && view.isAttachedToWindow && (!delivered || preferred != sent)) {
                delivered = true
                sent = preferred
                val version = attachment
                send(sent) { synchronized(this) { live && attachment == version } }
            }
        }
    }

    @Synchronized override fun onViewAttachedToWindow(view: View) {
        detached = false
        if (reported && !delivered) schedule()
    }

    @Synchronized override fun onViewDetachedFromWindow(view: View) {
        attachment++
        detached = true
        view.removeCallbacks(flush)
        queued = false
        // Keep the last live sample for reattachment. The old delivery stays invalid even then.
        delivered = false
    }

    @Synchronized internal fun retire() {
        live = false
        view.removeOnAttachStateChangeListener(this)
        view.removeCallbacks(flush)
    }

    private var offeredWidth: Int? = null
    private var autoHeight = true
    private var hasPreferred = false
    private var width = 0
    private var height = 0

    internal fun offer(props: JSONObject): Boolean {
        if (!props.has("autoHeight")) return false
        val beforePreferred = hasPreferred
        hasPreferred = props.optBoolean("hasPreferred")
        val before = offeredWidth
        val beforeHeight = autoHeight
        val dp = props.optDouble("autoWidth")
        offeredWidth = dp.takeIf { it.isFinite() }?.let { (it * view.resources.displayMetrics.density).roundToInt().coerceAtLeast(0) }
        autoHeight = props.optBoolean("autoHeight", true)
        return wraps && (before != offeredWidth || beforeHeight != autoHeight || beforePreferred != hasPreferred && width == 0)
    }

    internal fun params(w: Int, h: Int): FrameLayout.LayoutParams {
        width = w
        height = h
        return FrameLayout.LayoutParams(
            if (wraps && offeredWidth != null) FrameLayout.LayoutParams.WRAP_CONTENT else w,
            if (wraps && autoHeight) FrameLayout.LayoutParams.WRAP_CONTENT else h,
        )
    }

    internal fun measure() {
        if (offeredWidth == null && !autoHeight) {
            view.measure(exact(width), exact(height))
            return
        }
        val unspecified = MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED)
        val naturalWidth = offeredWidth?.let { MeasureSpec.makeMeasureSpec(it, MeasureSpec.AT_MOST) }
            ?: if (width > 0) exact(width) else unspecified
        val naturalHeight = if (autoHeight || height == 0) unspecified else exact(height)
        view.measure(naturalWidth, naturalHeight)
        val w = view.measuredWidth
        // CSS can stretch or shrink a widget. Keep its preferred width and wrap at the width it got.
        if (autoHeight && (width > 0 || hasPreferred || offeredWidth == null) && width != w) view.measure(exact(width), naturalHeight)
        val h = view.measuredHeight
        val density = view.resources.displayMetrics.density
        if (w > 0 && h > 0) intrinsicSize(SizeF(w / density, h / density))
        // Compose uses its last constraints to decide whether a content change needs parent
        // measurement. A capped auto height must still allow shorter content to request layout.
        if (view.measuredWidth != width || view.measuredHeight != height) {
            val boxHeight = if (autoHeight && h > height) MeasureSpec.makeMeasureSpec(height, MeasureSpec.AT_MOST) else exact(height)
            view.measure(exact(width), boxHeight)
        }
    }

    private fun exact(px: Int) = MeasureSpec.makeMeasureSpec(px, MeasureSpec.EXACTLY)
}

/** All size reports from one display frame enter the kernel in a single layout. */
private object ModuleSizes {
    private val main = android.os.Handler(android.os.Looper.getMainLooper())
    private var batch = LinkedHashMap<ModuleBinding, () -> JSONArray?>()

    fun add(binding: ModuleBinding, report: () -> JSONArray?) {
        if (batch.isEmpty()) main.post {
            val pending = batch
            batch = LinkedHashMap()
            Exact.moduleSizes(pending.values)
        }
        batch[binding] = report
    }
}

internal data class ModuleBinding(val id: Int, val slot: Int, val generation: Int, val session: Long) {
    fun bind(view: View, module: NativeModule, hasPreferred: Boolean, current: () -> Boolean): NativeViewSizing {
        val main = android.os.Handler(view.context.mainLooper)
        module.events(view) { kind, payload ->
            main.post { if (current()) Exact.moduleEvent(id, slot, generation, session, kind, payload) }
        }
        val sizing = NativeViewSizing(view, send = ::report)
        // Keep an offscreen row's last size while parked. Its new borrower supplies the next preference.
        if (hasPreferred) sizing.intrinsicSize(null)
        module.sizing(view, sizing)
        return sizing
    }

    private fun report(size: SizeF?, current: () -> Boolean) {
        ModuleSizes.add(this) {
            if (current()) JSONArray().put(id).put(slot).put(generation).put(session)
                .put(size?.let { JSONArray().put(it.width.toDouble()).put(it.height.toDouble()) } ?: JSONObject.NULL)
            else null
        }
    }
}

internal fun moduleProps(json: JSONObject): JSONObject =
    json.optString("props").takeIf { it.isNotEmpty() }?.let { runCatching { JSONObject(it) }.getOrNull() } ?: JSONObject()
