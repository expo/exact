package dev.exact.canvas

import android.graphics.Rect
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowLayoutInfo
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.job
import kotlinx.coroutines.launch

/** The state and dividing band of this window, in its native pixels (LLP 1078 D11).
 * A zero-width crease can divide the window. Rect.intersect would discard it. */
internal fun windowFold(features: List<FoldingFeature>, window: Rect, hasHinge: Boolean, origin: Rect = window): IntArray? {
    if (features.isEmpty() && !hasHinge) return null
    val left = window.left - origin.left
    val top = window.top - origin.top
    val right = left + window.width()
    val bottom = top + window.height()
    val visible = if (left == 0 && top == 0) features else features.filter {
        val b = it.bounds
        b.right >= left && b.left <= right && b.bottom >= top && b.top <= bottom
    }
    val state = if (visible.any { it.state == FoldingFeature.State.HALF_OPENED }) 2 else 1
    val single = intArrayOf(state, 0, 0, 0)
    for (feature in visible) {
        if (!feature.isSeparating) continue
        val b = feature.bounds
        if (feature.orientation == FoldingFeature.Orientation.VERTICAL) {
            if (b.top <= top && b.bottom >= bottom && b.left > left && b.right < right)
                return intArrayOf(state, 1, b.left - left, b.right - left)
        } else if (b.left <= left && b.right >= right && b.top > top && b.bottom < bottom) {
            return intArrayOf(state, 2, b.top - top, b.bottom - top)
        }
    }
    return single
}

internal data class FoldWindow(val bounds: Rect, val display: Int, val rotation: Int)

/** Feature coordinates are local to the window that Jetpack observed. */
internal class WindowFolds(
    private val updates: Flow<WindowLayoutInfo>,
    private var snapshot: (() -> WindowLayoutInfo)?,
    private val currentWindow: () -> FoldWindow,
    private val changed: () -> Unit,
) {
    private data class Layout(val info: WindowLayoutInfo, val window: FoldWindow)
    private class Observation(val window: FoldWindow, val previous: FoldWindow?, var layout: Layout? = null,
        val confirming: Boolean = false)
    private val scope = CoroutineScope(Dispatchers.Main)
    private var observation: Observation? = null
    private var collector: Job? = null
    private var started = false
    private var subscribing = false

    fun start() {
        if (started) return
        started = true
        subscribe()
    }

    fun stop() { scope.cancel() }

    private fun observe(next: Observation) {
        observation = next
        if (started) subscribe()
    }

    private fun moved(window: FoldWindow, previous: Observation?, current: WindowLayoutInfo? = null) {
        // While a stream-only device is silent, a pure translation leaves the last measured
        // feature at its screen position. A resize, display change or turn invalidates that frame.
        val layout = current?.let { Layout(it, window) } ?: previous?.layout?.takeIf {
            val from = it.window
            from.display == window.display && from.rotation == window.rotation &&
                from.bounds.width() == window.bounds.width() && from.bounds.height() == window.bounds.height()
        }
        observe(Observation(window, previous?.window, layout))
    }

    private fun subscribe() {
        if (subscribing) return
        subscribing = true
        scope.launch {
            // Unregister before requesting the new window's initial layout. A concurrent
            // subscriber would get Jetpack's replay of the old window-local coordinates.
            collector?.cancelAndJoin()
            subscribing = false
            val source = observation ?: return@launch
            collector = coroutineContext.job
            var first = true
            updates.collect { info ->
                if (source !== observation) return@collect
                if (snapshot == null) {
                    val now = currentWindow()
                    if (source.window != now) {
                        // The flow can be the first notice of a turn. Keep a feature whose span
                        // identifies the new window even if the replacement flow is slow to start.
                        val current = info.takeIf { it.fits(now) && !it.fits(source.window) }
                        moved(now, source, current)
                    } else if (info.fits(now)) {
                        val different = source.layout?.let { it.window != now || it.info != info } ?: true
                        if (different && !(first && source.confirming) && source.previous?.let { info.fits(it) } == true) {
                            // Empty reports and same-span features carry no evidence of which
                            // placement produced them. Confirm the change with a new registration.
                            observe(Observation(now, source.previous, source.layout, confirming = true))
                        } else source.layout = Layout(info, now)
                        first = false
                    }
                }
                changed()
            }
        }
    }

    private fun WindowLayoutInfo.fits(window: FoldWindow) = displayFeatures.filterIsInstance<FoldingFeature>().all {
        if (it.orientation == FoldingFeature.Orientation.VERTICAL) it.bounds.height() == window.bounds.height()
        else it.bounds.width() == window.bounds.width()
    }

    fun report(window: FoldWindow, hasHinge: Boolean): IntArray? {
        // A device can supply Window Extensions without a layout component. Jetpack
        // then uses its sidecar backend, which only implements the flow.
        val fresh = try { snapshot?.invoke() } catch (_: UnsupportedOperationException) {
            snapshot = null
            null
        }
        val previous = observation
        if (previous == null || (fresh == null && previous.window != window)) {
            moved(window, previous)
        }
        val layout = if (fresh != null) Layout(fresh, window) else observation?.layout
        return windowFold(layout?.info?.displayFeatures.orEmpty().filterIsInstance<FoldingFeature>(),
            window.bounds, hasHinge, layout?.window?.bounds ?: window.bounds)
    }
}
