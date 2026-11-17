package dev.exact.canvas

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.RenderNode
import android.net.Uri
import android.view.Choreographer
import android.view.KeyEvent
import android.view.SurfaceView
import android.view.SurfaceHolder
import android.view.TextureView
import android.view.View
import android.view.ViewGroup
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import org.json.JSONObject
import java.io.File
import kotlin.math.roundToInt

/**
 * A native module's view (LLP 1024): the app registers one per module name. [reset] readies a released view for
 * another element (LLP 1068 §4.8 reuse); a module that returns false from it is destroyed instead.
 */
interface NativeModule {
    fun create(context: Context, props: JSONObject): View
    /** [create] for element [id], the node's id: a package's view is made with it (LLP 1133.008 D8 item 1). */
    fun create(context: Context, props: JSONObject, id: Int): View = create(context, props)
    fun update(view: View, props: JSONObject)
    /** Bound before the first update. Keep this callback only until the next binding; a retired one is ignored. */
    fun events(view: View, emit: (String, String) -> Unit) {}
    /** Activity callbacks; CREATE receives saved state, SAVE_INSTANCE_STATE fills its private bundle (LLP 1133.021). */
    fun lifecycle(view: View, event: ViewLifecycle, state: android.os.Bundle? = null) {}
    fun reset(view: View): Boolean = false
    fun destroy(view: View) {}
}

/** The app's optional module roster, compiled from `modules/android` as `class exact.app.Modules`. */
interface NativeModules {
    val modules: Map<String, NativeModule>

    companion object {
        /** Resolve the app's roster at its first module view, so apps without one load nothing. */
        fun load(): Map<String, NativeModule> = object : AbstractMap<String, NativeModule>() {
            override val entries: Set<Map.Entry<String, NativeModule>> by lazy {
                val type = try { Class.forName("exact.app.Modules") } catch (e: ClassNotFoundException) { return@lazy emptySet() }
                try {
                    (type.getDeclaredConstructor().newInstance() as NativeModules).modules.entries
                } catch (e: Exception) {
                    val cause = (e as? java.lang.reflect.InvocationTargetException)?.targetException ?: e
                    android.util.Log.e("exact", "native modules: exact.app.Modules could not be made: $cause")
                    emptySet()
                }
            }
        }
    }
}

/**
 * A `video` or `audio` element's player (LLP 1042.001 D3). An APK whose plan has one links Media3's
 * ([ExactActivity.video]); the reader itself names no player, so an APK without one carries none of its code. A
 * player is lent to a node's [MediaRecord]: the record outlives it and is where it starts.
 */
interface VideoPlayer {
    /** The view the reader hosts and records for the element's box; made once, kept with the player. */
    val view: View
    /** Whether a parked row's player may go to the spares: only an inline picture with no controller. */
    val lendable: Boolean
    /** Playing on its own account, not under Chrome's off-screen rule: it keeps its player out of view (D1). */
    val engaged: Boolean
    /** The controller's view with `controls` (Media3's `PlayerView`), which reports itself to accessibility services
     * at the node (D11); null without. */
    val controller: View?
    /** A loan begins: this player reports to [sink] until [release] or the next bind. */
    fun bind(sink: MediaSink)
    /** Every prop of the element as the painter names it, with `uri` (the resolved source) or `refused` (why it
     * was refused) beside `src`, and the content box in device pixels. [from]: the record a loan starts from, given
     * with the loan's first update. */
    fun update(props: JSONObject, width: Int, height: Int, from: MediaRecord.Resume?)
    /** Whether a row of the node is in view, for Chrome's rule for a muted autoplaying video (LLP 1042 §7 B). */
    fun shown(inView: Boolean)
    /** What the record keeps of this loan. */
    fun snapshot(): MediaRecord.Resume?
    /** The activity stopped or the row was parked: pause as the stop does (D8) and give up the surface. */
    fun stop()
    /** The loan ends: no report follows. */
    fun release()
}

/** Where a media element's player reports (LLP 1042.001 D3, D7): to exact2, keyed by the element's node. */
interface MediaSink {
    /** An event by HTML's name, with its payload ("" for none). */
    fun event(name: String, payload: String)
    /** The element's observed state, a JSON object: its row of `state.media`. */
    fun state(json: String)
    /** The current source's first frame is in the view. */
    fun firstFrame()
}

/**
 * A media node's record (LLP 1042.001 D1), kept across the views and players it borrows, a parked row and a reader
 * made again: the authority a new player starts from.
 */
class MediaRecord internal constructor(val id: Int, internal val key: Long) {
    /**
     * Where a loan starts: [src] at [positionMs]; HTML's paused flag; whether it ended; [resumes] when Chrome's
     * off-screen rule paused it; the speed and [choices] (the player's own) the controls made; and [applied], the
     * props last applied, so that only a prop that differs from them counts as assigned.
     */
    data class Resume(
        val src: String, val positionMs: Long, val paused: Boolean, val ended: Boolean, val resumes: Boolean,
        val speed: Float, val choices: Any?, val applied: String,
    )
    var resume: Resume? = null
        internal set
}

/** Media records by node id, the process's: they outlive a reader made again ([ExactActivity.read]). */
internal object MediaRecords {
    private val records = HashMap<Int, MediaRecord>()
    /** The record for node [id] at allocation [key] (session, generation and slot folded): a new one where the id
     * names another allocation now. */
    fun of(id: Int, key: Long): MediaRecord =
        records[id]?.takeIf { it.key == key } ?: MediaRecord(id, key).also { records[id] = it }
    /** The node is gone. */
    fun forget(id: Int) { records.remove(id) }
}

/**
 * The platform elements of the stream (op 28: `video`, `iframe`, a native module's view). Each is a real Android view,
 * attached here at its device size so it runs as it would anywhere (a player's TextureView, a WebView, a MapLibre
 * map); this layout draws none of them. Each view's own drawing is recorded into its element's RenderNode, which the
 * rows draw where the element is: it scrolls, clips and stacks with the content around it. A view that invalidates
 * (a video frame, a page, map tiles) has only its node recorded again, at the next frame.
 */
class Natives(
    context: Context,
    private val root: () -> File,
    private val modules: Map<String, NativeModule>,
    private val videos: ((Context) -> VideoPlayer)? = null,
) : FrameLayout(context) {
    internal var keeps = false
        private set
    /** Where a video's or a page's path is a file ([AssetRoot]). */
    private val assets: File get() = root()

    /** The running generation's assets moved (a signed update activated, LLP 1030.006 D1): every video and page is
     * opened again, so one at an unchanged path shows the new generation's file. */
    fun reroot() {
        for (e in elements.values) {
            if (e.props.isEmpty()) continue
            when (e.kind) {
                0 -> { sources.remove(e.view); apply(e, e.props) }
                1 -> { val props = e.props; e.props = ""; apply(e, props) }
            }
        }
        if (refresh()) frame()
    }

    override fun requestDisallowInterceptTouchEvent(disallow: Boolean) {
        keeps = disallow
        super.requestDisallowInterceptTouchEvent(disallow)
    }

    private class Element(val id: Int, val kind: Int, var view: View, val node: RenderNode) {
        var props = ""
        var module: String? = null
        var generation = 0
        var session = 0L
        var slot = 0
        var disabled = false
        var eventBinding = 0
        /** A media element's record ([MediaRecords]); whether a loan has started from it, and the loan's number, so a
         * report from an ended loan is dropped (LLP 1042.001 D3). */
        var record: MediaRecord? = null
        var started = false
        var loan = 0
        /** A video whose view still shows another source's last frame (a lent player's): drawn once its own first
         * frame is in the view, as a new TextureView draws nothing until then. */
        var stale = false
        /** Whether a video's player was last told it is in view, and whether a row of it is. */
        var playing = false
        var inView = false
        /** A page paused because its row left the view ([near]). */
        var paused = false
        var w = 0
        var h = 0
        var dirty = true
        /** A GPU canvas's window, given to the host while its TextureView has a surface. */
        var surface: android.view.Surface? = null
        /** Written on the exact thread when [Exact.attachSurface] ran there; read there by [Exact.detachSurface]. */
        @Volatile var window = 0L
        /** Rows (and 0, the unrowed stream) that draw it. */
        val rows = HashSet<Int>()
        var fullWindow = false
        val presentation = CanvasPresentation()
        var copying = false
        var snapshotReady = false
        var last: android.graphics.Bitmap? = null
        val redraws = ArrayList<Runnable>()
        /** A GPU canvas drawn from its own buffers ([buffered]): the size they were asked at, the frame last drawn
         * ([Native.canvasFrame]) and each buffer of the ring wrapped once, by generation and slot. */
        var buffers = false
        var asked = 0L
        var drawn = -1L
        val wrapped = HashMap<Long, android.graphics.Bitmap>()
        /** The frame time, in milliseconds, after which its cap leaves the window's vote unless it draws again
         * ([vote]). */
        var rests = 0L
        /** Without its view ([near]): [view] is the stand-in, and [wanted] the props its view starts from. */
        var parked = false
        var wanted = ""
        /** A row of the reader's placing has drawn it: when none of its rows is placed any more, they have left. */
        var placed = false
    }

    /** Opaque full-window canvases present beneath the UI through SurfaceView.
     * gpubuf=1 forces buffers; gpubuf=0 compares the TextureView window path. */
    private val presentation = (context as? android.app.Activity)?.intent?.getStringExtra("gpubuf")
    private val windowCanvas = presentation == null || presentation == "auto" || presentation == "surface"
    private val buffered = (context as? android.app.Activity)?.intent?.getStringExtra("gpubuf") != "0"
    /** Buffers were refused (an older host library, or a device without them): canvases get windows. */
    private var refused = false
    private var polling = false
    private val src = android.graphics.Rect()
    private val dst = android.graphics.Rect()
    private val bitmapPaint = android.graphics.Paint(android.graphics.Paint.FILTER_BITMAP_FLAG)
    private val srgb = android.graphics.ColorSpace.get(android.graphics.ColorSpace.Named.SRGB)

    /** Each display frame while a buffered canvas lives: draw the ones with a new frame. */
    private fun poll() {
        if (polling) return
        polling = true
        Choreographer.getInstance().postFrameCallback { nanos ->
            polling = false
            val now = nanos / 1_000_000
            var any = false
            var capped = false
            var live = false
            val drew = ArrayList<Element>()
            for (e in elements.values.toList()) {
                if (e.view is SurfaceView) {
                    if (e.copying) { live = true; continue }
                    if (!e.presentation.ready) {
                        live = true
                        val ready = e.presentation.observe(Native.canvasReady(e.id), android.os.SystemClock.uptimeMillis())
                        if (ready < 0) {
                            replacePresentation(e, false)
                        } else if (ready > 0) {
                            e.dirty = true
                            record(e); finishRedraws(e); any = true
                        }
                    }
                    continue
                }
                if (!e.buffers) continue
                live = true
                val f = try { Native.canvasFrame(e.id) } catch (x: UnsatisfiedLinkError) { -2L }
                if (f == -2L) { refused = true; unbuffer(e); any = true; continue }
                // None after one was drawn: a new canvas (the host was replaced), whose ring may reuse the old one's
                // numbers. The image drawn stays until its first frame.
                if (f == -1L && e.drawn != -1L) { e.drawn = -1L; e.wrapped.clear() }
                if (f < 0 || f == e.drawn) continue
                val key = f ushr 32
                val bmp = e.wrapped[key] ?: run {
                    // A new ring (a resize): the old one's buffers go.
                    if (e.wrapped.keys.any { it ushr 8 != key ushr 8 }) e.wrapped.clear()
                    val hb = Native.canvasBuffer(e.id) ?: return@run null
                    // The buffer may be newer than [f] by now; it is drawn all the same and named at the next frame.
                    val made = android.graphics.Bitmap.wrapHardwareBuffer(hb, srgb)
                    hb.close()
                    if (made != null && Native.canvasFrame(e.id) ushr 32 == key) e.wrapped[key] = made
                    made
                } ?: continue
                e.drawn = f
                e.presentation.bufferFrame()
                e.last = bmp
                if (e.w <= 0 || e.h <= 0) continue
                if (votes(e)) e.rests = now + (1000 / e.view.requestedFrameRate).toLong() + VOTE_HOLD_MS
                drew.add(e)
            }
            // Recorded now that every canvas with a new frame is known: a capped one's vote counts them all ([vote]).
            for (e in drew) {
                val bmp = e.last!!
                e.node.setPosition(0, 0, e.w, e.h)
                val c = e.node.beginRecording(e.w, e.h)
                src.set(0, 0, bmp.width, bmp.height); dst.set(0, 0, e.w, e.h)
                c.drawBitmap(bmp, src, dst, bitmapPaint)
                if (votes(e)) { vote(e, c, now); capped = true } else any = true
                e.node.endRecording()
                if (e.fullWindow && windowCanvas && !e.presentation.failed) presentation(e, true)
            }
            // A frame only capped canvases drew is not asked for through [frame]: the view that draws would vote a
            // category by its size, beside which SurfaceFlinger drops the window's rate on a panel of fixed modes
            // (`FrameRate::isVoteValidForMrr`). The window is told its drawing changed, and records no view for it.
            if (any) frame() else if (capped) parent?.onDescendantInvalidated(this, this)
            if (live) poll()
        }
    }

    /** Ask for (or, at a new size, ask again for) a buffered canvas's buffers. */
    private fun askBuffers(e: Element) {
        if (!e.buffers || e.w <= 0 || e.h <= 0) return
        val size = (e.w.toLong() shl 32) or e.h.toLong()
        if (e.asked == size) return
        e.asked = size
        Exact.attachBuffers(e.id, e.w, e.h)
        poll()
    }

    /** Buffers were refused: the canvas gets a TextureView and its window, as with gpubuf=0. */
    private fun unbuffer(e: Element) {
        e.buffers = false
        e.wrapped.clear()
        byView.remove(e.view)
        removeView(e.view)
        val tv = TextureView(context).apply { isOpaque = false }
        addView(tv, LayoutParams(1, 1))
        e.view = tv
        byView[tv] = e
        listen(e, tv)
        if (e.w > 0 && e.h > 0) {
            tv.layoutParams = LayoutParams(e.w, e.h)
            tv.measure(MeasureSpec.makeMeasureSpec(e.w, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(e.h, MeasureSpec.EXACTLY))
            tv.layout(0, 0, e.w, e.h)
        }
        e.dirty = true
    }

    private fun listen(e: Element, view: TextureView) {
        view.surfaceTextureListener = object : TextureView.SurfaceTextureListener {
            override fun onSurfaceTextureAvailable(st: android.graphics.SurfaceTexture, w: Int, h: Int) = window(e, st, w, h)
            override fun onSurfaceTextureSizeChanged(st: android.graphics.SurfaceTexture, w: Int, h: Int) = window(e, st, w, h)
            override fun onSurfaceTextureDestroyed(st: android.graphics.SurfaceTexture): Boolean { unwindow(e); return true }
            override fun onSurfaceTextureUpdated(st: android.graphics.SurfaceTexture) {
                if (e.view !== view) return
                e.presentation.firstFrame()
                onDescendantInvalidated(view, view)
            }
        }
    }

    internal val lifecycle = HostedLifecycle()
    private val elements = HashMap<Int, Element>()
    private val byView = HashMap<View, Element>()
    private val pool = HashMap<String, ArrayList<View>>()
    /** A released row's WebView, kept for the next (as a RecyclerView keeps its holders' views): it shows its last page
     * until the new one paints, where a new WebView is white for that long and costs its construction on this thread
     * (webpool=0: one made for each, destroyed with it). */
    private val webPool = ArrayList<WebView>()
    /** modpool=N: a module's views kept for reuse ([MODULE_POOL] by default), to compare. */
    private val modulePool = (context as? android.app.Activity)?.intent?.getStringExtra("modpool")?.toIntOrNull() ?: MODULE_POOL
    private val webs = (context as? android.app.Activity)?.intent?.getStringExtra("webpool") != "0"
    private var scheduled = false
    /** Asked to draw a frame after nodes were recorded again (no new stream). */
    var frame: () -> Unit = {}

    init {
        clipChildren = false
        // Laid out, attached and "visible" to themselves, but drawn only through their nodes.
        setWillNotDraw(true)
    }

    /**
     * Whether [e] is a canvas drawn here from its buffers that asks for a frame rate. A view's requested rate reaches
     * its window only when the view is recorded after it was invalidated
     * ([View.setRequestedFrameRate](https://developer.android.com/reference/android/view/View#setRequestedFrameRate(float))),
     * and this canvas's view draws nothing, so [poll] has it vote with each frame ([vote]). The window keeps the rate
     * between those frames: `ViewRootImpl` drops it at a frame drawn with no such vote more than 100 ms after the
     * last, which a cap under 10 fps meets whenever the window draws anything else.
     */
    private fun votes(e: Element) = e.buffers && android.os.Build.VERSION.SDK_INT >= 35 && e.view.requestedFrameRate > 0f

    /**
     * Casts the window's rate with a frame of [e], whose node [c] records, at frame time [now]: [e]'s view,
     * invalidated, is recorded, empty, beside the frame. The window has one rate, so the vote is the highest cap of
     * the canvases drawn here from their buffers that are still drawing, whichever of them drew: given two rates in
     * one frame of which neither divides the other, `ViewRootImpl.votePreferredFrameRate` sends the window's layer
     * neither (Android 15 and 16). A canvas is still drawing until an interval of its cap and [VOTE_HOLD_MS] pass
     * with no new frame of it, so one that rests stops raising the others' vote about when the view root would have
     * dropped its rate had it drawn alone. Each canvas still paces its own cap.
     */
    private fun vote(e: Element, c: Canvas, now: Long) {
        val own = e.view.requestedFrameRate
        e.view.requestedFrameRate =
            elements.values.maxOf { if (votes(it) && now <= it.rests) it.view.requestedFrameRate else 0f }
        e.view.invalidate(); drawChild(c, e.view, drawingTime)
        e.view.requestedFrameRate = own
    }

    /** Moving a controller to where it is drawn ([placeControls]): not a change of its drawing. */
    private var placing = false
    private val placeMatrix = android.graphics.Matrix()
    private val placePoint = FloatArray(2)
    private val placer = android.view.ViewTreeObserver.OnPreDrawListener { placeControls(); true }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnPreDrawListener(placer)
    }

    override fun onDetachedFromWindow() {
        viewTreeObserver.removeOnPreDrawListener(placer)
        super.onDetachedFromWindow()
    }

    /**
     * A media controller's view is kept where its node is drawn, at each frame, as a text field's is (LLP 1042.001
     * D4): the layer draws none of it, but its real bounds are where it is seen, so an accessibility service, its
     * popups' anchor and its focus find it there. A controller none of whose rows is drawn is moved off the window
     * and hidden from accessibility.
     */
    private fun placeControls() {
        placing = true
        try {
            for (e in elements.values) {
                if (e.kind != 0 || e.parked || players[e.view]?.lendable != false) continue
                val drawn = origins?.transform(e.node, placeMatrix) == true
                val x: Float; val y: Float
                if (drawn) {
                    placePoint[0] = 0f; placePoint[1] = 0f
                    placeMatrix.mapPoints(placePoint)
                    x = placePoint[0].roundToInt().toFloat(); y = placePoint[1].roundToInt().toFloat()
                } else { x = -(e.w + 100000f); y = 0f }
                if (e.view.translationX != x) e.view.translationX = x
                if (e.view.translationY != y) e.view.translationY = y
                val access = if (drawn) IMPORTANT_FOR_ACCESSIBILITY_AUTO else IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
                if (e.view.importantForAccessibility != access) e.view.importantForAccessibility = access
            }
        } finally { placing = false }
    }

    override fun dispatchDraw(canvas: Canvas) {
        for (e in elements.values) if (e.view is SurfaceView) drawChild(canvas, e.view, drawingTime)
    }

    override fun onDescendantInvalidated(child: View, target: View) {
        if (placing) return
        if (child is SurfaceView) super.onDescendantInvalidated(child, target)
        val e = byView[child] ?: return
        e.dirty = true
        if (!scheduled) {
            scheduled = true
            Choreographer.getInstance().postFrameCallback {
                scheduled = false
                if (refresh()) frame()
            }
        }
    }

    /** Record every changed element's view again; whether any was. */
    fun refresh(): Boolean {
        var any = false
        for (e in elements.values) if (e.dirty && !e.buffers) { record(e); any = true }
        return any
    }

    private fun record(e: Element) {
        e.dirty = false
        if (e.w <= 0 || e.h <= 0) return
        e.node.setPosition(0, 0, e.w, e.h)
        val c = e.node.beginRecording(e.w, e.h)
        if (e.kind == 3 && !e.presentation.ready && e.last != null) {
            if (e.view is TextureView) {
                // TextureView latches its first image during draw; its update callback can run here.
                val save = c.saveLayerAlpha(0f, 0f, e.w.toFloat(), e.h.toFloat(), 0)
                e.view.draw(c)
                c.restoreToCount(save)
            }
            val bmp = e.last!!
            src.set(0, 0, bmp.width, bmp.height); dst.set(0, 0, e.w, e.h)
            c.drawBitmap(bmp, src, dst, bitmapPaint)
        } else if (e.view is SurfaceView && e.presentation.ready) c.drawColor(Color.TRANSPARENT, android.graphics.PorterDuff.Mode.CLEAR)
        else if (!e.stale) e.view.draw(c)
        e.node.endRecording()
    }

    /** The node id of the element whose view is [view] or holds it ([ExactForma]'s place), or null (main thread). */
    fun place(view: View): Int? {
        var v: View? = view
        while (v != null && v !== this) {
            byView[v]?.let { return it.id }
            v = v.parent as? View
        }
        return null
    }

    /** The controller media node [id]'s player hosts, which stands at the node in the accessibility tree (LLP
     * 1042.001 D11). */
    fun controller(id: Int): View? = elements[id]?.takeIf { it.kind == 0 && !it.parked }?.let { players[it.view]?.controller }

    /** The reader's own view, which takes the window's focus back from a media controller (LLP 1042.001 D4). */
    var reader: View? = null

    /**
     * A key that the view holding the focus did not take and that would move the focus out of a media controller
     * (Tab, Shift+Tab, an arrow at its edge) is exact2's: the reader's view takes the focus back and hears the key, so
     * exact2 moves its own focus on from the video (LLP 1042.001 D4). A key the reader does not take either (a
     * phone's arrows: only a TV's remote moves exact2's focus by them) leaves the focus in the controller.
     */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (super.dispatchKeyEvent(event)) return true
        if (event.action != KeyEvent.ACTION_DOWN) return false
        val direction = direction(event) ?: return false
        val focused = findFocus() ?: return false
        val e = place(focused)?.let { elements[it] }?.takeIf { it.kind == 0 } ?: return false
        var next = focused.focusSearch(direction)
        while (next != null && next !== e.view) next = next.parent as? View
        if (next != null) return false
        val reader = reader?.takeIf { it.requestFocus() } ?: return false
        if (reader.dispatchKeyEvent(event)) return true
        focused.requestFocus()
        return true
    }

    /** The way [event] moves the focus, as the window's own navigation reads it, or null. */
    private fun direction(event: KeyEvent): Int? = when (event.keyCode) {
        KeyEvent.KEYCODE_TAB -> when {
            event.isCtrlPressed || event.isAltPressed || event.isMetaPressed -> null
            event.isShiftPressed -> View.FOCUS_BACKWARD
            else -> View.FOCUS_FORWARD
        }
        KeyEvent.KEYCODE_DPAD_UP -> View.FOCUS_UP
        KeyEvent.KEYCODE_DPAD_DOWN -> View.FOCUS_DOWN
        KeyEvent.KEYCODE_DPAD_LEFT -> View.FOCUS_LEFT
        KeyEvent.KEYCODE_DPAD_RIGHT -> View.FOCUS_RIGHT
        else -> null
    }

    /** A controller reports itself only where its node stands ([Access]), not again among the layer's views. */
    override fun onInitializeAccessibilityNodeInfo(info: android.view.accessibility.AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        for (p in players.values) p.controller?.let { info.removeChild(it) }
    }

    /** The node op 28 draws for [id] (main thread). */
    fun node(id: Int): RenderNode? = elements[id]?.node

    internal var origins: PaintedOrigins? = null
    internal fun transform(view: View, matrix: android.graphics.Matrix): Boolean =
        byView[view]?.let { origins?.transform(it.node, matrix) } == true

    /** A view the app's Kotlin hatches host at a node (kind 5, LLP 1115.001.000 D4), made for this layer's
     * activity: each element its own. */
    var hatched: ((Int, Context) -> View?)? = null

    /** Every element's node, for a row recorded on another thread: one map shared until an element comes or goes. */
    private var nodes: Map<Int, RenderNode>? = null
    fun snapshot(): Map<Int, RenderNode> = nodes ?: elements.mapValues { it.value.node }.also { nodes = it }

    /** Op 28 seen while ingesting a stream: make or update the element; [row] draws it (0: not in a row). */
    fun ensure(id: Int, kind: Int, w: Int, h: Int, props: String, row: Int, fullWindow: Boolean = false) {
        // A top-level WebView cannot enforce an iframe's sandbox (LLP 1020 D2).
        if (kind == 1 && !JSONObject(props).isNull("sandbox")) {
            elements[id]?.let { release(it, reuse = false) }
            android.util.Log.w("exact", "iframe $id: sandbox is unsupported on Android")
            return
        }
        val json = if (kind == 2 || kind == 0) JSONObject(props) else null
        val module = if (kind == 2) json?.optString("module") else null
        val generation = json?.optInt("generation") ?: 0
        val slot = json?.optInt("slot") ?: 0
        val session = json?.optLong("session") ?: 0L
        var e = elements[id]
        if (e == null || e.kind != kind || ((kind == 2 || kind == 0) && (e.generation != generation || e.slot != slot || e.module != module || e.session != session))) {
            e?.let { release(it) }
            val created = if (lazy && row != 0 && ((kind == 0 && videos != null && lazyVideo) || kind == 1 || (kind == 2 && modules.containsKey(module)))) {
                Element(id, kind, standIn, RenderNode("exact native")).also {
                    it.parked = true; it.module = module
                    it.generation = generation; it.slot = slot; it.session = session
                    waiting++
                }
            } else create(id, kind, props) ?: return
            if (kind == 0) { created.generation = generation; created.slot = slot; created.session = session }
            e = created
            elements[id] = e
            nodes = null
            if (!e.parked) byView[e.view] = e
        }
        e.fullWindow = fullWindow && row == 0
        if (!e.fullWindow) e.presentation.failed = false
        if (kind == 3) {
            presentation(e, e.fullWindow && windowCanvas)
            if (e.view is SurfaceView && e.presentation.replaced(Native.canvasReady(id))) { e.dirty = true; poll() }
        }
        if (json != null && kind == 2) e.disabled = json.optBoolean("disabled")
        e.rows.add(row)
        e.wanted = props
        if (e.w != w || e.h != h) {
            e.w = w; e.h = h
            if (!e.parked) { place(e); askBuffers(e) }
            // A player sizes its picture, its fit and its rendition cap by the box (LLP 1042.001 D6).
            if (kind == 0 && !e.parked && e.props == props) { e.props = ""; apply(e, props); return }
        }
        if (e.parked && row == 0) unpark(e)
        if (e.parked) return
        if (e.props != props) apply(e, props)
        if (e.dirty && !e.buffers) record(e)
    }

    private fun place(e: Element) {
        e.view.layoutParams = LayoutParams(e.w, e.h)
        e.view.measure(MeasureSpec.makeMeasureSpec(e.w, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(e.h, MeasureSpec.EXACTLY))
        e.view.layout(0, 0, e.w, e.h)
        e.dirty = true
    }

    /**
     * A row's WebView or module view exists while the row is near the view, as a RecyclerView's holder is bound when
     * it scrolls near (lazy=0: for every row the stream keeps, which is several screens of them; a MapLibre map is
     * ~110 MB on a Pixel 10 Pro XL, and a feed of maps kept fifteen). Elsewhere the element is its node with no
     * drawing. Set by the reader once it tells where the rows are ([near]); an element outside a row always has
     * its view.
     */
    var lazy = false
    /** What a parked element holds for a view: never attached, never drawn. */
    private val standIn = View(context)
    /** Parked elements, and those that can be: whether [near] has anything to do. */
    private var waiting = 0
    private var held = 0
    val nearing: Boolean get() = waiting > 0 || held > 0

    /**
     * Where the rows are: [show] within the margin that gives a row's element its view, [keep] within the wider one
     * that lets it keep one, [known] every row the reader placed. An element none of whose rows it ever placed (a
     * nested list's) has its view; one whose rows it placed and no longer does has left. A view let go is lent on,
     * first to an element arriving in this call. [seen]: the rows in the view itself.
     */
    fun near(show: Set<Int>, keep: Set<Int>, known: Set<Int>, seen: Set<Int> = show) {
        for (e in elements.values) if (!e.placed && e.rows.any { known.contains(it) }) e.placed = true
        if (held > 0) for (e in elements.values) if (far(e, keep)) park(e)
        if (waiting > 0) lend(show, seen)
        // A video plays while a row of its is in the view, as a list's player plays the row on screen: a row near
        // but out of view is prepared and paused on its first frame.
        if (lazyVideo) for (e in elements.values) if (e.kind == 0 && !e.parked) {
            play(e, !e.placed || e.rows.contains(0) || e.rows.any { seen.contains(it) })
        }
        // A page runs (its animations, its timers' drawing) while a row of its is in the view, and from when it is
        // made until then, so it loads and paints ahead; once its row has left the view it is paused, as a page in
        // a list is not drawn while its holder is off screen.
        if (webPause) for (e in elements.values) if (e.kind == 1 && !e.parked) {
            val inView = !e.placed || e.rows.contains(0) || e.rows.any { seen.contains(it) }
            if (inView == e.inView) continue
            e.inView = inView
            if (inView) { if (e.paused) { (e.view as WebView).onResume(); e.paused = false } }
            else { (e.view as WebView).onPause(); e.paused = true }
        }
    }

    /** webpause=0: a page runs for as long as its row has its view, to compare. */
    private val webPause = (context as? android.app.Activity)?.intent?.getStringExtra("webpause") != "0"

    private fun lend(show: Set<Int>, seen: Set<Int>) {
        var any = false
        for (e in elements.values) {
            if (!e.parked || !(!e.placed || e.rows.any { show.contains(it) })) continue
            // None kept to lend: one still held by a row that has left [show] (kept only against coming back) is
            // lent on, before another is made.
            val none = when (e.kind) { 0 -> videoPool.isEmpty(); 1 -> webPool.isEmpty(); else -> pool[e.module]?.isEmpty() != false }
            if (none) {
                // Then one held by a row near but out of view (behind, or ahead of this one): a view is made only
                // when every other is in view, or this row is. Otherwise this one waits for the next call. (A
                // window that leads names rows a viewport ahead while the last still has its view half a viewport
                // behind: two maps for map rows two viewports apart, 110 MB each, and both kept after.)
                val other = { o: Element -> o !== e && !o.parked && o.kind == e.kind && o.module == e.module && o.placed && !o.rows.contains(0) }
                val lend = elements.values.firstOrNull { o -> o.kind == e.kind && o.module == e.module && far(o, show) }
                    ?: elements.values.firstOrNull { o -> other(o) && o.rows.none { seen.contains(it) } }
                if (lend != null) park(lend)
                else if (e.placed && e.rows.none { seen.contains(it) } && elements.values.any(other)) continue
            }
            unpark(e); any = true
        }
        if (any) frame()
    }

    /** The reader cannot tell where rows are: every element has its view. */
    fun everywhere() {
        lazy = false
        for (e in elements.values) if (e.kind == 0 && !e.parked) play(e, true)
        if (waiting == 0) return
        for (e in elements.values) if (e.parked) { unpark(e); if (e.kind == 0) play(e, true) }
        frame()
    }

    /** Whether [e] has a view of its own that none of its rows, all the reader's to place, is within [zone] to use. */
    private fun far(e: Element, zone: Set<Int>): Boolean =
        !e.parked && pooled(e.kind) && e.placed && !e.rows.contains(0) && e.rows.none { zone.contains(it) } &&
            !(e.kind == 0 && players[e.view]?.engaged == true)

    /** The kinds whose views are kept and lent: a video's TextureView with its player, a WebView, a module's view. */
    private fun pooled(kind: Int) = (kind == 0 && lazyVideo) || kind == 1 || kind == 2

    /** lazyvideo=0: a video row has a player of its own for as long as the stream keeps the row, playing whether
     * in view or not, to compare. */
    private val lazyVideo = (context as? android.app.Activity)?.intent?.getStringExtra("lazyvideo") != "0"
    /** A video's player by its view, and the source it was last given: a pair is kept and lent as one. */
    private val players = HashMap<View, VideoPlayer>()
    private val sources = HashMap<View, String>()
    private val videoPool = ArrayList<View>()
    /** Said once: the plan has a `video` this APK links no player for (LLP 1042.001 D2). */
    private var noPlayerSaid = false

    private fun play(e: Element, inView: Boolean) {
        e.inView = inView
        if (inView == e.playing) return
        e.playing = inView
        players[e.view]?.shown(inView)
    }

    /** A loan's reports, to exact2 while the element is current: each made while the loan is open and the element
     * holds its view, and posted, so exact2 never runs on a Media3 listener's stack (LLP 1042.001 D3). A report made
     * as the loan ends (the stop's `pause`) still goes. */
    private fun sink(e: Element, view: View): MediaSink {
        val loan = ++e.loan
        fun open() = elements[e.id] === e && !e.parked && e.view === view && e.loan == loan
        fun send(run: () -> Unit) { if (open()) post { if (elements[e.id] === e) run() } }
        return object : MediaSink {
            override fun event(name: String, payload: String) =
                send { Exact.media(e.id, e.slot, e.generation, e.session, name, payload, "") }
            override fun state(json: String) =
                send { Exact.media(e.id, e.slot, e.generation, e.session, "", "", json) }
            override fun firstFrame() =
                send { if (e.stale) { e.stale = false; e.dirty = true; if (refresh()) frame() } }
        }
    }

    /** A new video view with its player, or a view that draws nothing where the APK links none. */
    private fun videoView(): View {
        val made = videos ?: run {
            if (!noPlayerSaid) { noPlayerSaid = true; android.util.Log.w("exact", "video: this APK links no player; an APK built for a plan with a video links Media3's (LLP 1042.001 D2)") }
            return View(context)
        }
        AudioFocus.application = context.applicationContext
        val player = made(context)
        players[player.view] = player
        return player.view
    }

    /** Keep the open loan's state in [e]'s record. */
    private fun keep(e: Element) {
        val player = players[e.view] ?: return
        if (e.started) player.snapshot()?.let { e.record?.resume = it }
    }

    /** The activity stopped (LLP 1042.001 D8): each player pauses as the stop does, its state goes into its record,
     * and it is released, so no decoder is held in the background. The reader made at the next start gives each
     * record a player again. */
    fun stopMedia() {
        for (e in elements.values) if (e.kind == 0 && !e.parked) {
            val player = players.remove(e.view) ?: continue
            if (e.started) { player.stop(); player.snapshot()?.let { e.record?.resume = it } }
            e.loan++
            player.release()
            e.started = false
        }
        for (v in videoPool) players.remove(v)?.release()
    }

    private fun park(e: Element) {
        byView.remove(e.view)
        retire(e)
        e.view = standIn
        e.inView = false; e.paused = false
        e.parked = true; waiting++
        e.props = ""
        e.node.discardDisplayList()
    }

    private fun unpark(e: Element) {
        val view = obtain(e.kind, JSONObject(e.wanted), e.id) ?: return
        if (view.parent == null) addView(view, LayoutParams(1, 1))
        e.view = view
        e.parked = false; waiting--
        if (pooled(e.kind)) held++
        byView[view] = e
        if (e.kind == 2) bindEvents(e)
        if (e.w > 0 && e.h > 0) place(e)
        apply(e, e.wanted)
        if (e.dirty) record(e)
    }

    /** A row was freed: its elements that no other row draws go. */
    fun rowFreed(row: Int) {
        val gone = elements.values.filter { it.rows.remove(row) && it.rows.isEmpty() }
        for (e in gone) release(e)
    }

    /** The unrowed stream was replaced: elements only it drew and [seen] does not name go. */
    fun unrowed(seen: Set<Int>) {
        val gone = elements.values.filter { it.rows.contains(0) && !seen.contains(it.id) }
        for (e in gone) { e.rows.remove(0); if (e.rows.isEmpty()) release(e) }
    }

    private fun obtain(kind: Int, json: JSONObject, id: Int = 0): View? = when (kind) {
        5 -> hatched?.invoke(id, context)?.also { (it.parent as? android.view.ViewGroup)?.removeView(it) }
        3 -> if (buffered && !refused) View(context) else TextureView(context).apply { isOpaque = false }
        0 -> videoPool.removeLastOrNull() ?: videoView()
        1 -> webPool.removeLastOrNull()?.apply { settings.javaScriptEnabled = true; onResume() } ?: webView()
        else -> {
            val name = json.optString("module")
            val module = modules[name]
            if (module == null) { android.util.Log.w("exact", "no native module $name"); null }
            else {
                val key = "$name:$id:${json.optInt("slot")}:${json.optInt("generation")}"
                val borrowed = if (lifecycle.awaiting(key)) null else pool[name]?.removeLastOrNull()
                borrowed ?: module.create(context, moduleProps(json), id)
            }
        }
    }

    private fun webView(): WebView = WebView(context).apply {
        settings.javaScriptEnabled = true
        webViewClient = object : WebViewClient() {
            // A guest's navigations stay in its frame, as in a browser's; a file only under the asset root.
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
                !Sources.navigable(request.url.scheme, request.url.path, assets)

            // Android ends the app with the renderer unless this answers true.
            override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
                rendererGone(view, detail.didCrash())
                return true
            }
        }
        isVerticalScrollBarEnabled = false; isHorizontalScrollBarEnabled = false
        overScrollMode = View.OVER_SCROLL_NEVER
        setBackgroundColor(Color.WHITE)
    }

    /**
     * [web]'s renderer ended: it crashed, or Android reclaimed it. The view cannot be used again, so it is destroyed
     * (https://developer.android.com/reference/android/webkit/WebViewClient#onRenderProcessGone(android.webkit.WebView,%20android.webkit.RenderProcessGoneDetail)).
     * A frame's page is loaded again from its `src` in a new view, with no event, as Apple's hosts re-serve a frame
     * whose content process ended (LLP 1020 D2). The app's WebViews share a renderer, so a pooled one is told too.
     */
    private fun rendererGone(web: WebView, crashed: Boolean) {
        removeView(web)
        val e = byView.remove(web)?.takeIf { elements[it.id] === it && it.view === web }
        webPool.remove(web)
        web.destroy()
        if (e == null) return
        android.util.Log.w("exact", "iframe ${e.id}: the page's renderer ${if (crashed) "crashed" else "was ended by Android"}; loading its src again")
        val fresh = webView()
        addView(fresh, LayoutParams(1, 1))
        e.view = fresh
        byView[fresh] = e
        if (e.paused) fresh.onPause()
        if (e.w > 0 && e.h > 0) place(e)
        if (refresh()) frame()
        val props = e.props
        e.props = ""
        // Loaded after every view of the renderer has been told and outside WebView's report of its end. A new
        // `src`, a park or a release in between takes the frame instead.
        android.os.Handler(context.mainLooper).post { if (elements[e.id] === e && e.view === fresh && e.props.isEmpty()) { apply(e, props); if (refresh()) frame() } }
    }

    private fun create(id: Int, kind: Int, props: String): Element? {
        val json = JSONObject(props)
        val view = obtain(kind, json, id) ?: return null
        if (view.parent == null) addView(view, LayoutParams(1, 1))
        val e = Element(id, kind, view, RenderNode("exact native"))
        if (kind == 2) {
            e.module = json.optString("module")
            e.generation = json.optInt("generation")
            e.session = json.optLong("session")
            e.slot = json.optInt("slot")
            bindEvents(e)
        }
        if (pooled(kind)) held++
        if (kind == 3) { if (view is TextureView) listen(e, view) else e.buffers = true; frameRate(e) }
        return e
    }

    private fun finishRedraws(e: Element) {
        val done = e.redraws.toList(); e.redraws.clear()
        done.forEach { it.run() }
    }

    private fun presentation(e: Element, surface: Boolean) {
        if ((e.view is SurfaceView) == surface || e.copying) return
        if (surface) {
            // Start on buffers, retaining one image until the window's first present.
            if (!e.presentation.canAttach(e.last != null)) return
            replacePresentation(e, true)
            return
        }
        if (e.view !is SurfaceView || !e.snapshotReady) return
        replacePresentation(e, false)
    }

    /** Keep the current composition until its visible surface has been copied.
     * The caller then ingests the new composition and saved image together. */
    internal fun prepare(stream: java.nio.IntBuffer, structure: Structure, resume: () -> Unit): Boolean {
        if (stream.limit() == 0 || stream.get(0) == 22 || elements.values.none { it.view is SurfaceView }) return true
        val direct = HashSet<Int>()
        var i = 0
        while (i < stream.limit()) {
            if (stream.get(i) == 28 && stream.get(i + 2) and 256 != 0) direct.add(stream.get(i + 1))
            i = structure.end(stream, i)
        }
        val e = elements.values.firstOrNull {
            it.view is SurfaceView && !direct.contains(it.id) && !it.snapshotReady
        } ?: return true
        if (!e.copying) copy(e) { resume() }
        return false
    }

    private fun copy(e: Element, done: () -> Unit) {
        val view = e.view as? SurfaceView ?: return
        val bitmap = android.graphics.Bitmap.createBitmap(e.w.coerceAtLeast(1), e.h.coerceAtLeast(1), android.graphics.Bitmap.Config.ARGB_8888)
        e.copying = true
        val copied: (Int) -> Unit = { result ->
            e.copying = false
            if (elements[e.id] === e && e.view === view) {
                if (result == android.view.PixelCopy.SUCCESS) e.last = bitmap else {
                    bitmap.recycle()
                    e.last = null
                    android.util.Log.w("exact", "canvas ${e.id} snapshot failed: $result; waiting for a fresh buffer")
                }
                e.snapshotReady = true
            } else bitmap.recycle()
            done()
        }
        try { android.view.PixelCopy.request(view, bitmap, copied, handler) }
        catch (_: IllegalArgumentException) { copied(android.view.PixelCopy.ERROR_SOURCE_INVALID) }
    }

    private fun replacePresentation(e: Element, surface: Boolean) {
        if (e.buffers) { if (e.asked != 0L) Exact.detachBuffers(e.id) } else unwindow(e)
        finishRedraws(e)
        e.presentation.clear()
        e.snapshotReady = false
        e.wrapped.clear(); e.asked = 0L; e.drawn = -1L
        byView.remove(e.view); removeView(e.view)
        val view = if (surface) SurfaceView(context) else obtain(3, JSONObject())!!
        e.view = view; e.buffers = !surface && view !is TextureView
        byView[view] = e
        if (view is SurfaceView) view.holder.addCallback(object : SurfaceHolder.Callback2 {
            override fun surfaceCreated(holder: SurfaceHolder) {}
            override fun surfaceChanged(holder: SurfaceHolder, format: Int, w: Int, h: Int) {
                if (e.view !== view) return
                unwindow(e)
                e.presentation.attach()
                e.surface = holder.surface
                Exact.attachSurface(e.id, holder.surface, w, h) { e.window = it }
                frameRate(e)
                e.dirty = true
                if (refresh()) frame()
                poll()
            }
            override fun surfaceRedrawNeeded(holder: SurfaceHolder) { poll() }
            override fun surfaceRedrawNeededAsync(holder: SurfaceHolder, drawingFinished: Runnable) {
                if (e.view !== view || e.presentation.ready) drawingFinished.run()
                else { e.redraws.add(drawingFinished); poll() }
            }
            override fun surfaceDestroyed(holder: SurfaceHolder) {
                if (e.view === view) {
                    unwindow(e); e.presentation.clear()
                    e.dirty = true; finishRedraws(e)
                    if (refresh()) frame()
                }
            }
        }) else if (view is TextureView) listen(e, view)
        addView(view, LayoutParams(1, 1))
        frameRate(e)
        if (e.w > 0 && e.h > 0) { place(e); askBuffers(e); record(e) }
        invalidate(); poll()
    }

    private fun bindEvents(e: Element) {
        val binding = ++e.eventBinding
        modules[e.module]?.events(e.view) { kind, payload ->
            // Modules may reply asynchronously; retirement or reuse must never retarget that reply.
            post {
                if (elements[e.id] === e && !e.parked && e.eventBinding == binding) {
                    Exact.moduleEvent(e.id, e.slot, e.generation, e.session, kind, payload)
                }
            }
        }
        modules[e.module]?.let { lifecycle.made(e.view, "${e.module}:${e.id}:${e.slot}:${e.generation}", it) }
    }

    /** The module view the engine's hit names, while that allocation of that session is still mounted and enabled
     * when its reply arrives. */
    fun taker(id: Int, slot: Int, generation: Int, session: Long): View? {
        // A media element with a controller takes its gestures as a module's view does (LLP 1042.001 D4).
        val e = elements[id]?.takeIf {
            (it.kind == 2 || (it.kind == 0 && players[it.view]?.lendable == false)) &&
                !it.parked && it.slot == slot && it.generation == generation && it.session == session
        } ?: return null
        return e.view.takeIf { !e.disabled && it.isEnabled }
    }

    internal var retiring: ((View) -> Unit)? = null

    private fun moduleProps(json: JSONObject): JSONObject =
        json.optString("props").takeIf { it.isNotEmpty() }?.let { runCatching { JSONObject(it) }.getOrNull() } ?: JSONObject()

    private fun apply(e: Element, props: String) {
        val old = e.props.takeIf { it.isNotEmpty() }?.let { JSONObject(it) }
        val json = JSONObject(props)
        e.props = props
        when (e.kind) {
            0 -> {
                val view = e.view
                val p = players[view] ?: return
                val src = json.optString("src")
                val key = (json.optLong("session") * 31 + json.optInt("generation")) * 31 + json.optInt("slot")
                val record = e.record ?: MediaRecords.of(e.id, key).also { e.record = it }
                val from = if (e.started) null else record.resume
                if (!e.started) p.bind(sink(e, view))
                // A lent view holds the last source's frame until this one's first.
                if (sources.containsKey(view) && sources[view] != src) e.stale = true
                sources[view] = src
                when (val s = Sources.video(src, assets) { Native.appFile(it)?.let(::File)?.takeIf { f -> Files.regular(f) } }) {
                    is Source.Remote -> json.put("uri", s.url)
                    is Source.Local -> json.put("uri", Uri.fromFile(s.file).toString())
                    is Source.Refused -> json.put("refused", s.why)
                }
                p.update(json, e.w, e.h, from)
                e.started = true
                // exact2's focus left the controller: the reader's view takes the window's focus back (D4).
                if (!json.optBoolean("focused") && view.hasFocus()) reader?.requestFocus()
                // Until the reader says where its row is, a lazy video waits to be seen ([near] shows it).
                val shown = e.inView || !lazyVideo || !lazy || e.rows.contains(0)
                if (shown != e.playing) { e.playing = shown; p.shown(shown) }
            }
            1 -> {
                val src = json.optString("src")
                if (old?.optString("src") != src) {
                    val web = e.view as WebView
                    val s = Sources.frame(src, assets)
                    // A remote page or the error page reads no file; a pooled view may have shown a local one.
                    web.settings.allowFileAccess = s is Source.Local
                    when (s) {
                        is Source.Remote -> web.loadUrl(s.url, Sources.frameHeaders(s.url))
                        is Source.Local -> web.loadUrl(Uri.fromFile(s.file).toString() + s.suffix)
                        is Source.Refused -> {
                            android.util.Log.w("exact", "iframe ${e.id}: ${s.why}")
                            web.loadDataWithBaseURL(null, Sources.errorPage(s.why), "text/html", "utf-8", null)
                        }
                    }
                }
            }
            // A hatch gave the node another view.
            5 -> if (old != null && old.optInt("gen") != json.optInt("gen")) {
                val view = obtain(5, json, e.id) ?: return
                byView.remove(e.view); removeView(e.view)
                e.view = view
                if (view.parent == null) addView(view, LayoutParams(1, 1))
                byView[view] = e
                if (e.w > 0 && e.h > 0) place(e)
            }
            else -> {
                modules[e.module]?.update(e.view, moduleProps(json))
                // An update that asked for a layout (an ImageView's scale type takes effect only at its next layout)
                // is laid out before it is recorded: at an unchanged size nothing else lays the view out first.
                if (e.view.isLayoutRequested && e.w > 0 && e.h > 0) place(e)
            }
        }
        e.dirty = true
    }

    /** A GPU canvas's TextureView has a surface (again, or at a new size): the host presents into it. */
    private fun window(e: Element, st: android.graphics.SurfaceTexture, w: Int, h: Int) {
        unwindow(e)
        st.setDefaultBufferSize(w, h)
        val surface = android.view.Surface(st)
        e.presentation.attach()
        e.surface = surface
        // Not waited for: exact2's thread may be busy (the GPU module's first frame, right after the first content),
        // and the main thread would wait out that work in its first draw.
        Exact.attachSurface(e.id, surface, w, h) { e.window = it }
        frameRate(e)
    }

    /** The frame rates GPU canvases ask for ([Native.frameRates]: view and rate pairs, 0 for the display's): a
     * canvas with a window of its own votes on its surface (Android 11 and later); one drawn in the reader's window
     * votes on its view (Android 15 and later, [votes]). These are platform hints; the module still paces its cap
     * (LLP 1009). */
    fun frameRates(rates: IntArray) {
        for (k in 0 until rates.size / 2) {
            val e = elements[rates[2 * k]] ?: continue
            frameRate(e, rates[2 * k + 1].toFloat())
        }
    }

    /** A canvas given a view or a window asks for the rate last told for it, if any. */
    private fun frameRate(e: Element) {
        val fps = Exact.owner?.frameRates?.of(e.id) ?: 0
        if (fps > 0) frameRate(e, fps.toFloat())
    }

    private fun frameRate(e: Element, fps: Float) {
        val surface = e.surface
        if (e.view is SurfaceView && surface != null && surface.isValid && android.os.Build.VERSION.SDK_INT >= 30) {
            try {
                // AOSP's RefreshRateSelector suppresses touch boost for Default votes. FIXED_SOURCE permits a
                // higher multiple on touch; a canvas cap asks to keep its rate during interaction (#574).
                surface.setFrameRate(fps, android.view.Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
            } catch (x: IllegalArgumentException) {}
        } else if (android.os.Build.VERSION.SDK_INT >= 35) {
            e.view.requestedFrameRate = if (fps > 0f) fps else android.view.View.REQUESTED_FRAME_RATE_CATEGORY_DEFAULT
        }
    }

    private fun unwindow(e: Element) {
        // Waited for: after the attach posted before it, and before the surface goes.
        if (e.surface != null) Exact.detachSurface(e.id) { e.window.also { e.window = 0L } }
        if (e.view !is SurfaceView) e.surface?.release()
        e.surface = null
    }

    private fun release(e: Element, reuse: Boolean = true, forget: Boolean = true) {
        elements.remove(e.id)
        nodes = null
        // The node is gone; a reader made again keeps the records of the nodes it will make again (LLP 1042.001 D1).
        if (e.kind == 0 && forget) MediaRecords.forget(e.id)
        if (e.parked) { waiting--; e.node.discardDisplayList(); return }
        byView.remove(e.view)
        if (e.kind == 3) { if (e.buffers) { if (e.asked != 0L) Exact.detachBuffers(e.id); e.wrapped.clear() } else unwindow(e) }
        retire(e, reuse)
        e.node.discardDisplayList()
    }

    /** [e]'s view is kept for the next element of its kind, or goes. */
    private fun retire(e: Element, reuse: Boolean = true) {
        if (e.kind == 2) retiring?.invoke(e.view)
        e.eventBinding++
        if (pooled(e.kind)) held--
        val module = e.module?.let { modules[it] }
        if (e.kind == 0) {
            val player = players[e.view]
            // Out of view: the off-screen rule pauses it first, so the record says it resumes when seen (D1).
            if (player != null && e.started) { player.shown(false); player.stop(); keep(e) }
            e.loan++
            if (player != null && player.lendable && lazyVideo && videoPool.size < VIDEO_POOL) {
                // Kept with its player, paused: the next video row's, prepared again only for another source.
                videoPool.add(e.view)
            } else {
                players.remove(e.view)?.release()
                sources.remove(e.view)
                removeView(e.view)
            }
            e.playing = false; e.inView = false; e.stale = false; e.started = false
        } else if (e.kind == 2 && module != null && (pool[e.module!!]?.size ?: 0) < modulePool && module.reset(e.view)) {
            lifecycle.unbind(e.view)
            pool.getOrPut(e.module!!) { ArrayList() }.add(e.view)
        } else if (e.kind == 1 && reuse && webPool.size < WEB_POOL && webs) {
            webPool.add((e.view as WebView).apply {
                // onPause alone keeps JavaScript running; a lent view must hold no active guest.
                stopLoading()
                settings.javaScriptEnabled = false
                settings.allowFileAccess = false
                loadUrl("about:blank")
                onPause()
            })
        } else {
            lifecycle.destroyed(e.view)
            module?.destroy(e.view)
            removeView(e.view)
            (e.view as? WebView)?.destroy()
        }
    }

    fun destroyAll() {
        for (e in elements.values.toList()) release(e, forget = false)
        for (w in webPool) { removeView(w); w.destroy() }
        webPool.clear()
        for (v in videoPool) { players.remove(v)?.release(); removeView(v) }
        videoPool.clear(); sources.clear()
        for ((name, views) in pool) for (v in views) { lifecycle.destroyed(v); modules[name]?.destroy(v); removeView(v) }
        pool.clear()
    }
}

/** A module's views kept for reuse: what a row leaving lends the row arriving. A kept view holds all a shown one does
 * (a MapLibre map ~135 MB on a Pixel 10 Pro XL), so a feed of them must not keep one for every row that ever left. */
private const val MODULE_POOL = 2
/** How long `ViewRootImpl` keeps a voted frame rate after the last vote for it (`FRAME_RATE_SETTING_REEVALUATE_TIME`,
 * Android 15 and 16). */
private const val VOTE_HOLD_MS = 100

/** Video views kept with their players for reuse: the one a row leaving lends the row arriving, and one more for
 * two video rows near at once (a Media3 player holds a decoder and its threads; making one costs ~40 ms here). */
private const val VIDEO_POOL = 2

/** WebViews kept for reuse: a screen and a half of webview rows. */
private const val WEB_POOL = 3
