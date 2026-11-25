package dev.exact.canvas

import android.app.Activity
import android.content.Context
import android.graphics.Rect
import android.graphics.RenderNode
import android.view.View
import android.view.accessibility.AccessibilityManager
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityNodeProvider
import android.widget.Button
import android.widget.FrameLayout
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativeBoundsTest {
    private lateinit var layer: Natives
    private val graph = PaintedOrigins()
    private val groups = mutableListOf<FrameLayout>()
    private val at = IntArray(2)
    private var inset = false
    private var onUpdate: (View) -> Unit = {}
    private val original = object : View.AccessibilityDelegate() {
        override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
            super.onInitializeAccessibilityNodeInfo(host, info)
            info.hintText = "Native hint"
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CONTEXT_CLICK)
            if (inset) {
                val xy = IntArray(2).also(host::getLocationOnScreen)
                info.setBoundsInScreen(Rect(xy[0] + 4, xy[1] + 1, xy[0] + 14, xy[1] + 9))
            }
        }
    }

    @Before fun host() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        shadowOf(activity.getSystemService(AccessibilityManager::class.java)).setEnabled(true)
        fun group(context: Context) = FrameLayout(context).apply {
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
            addView(Button(context).apply { accessibilityDelegate = original }, FrameLayout.LayoutParams(40, 20).apply { leftMargin = 8; topMargin = 5 })
            groups.add(this)
        }
        val module = object : NativeModule {
            override fun create(context: Context, props: JSONObject): View = group(context)
            override fun update(view: View, props: JSONObject) = onUpdate(view)
        }
        layer = Natives(activity, { File(".") }, mapOf("probe" to module))
        layer.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
        layer.hatched = { _, ctx -> group(ctx) }
        activity.setContentView(layer)
        layer.origins = graph
        layer.accessible(true)
        layer.ensure(2, 2, 90, 50, """{"module":"probe","props":"{}"}""", 0)
        layer.ensure(5, 5, 90, 50, "{}", 0)
        val decor = activity.window.decorView
        val exact = View.MeasureSpec.EXACTLY
        decor.measure(View.MeasureSpec.makeMeasureSpec(400, exact), View.MeasureSpec.makeMeasureSpec(600, exact))
        decor.layout(0, 0, 400, 600)
        layer.getLocationOnScreen(at)
    }

    @Suppress("DEPRECATION")
    @Test fun moduleAndHatchSubtreesFollowMoveAndScroll() {
        val scroller = RenderNode("scroll").apply { setPosition(0, 0, 300, 400) }
        graph.record(null, listOf(PaintedOrigins.Placement(scroller, 0f, 0f)))
        val window = IntArray(2).also(layer::getLocationInWindow)
        fun check(x: Int, y: Int, scroll: Int) {
            graph.record(scroller, listOf(2, 5).map { PaintedOrigins.Placement(layer.node(it)!!, x.toFloat(), y.toFloat()) })
            graph.translate(scroller, 0f, -scroll.toFloat())
            layer.viewTreeObserver.dispatchOnPreDraw()
            for (root in groups) {
                val top = y - scroll
                val parentBox = Rect(x, top, x + 90, top + 50)
                fun rect(info: AccessibilityNodeInfo) = Rect().also(info::getBoundsInScreen)
                val info = root.createAccessibilityNodeInfo()
                assertEquals(parentBox, Rect().also(info::getBoundsInParent))
                assertEquals(Rect(parentBox).apply { offset(window[0], window[1]) }, Rect().also(info::getBoundsInWindow))
                parentBox.offset(at[0], at[1])
                assertEquals(parentBox, rect(info))
                val child = root.getChildAt(0).createAccessibilityNodeInfo()
                assertEquals("Native hint", child.hintText)
                assertTrue(child.actionList.contains(AccessibilityNodeInfo.AccessibilityAction.ACTION_CONTEXT_CLICK))
                assertEquals(Rect(8, 5, 48, 25), Rect().also(child::getBoundsInParent))
                assertEquals(Rect(parentBox.left + 8, parentBox.top + 5, parentBox.left + 48, parentBox.top + 25), rect(child))
                assertEquals(0, root.left); assertEquals(0, root.top)
                assertEquals(0f, root.translationY, 0f)
            }
        }
        check(30, 140, 0)
        check(30, 140, 64)
        check(60, 170, 64)
    }

    private fun placeModule(x: Float, y: Float) {
        graph.record(null, listOf(PaintedOrigins.Placement(layer.node(2)!!, x, y)))
        layer.viewTreeObserver.dispatchOnPreDraw()
    }

    @Test fun nativeClipsFocusBoundsAndLayerScaleSurviveProjection() {
        placeModule(60f, 106f)
        val root = groups.first()
        val child = root.getChildAt(0)
        // The native group's own clip narrows the button even when its painted recording is fully visible.
        root.setPadding(20, 0, 0, 0)
        root.clipToPadding = true
        assertEquals(Rect(at[0] + 80, at[1] + 111, at[0] + 108, at[1] + 131),
            Rect().also(child.createAccessibilityNodeInfo()::getBoundsInScreen))
        root.setPadding(0, 0, 0, 0)
        inset = true
        val local = Rect(72, 112, 82, 120)
        assertEquals(Rect(local).apply { offset(at[0], at[1]) }, Rect().also(child.createAccessibilityNodeInfo()::getBoundsInScreen))
        layer.pivotX = 0f; layer.pivotY = 0f; layer.scaleX = 2f
        // The delegate supplies a ten-pixel-wide focus box in screen coordinates even under a scaled layer.
        assertEquals(Rect(at[0] + 140, at[1] + 112, at[0] + 150, at[1] + 120),
            Rect().also(child.createAccessibilityNodeInfo()::getBoundsInScreen))
        inset = false
        assertEquals(Rect(at[0] + 136, at[1] + 111, at[0] + 216, at[1] + 131),
            Rect().also(child.createAccessibilityNodeInfo()::getBoundsInScreen))
        layer.accessible(false)
        assertSame(original, child.accessibilityDelegate)
        assertNull(root.accessibilityDelegate)
    }

    @Test fun aHiddenHatchAndAChildScrolledIntoTheWindowReportVisibility() {
        val child = groups.first().getChildAt(0)
        graph.record(null, listOf(PaintedOrigins.Placement(layer.node(5)!!, 90f, 80f)))
        layer.viewTreeObserver.dispatchOnPreDraw()
        assertTrue(groups.last().getChildAt(0).createAccessibilityNodeInfo().isVisibleToUser)
        layer.ensure(5, 5, 0, 0, "{}", 0, hidden = true)
        layer.viewTreeObserver.dispatchOnPreDraw()
        val hidden = groups.last().getChildAt(0).createAccessibilityNodeInfo()
        assertFalse(hidden.isVisibleToUser)
        assertTrue(Rect().also(hidden::getBoundsInScreen).isEmpty)

        // An offset in the recording brings a child into the window that Android's physical layout clips out.
        child.layoutParams = (child.layoutParams as FrameLayout.LayoutParams).apply { topMargin = 900 }
        layer.ensure(2, 2, 90, 1200, """{"module":"probe","props":"{}"}""", 0)
        placeModule(20f, -700f)
        assertFalse(child.getGlobalVisibleRect(Rect()))
        val visible = child.createAccessibilityNodeInfo()
        assertEquals(Rect(at[0] + 28, at[1] + 200, at[0] + 68, at[1] + 220), Rect().also(visible::getBoundsInScreen))
        assertTrue(visible.isVisibleToUser)
    }

    @Test fun aModuleMayReplaceItsDelegateDuringAnUpdate() {
        val group = groups.first()
        group.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        placeModule(30f, 60f)
        onUpdate = { v ->
            v.accessibilityDelegate = object : View.AccessibilityDelegate() {
                override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
                    super.onInitializeAccessibilityNodeInfo(host, info)
                    info.hintText = "Updated hint"
                }
            }
        }
        layer.ensure(2, 2, 90, 50, """{"module":"probe","props":"{\"changed\":true}"}""", 0)
        val info = group.createAccessibilityNodeInfo()
        assertEquals("Updated hint", info.hintText)
        assertEquals(Rect(at[0] + 30, at[1] + 60, at[0] + 120, at[1] + 110), Rect().also(info::getBoundsInScreen))
        assertFalse(group.isImportantForAccessibility)
    }

    @Suppress("DEPRECATION")
    @Test fun aProviderCanReturnTheSameVirtualChildForMoreThanOneQuery() {
        val v = groups.first().getChildAt(0)
        // Use Android's node identity. The default Robolectric shadow compares only the hosting View.
        AccessibilityNodeInfo.obtain(v).setQueryFromAppProcessEnabled(v, true)
        val origin = IntArray(2).also(v::getLocationOnScreen)
        val source = AccessibilityNodeInfo.obtain(v, 7).apply {
            setBoundsInParent(Rect(3, 2, 13, 8))
            setBoundsInScreen(Rect(origin[0] + 3, origin[1] + 2, origin[0] + 13, origin[1] + 8))
        }
        val provider = object : AccessibilityNodeProvider() {
            override fun createAccessibilityNodeInfo(id: Int) = source
            override fun findFocus(focus: Int) = source
            override fun findAccessibilityNodeInfosByText(text: String, id: Int) = listOf(source)
        }
        v.accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun getAccessibilityNodeProvider(host: View) = provider
        }
        placeModule(30f, 60f)
        val projected = v.accessibilityNodeProvider!!
        assertSame(projected, v.accessibilityNodeProvider)
        val box = Rect(at[0] + 41, at[1] + 67, at[0] + 51, at[1] + 73)
        for (info in listOf(projected.createAccessibilityNodeInfo(7)!!, projected.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY)!!,
                projected.findAccessibilityNodeInfosByText("child", 7)!!.single())) {
            assertEquals(box, Rect().also(info::getBoundsInScreen))
            assertEquals(Rect(3, 2, 13, 8), Rect().also(info::getBoundsInParent))
        }
        val real = Rect(origin[0] + 3, origin[1] + 2, origin[0] + 13, origin[1] + 8)
        assertEquals(real, Rect().also(source::getBoundsInScreen))
        // An accepted down translates the physical root. Its provider now reports from that physical position.
        groups.first().translationX = 30f; groups.first().translationY = 60f
        real.offset(30, 60)
        source.setBoundsInScreen(real)
        assertEquals(box, Rect().also(projected.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY)!!::getBoundsInScreen))
    }

    @Suppress("DEPRECATION")
    @Test fun aViewsOwnProviderSurvivesBinding() {
        val root = groups.first()
        val source = object : AccessibilityNodeProvider() {
            override fun createAccessibilityNodeInfo(id: Int) = AccessibilityNodeInfo.obtain().apply { text = "Native item" }
        }
        val direct = object : View(root.context) {
            override fun getAccessibilityNodeProvider() = source
        }
        val cooperative = object : View(root.context) {
            override fun getAccessibilityNodeProvider() = super.getAccessibilityNodeProvider() ?: source
        }
        for (v in listOf(direct, cooperative)) root.addView(v, FrameLayout.LayoutParams(20, 20))
        placeModule(30f, 60f)
        layer.viewTreeObserver.dispatchOnGlobalLayout()
        assertSame(source, direct.accessibilityNodeProvider)
        assertNotSame(source, cooperative.accessibilityNodeProvider)
        for (v in listOf(direct, cooperative)) assertEquals("Native item", v.accessibilityNodeProvider!!.createAccessibilityNodeInfo(7)!!.text)
    }

    @Test fun partialPublicationKeepsTheLastDisplayedBoxUntilTheMoveCommits() {
        val node = groups.first()
        fun box() = Rect().also(node.createAccessibilityNodeInfo()::getBoundsInScreen)
        placeModule(30f, 60f)
        val first = box()
        graph.publishing = true
        placeModule(44f, 90f)
        assertEquals(first, box())
        graph.publishing = false
        layer.viewTreeObserver.dispatchOnPreDraw()
        assertEquals(Rect(first).apply { offset(14, 30) }, box())
        graph.translate(layer.node(2)!!, 0f, -10f)
        layer.viewTreeObserver.dispatchOnPreDraw()
        assertEquals(Rect(first).apply { offset(14, 20) }, box())
    }
}
