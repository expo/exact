package dev.exact.canvas

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.HardwareRenderer
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.RenderNode
import android.graphics.Shader
import android.hardware.HardwareBuffer
import android.media.ImageReader
import android.os.Handler

/**
 * A press wakes the GPU, as a platform press's ripple does: one pixel drawn offscreen (a renderer of its own over a 1x1
 * image, on the RenderThread), so the release's response, ~50-100 ms on, finds the GPU awake. After ~50 ms idle the
 * first frame's GPU work takes 7-10 ms more on the Nothing Phone (4a) Pro (the GPU wakes and clocks up), and a
 * response frame that pays that misses its deadline. Nothing of the window is drawn: no frame of the app's, no damage.
 */
internal class Warm {
    private var renderer: HardwareRenderer? = null
    private var reader: ImageReader? = null
    private val node = RenderNode("exact warm").apply { setPosition(0, 0, 1, 1) }
    private var shade = 0

    /** Its images are let go here: taking one waits for its fence, the GPU's work (not on the main thread). */
    private var drain: android.os.HandlerThread? = null

    /** Made before the first press (after first content): the renderer and its first frame cost ~15 ms. */
    fun prepare() { if (renderer == null) poke() }

    fun poke() {
        try {
            val r = renderer ?: run {
                val ir = ImageReader.newInstance(1, 1, PixelFormat.RGBA_8888, 2,
                    HardwareBuffer.USAGE_GPU_COLOR_OUTPUT or HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE)
                val d = android.os.HandlerThread("exact warm").apply { start() }.also { drain = it }
                ir.setOnImageAvailableListener({ it.acquireLatestImage()?.close() }, Handler(d.looper))
                reader = ir
                HardwareRenderer().apply { setSurface(ir.surface); setContentRoot(node) }.also { renderer = it }
            }
            // A new drawing each time: an unchanged one is no frame.
            val c = node.beginRecording()
            c.drawColor(if (++shade % 2 == 0) 0xff000000.toInt() else 0xffffffff.toInt())
            node.endRecording()
            android.os.Trace.beginSection("exact warm")
            r.createRenderRequest().syncAndDraw()
            android.os.Trace.endSection()
        } catch (e: Exception) {
            android.util.Log.w("exact", "warm: $e")
        }
    }

    fun destroy() {
        renderer?.destroy(); renderer = null
        reader?.close(); reader = null
        drain?.quitSafely(); drain = null
    }

    companion object {
        /** Set as the host makes its first picture: a frame of this then would hold the RenderThread from it, and the main
         * thread's own calls into it. */
        @Volatile var stop = false

        /**
         * At a launch, while the host boots and the RenderThread has nothing to draw: offscreen frames of the drawing
         * [Player] does most, so the GPU pipelines a first screen needs are made then, not inside its frame. HWUI
         * keeps them in its cache on disk, saved seconds after a launch; with none (an install's first launch, or after
         * the app's cache was cleared) a moto g57 power's first screen made 15 in ~160 ms. Small
         * frames until [stop], one in flight delaying the first picture by its own few pipelines; first those that
         * saved Jetchat's first screen the most pipelines on the moto.
         */
        fun launch() = Thread({
            // Optional: any failure, an allocation's included, gives the warm-up up.
            try { if (!stop) frames() } catch (e: Throwable) { android.util.Log.w("exact", "warm launch: $e") }
        }, "exact warm launch").start()

        private fun frames() {
            val size = 64
            val reader = ImageReader.newInstance(size, size, PixelFormat.RGBA_8888, 2,
                HardwareBuffer.USAGE_GPU_COLOR_OUTPUT or HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE)
            try {
                val node = RenderNode("exact warm launch").apply { setPosition(0, 0, size, size) }
                val renderer = HardwareRenderer()
                try {
                    renderer.setSurface(reader.surface); renderer.setContentRoot(node)
                    val soft = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(0x80ff8000.toInt()) }
                    val hard = soft.copy(Bitmap.Config.HARDWARE, false) ?: soft
                    val mask = soft.extractAlpha()
                    // [Player]'s paints. Every draw covers the frame: one outside it is culled, and makes nothing.
                    val all = RectF(0f, 0f, size.toFloat(), size.toFloat())
                    val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xff336699.toInt() }
                    val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeWidth = 3f }
                    val image = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)
                    val text = Paint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 40f }
                    val gradient = LinearGradient(0f, 0f, 64f, 64f, -1, 0xff000000.toInt(), Shader.TileMode.CLAMP)
                    val circle = Path().apply { addCircle(32f, 32f, 32f, Path.Direction.CW) }
                    val bubble = Path().apply { addRoundRect(all, floatArrayOf(4f, 4f, 16f, 16f, 16f, 16f, 16f, 16f), Path.Direction.CW) }
                    val convex = Path().apply { moveTo(0f, 0f); lineTo(64f, 0f); lineTo(32f, 64f); close() }
                    val concave = Path().apply { moveTo(0f, 0f); lineTo(64f, 0f); lineTo(32f, 20f); lineTo(64f, 64f); lineTo(0f, 64f); close() }
                    val child = RenderNode("exact warm child").apply {
                        setPosition(0, 0, size, size)
                        val c = beginRecording(); c.drawRect(all, fill); endRecording()
                    }
                    val frames = listOf<(Canvas) -> Unit>(
                        { c -> c.drawPath(bubble, fill); c.drawPath(convex, fill); c.drawPath(concave, fill) },
                        { c -> c.drawLines(floatArrayOf(0f, 0f, 64f, 64f, 64f, 0f, 0f, 64f), stroke)
                            c.drawPoints(floatArrayOf(32f, 32f), Paint(stroke).apply { strokeCap = Paint.Cap.ROUND; strokeWidth = 64f })
                            c.drawText("\uD83D\uDE00", 0f, 40f, text); child.alpha = 0.5f; c.drawRenderNode(child) },
                        { c -> c.drawRect(1.5f, 1.5f, 62.5f, 62.5f, fill); c.drawRoundRect(all, 12f, 12f, fill); c.drawText("Ag", 0f, 40f, text) },
                        { c -> c.save(); c.clipPath(circle); c.drawBitmap(hard, null, all, image); c.restore(); c.drawOval(all, fill)
                            c.save(); c.clipPath(bubble); c.drawBitmap(hard, null, all, image); c.restore() },
                        { c -> c.drawBitmap(hard, null, RectF(0.5f, 0.5f, 63.5f, 63.5f), image); c.drawBitmap(soft, 0f, 0f, null); c.drawBitmap(soft, null, all, null) },
                        { c -> c.drawOval(all, stroke); c.drawDoubleRoundRect(all, 12f, 12f, RectF(3f, 3f, 61f, 61f), 9f, 9f, fill); c.drawBitmap(mask, null, all, fill) },
                        { c -> fill.shader = gradient; c.drawRect(all, fill); c.drawRoundRect(all, 12f, 12f, fill); fill.shader = null
                            c.saveLayerAlpha(all, 128); c.drawRect(all, fill); c.drawOval(all, stroke); c.restore() },
                    )
                    for (draw in frames) {
                        if (stop) break
                        val c = node.beginRecording(); c.drawColor(-1); draw(c); node.endRecording()
                        renderer.createRenderRequest().setWaitForPresent(true).syncAndDraw()
                        // Let go, or the next frame waits for a buffer, and the RenderThread with it.
                        reader.acquireLatestImage()?.close()
                    }
                } finally { renderer.destroy() }
            } finally { reader.close() }
        }
    }
}
