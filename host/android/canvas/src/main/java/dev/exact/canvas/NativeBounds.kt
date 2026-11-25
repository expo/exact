package dev.exact.canvas

import android.graphics.Matrix
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.RenderNode
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.view.ViewTreeObserver
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityNodeProvider

/** Accessibility geometry for native subtrees recorded into the page. Their views keep their local touch coordinates. */
internal class NativeBounds(private val layer: ViewGroup, private val hidden: (View) -> Boolean, private val node: (View) -> RenderNode?) : ViewTreeObserver.OnGlobalLayoutListener {
    var enabled = false
        set(value) { field = value; if (!value) clear() }
    private val roots = HashMap<View, Placement>()

    private var observer: ViewTreeObserver? = null
    override fun onGlobalLayout() { roots.values.forEach { it.bind() } }

    fun place(graph: PaintedOrigins?, hierarchy: Boolean = false) {
        if (!enabled) return
        roots.entries.removeAll { (view, placement) ->
            val removed = view.parent !== layer || node(view) !== placement.node
            if (removed) placement.close()
            removed
        }
        if (graph == null) return
        for (i in 0 until layer.childCount) {
            val root = layer.getChildAt(i)
            val drawn = node(root) ?: continue
            val place = roots.getOrPut(root) { Placement(layer, root, graph, drawn).also { it.bind() } }
            if (hierarchy) place.bind()
            place.update(hidden(root))
        }
        val tree = if (roots.isEmpty()) null else layer.viewTreeObserver
        if (tree !== observer) {
            observer?.takeIf { it.isAlive }?.removeOnGlobalLayoutListener(this)
            observer = tree
            tree?.addOnGlobalLayoutListener(this)
        }
    }

    fun bind(root: View) { roots[root]?.bind() }

    fun clear() {
        observer?.takeIf { it.isAlive }?.removeOnGlobalLayoutListener(this)
        observer = null
        roots.values.forEach { it.close() }
        roots.clear()
    }
}

private class Placement(
    private val layer: ViewGroup, private val root: View, private val graph: PaintedOrigins, val node: RenderNode,
) {
    private val delegates = HashMap<View, BoundsDelegate>()
    private val transform = Matrix()
    private val clip = RectF()
    private val next = Matrix()
    private val visible = RectF()
    private val at = IntArray(2)

    fun bind() {
        val gone = delegates.keys.toMutableSet()
        fun visit(v: View) {
            gone.remove(v)
            // A delegate itself makes an AUTO view important. Keep decorative containers out of the tree.
            val include = v.isImportantForAccessibility || v.importantForAccessibility != View.IMPORTANT_FOR_ACCESSIBILITY_AUTO
            if (include && (delegates[v] == null || v.accessibilityDelegate !== delegates[v])) {
                val current = v.accessibilityDelegate
                val delegate = BoundsDelegate(if (current is BoundsDelegate) current.original else current, this)
                delegates[v] = delegate
                v.accessibilityDelegate = delegate
            }
            if (v is ViewGroup) for (i in 0 until v.childCount) visit(v.getChildAt(i))
        }
        visit(root)
        for (v in gone) delegates.remove(v)?.restore(v)
    }

    fun close() {
        delegates.forEach { (v, delegate) -> delegate.restore(v) }
        delegates.clear()
    }

    fun update(hidden: Boolean) {
        if (graph.publishing) return
        visible.set(0f, 0f, node.width.toFloat(), node.height.toFloat())
        if (hidden || !graph.transform(node, next, visible)) visible.setEmpty()
        if (next == transform && visible == clip) return
        transform.set(next); clip.set(visible)
        layer.notifySubtreeAccessibilityStateChanged(root, root, AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE)
    }

    /** Android clips an ordinary node at its physical origin. Start it at full size and apply the clips
     * where it is painted. A provider or delegate can supply a smaller focus target in screen coordinates. */
    @Suppress("DEPRECATION")
    fun project(v: View, info: AccessibilityNodeInfo, supplied: Rect? = null, virtual: Boolean = false) {
        val rect = Rect()
        val box = if (supplied == null) RectF(0f, 0f, v.width.toFloat(), v.height.toFloat()) else RectF(supplied).also {
            val fromScreen = Matrix()
            v.transformMatrixToLocal(fromScreen)
            fromScreen.mapRect(it)
        }
        val parent = v.parentForAccessibility as? View
        var parentPlaced = false
        var child = v
        while (true) {
            child.clipBounds?.let { box.clip(it.left, it.top, it.right, it.bottom) }
            if (child === root) break
            val container = child.parent as? ViewGroup ?: return
            child.matrix.mapRect(box)
            box.offset((child.left - container.scrollX).toFloat(), (child.top - container.scrollY).toFloat())
            if (container.clipChildren) box.clip(0, 0, container.width, container.height)
            if (container.clipToPadding) box.clip(container.paddingLeft, container.paddingTop,
                container.width - container.paddingRight, container.height - container.paddingBottom)
            if (!virtual && container === parent) { box.round(rect); info.setBoundsInParent(rect); parentPlaced = true }
            child = container
        }
        transform.mapRect(box)
        if (!box.intersect(clip) || !layer.getLocalVisibleRect(rect) || !box.intersect(RectF(rect))) box.setEmpty()
        val toScreen = Matrix()
        layer.transformMatrixToGlobal(toScreen)
        toScreen.mapRect(box)
        box.round(rect)
        if (!virtual && !parentPlaced) {
            val local = RectF(box)
            val toParent = Matrix()
            parent?.transformMatrixToLocal(toParent)
            toParent.mapRect(local)
            info.setBoundsInParent(Rect().also(local::round))
        }
        info.setBoundsInScreen(rect)
        if (rect.isEmpty) info.isVisibleToUser = false
        else if (!info.isVisibleToUser && !v.getGlobalVisibleRect(Rect()) && v.isShown && v.windowVisibility == View.VISIBLE) {
            var ancestor: View? = v
            while (ancestor != null && ancestor.alpha > 0f && ancestor.transitionAlpha > 0f) ancestor = ancestor.parent as? View
            info.isVisibleToUser = ancestor == null
        }
        if (Build.VERSION.SDK_INT >= 34) {
            layer.getLocationOnScreen(at)
            rect.offset(-at[0], -at[1])
            layer.getLocationInWindow(at)
            rect.offset(at[0], at[1])
            info.setBoundsInWindow(rect)
        }
    }

    private fun RectF.clip(left: Int, top: Int, right: Int, bottom: Int) {
        if (!intersect(left.toFloat(), top.toFloat(), right.toFloat(), bottom.toFloat())) setEmpty()
    }
}

/** Keep the module's actions, events and provider. Only the boxes returned to a service change. */
private class BoundsDelegate(val original: View.AccessibilityDelegate?, private val place: Placement) : View.AccessibilityDelegate() {
    private val base = original ?: View.AccessibilityDelegate()
    private var readingProvider = false
    private var source: AccessibilityNodeProvider? = null
    private var wrapper: AccessibilityNodeProvider? = null
    fun restore(v: View) { if (v.accessibilityDelegate === this) v.accessibilityDelegate = original }
    override fun onInitializeAccessibilityNodeInfo(v: View, info: AccessibilityNodeInfo) {
        base.onInitializeAccessibilityNodeInfo(v, info)
        if (readingProvider) return
        val screen = if (original == null) null else {
            @Suppress("DEPRECATION")
            val ordinary = AccessibilityNodeInfo.obtain()
            val standard = try {
                super.onInitializeAccessibilityNodeInfo(v, ordinary)
                Rect().also(ordinary::getBoundsInScreen)
            } finally { @Suppress("DEPRECATION") ordinary.recycle() }
            Rect().also(info::getBoundsInScreen).takeIf { it != standard }
        }
        place.project(v, info, screen)
    }

    override fun getAccessibilityNodeProvider(v: View): AccessibilityNodeProvider? {
        if (readingProvider) return base.getAccessibilityNodeProvider(v)
        // A View subclass may supply its own provider when its delegate has none.
        val provider = native { v.accessibilityNodeProvider }
        if (source !== provider) {
            source = provider
            wrapper = provider?.let { wrap(v, it) }
        }
        return wrapper
    }

    private fun wrap(v: View, provider: AccessibilityNodeProvider) =
        object : AccessibilityNodeProvider() {
            override fun createAccessibilityNodeInfo(id: Int) = native { provider.createAccessibilityNodeInfo(id) }
                ?.let { projected(v, it, id == HOST_VIEW_ID) }
            override fun findAccessibilityNodeInfosByText(text: String, id: Int) = native { provider.findAccessibilityNodeInfosByText(text, id) }
                ?.map { projected(v, it) }
            override fun findFocus(focus: Int) = native { provider.findFocus(focus) }?.let { projected(v, it) }
            override fun performAction(id: Int, action: Int, args: Bundle?) = provider.performAction(id, action, args)
            override fun addExtraDataToAccessibilityNodeInfo(id: Int, info: AccessibilityNodeInfo, key: String, args: Bundle?) =
                provider.addExtraDataToAccessibilityNodeInfo(id, info, key, args)
        }

    // Providers may initialize the host node or keep a node for the next query. Project only the answer sent out.
    private inline fun <T> native(read: () -> T): T {
        val previous = readingProvider
        readingProvider = true
        return try { read() } finally { readingProvider = previous }
    }

    @Suppress("DEPRECATION")
    private fun projected(v: View, native: AccessibilityNodeInfo, host: Boolean? = null): AccessibilityNodeInfo {
        val result = AccessibilityNodeInfo.obtain(native)
        val isHost = host ?: AccessibilityNodeInfo.obtain(v).let { try { it == native } finally { it.recycle() } }
        if (isHost) place.project(v, result)
        else place.project(v, result, Rect().also(native::getBoundsInScreen), virtual = true)
        return result
    }

    override fun performAccessibilityAction(v: View, action: Int, args: Bundle?) = base.performAccessibilityAction(v, action, args)
    override fun addExtraDataToAccessibilityNodeInfo(v: View, info: AccessibilityNodeInfo, key: String, args: Bundle?) =
        base.addExtraDataToAccessibilityNodeInfo(v, info, key, args)
    override fun dispatchPopulateAccessibilityEvent(v: View, e: AccessibilityEvent) = base.dispatchPopulateAccessibilityEvent(v, e)
    override fun onInitializeAccessibilityEvent(v: View, e: AccessibilityEvent) = base.onInitializeAccessibilityEvent(v, e)
    override fun onPopulateAccessibilityEvent(v: View, e: AccessibilityEvent) = base.onPopulateAccessibilityEvent(v, e)
    override fun onRequestSendAccessibilityEvent(v: ViewGroup, child: View, e: AccessibilityEvent) = base.onRequestSendAccessibilityEvent(v, child, e)
    override fun sendAccessibilityEvent(v: View, type: Int) = base.sendAccessibilityEvent(v, type)
    override fun sendAccessibilityEventUnchecked(v: View, e: AccessibilityEvent) = base.sendAccessibilityEventUnchecked(v, e)
}
