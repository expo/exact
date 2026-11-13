package dev.exact.canvas

import android.app.Activity
import android.widget.EditText
import android.widget.FrameLayout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import java.lang.ref.WeakReference
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class FieldsTest {
    companion object {
        private val family = File("../../painted/tests/fonts/Inter-Regular.ttf").canonicalPath + "#0"

        /** A statement of one field, id 1, with these font features; with a size, placed in that box. */
        fun statement(features: String = "", x: Float = 0f, y: Float = 0f, w: Float = 0f, h: Float = 0f): IntArray {
            val placed = if (w > 0f) 1 else 0
            val words = mutableListOf(1, 1, 0, placed, 0, x.toBits(), y.toBits(), w.toBits(), h.toBits(), 0, 0, (-1f).toBits(), (-1f).toBits(), 0)
            fun string(s: String) {
                val bytes = s.toByteArray(Charsets.UTF_8)
                words.add(bytes.size)
                for (part in bytes.toList().chunked(4)) {
                    words.add(part.foldIndexed(0) { i, word, b -> word or ((b.toInt() and 255) shl (i * 8)) })
                }
            }
            string("1111")
            words.addAll(listOf(0, 0, 0, 0, -1, 0, -16777216, 0, 0, 0, 0, 40f.toBits(), 400, 0, 0, 0, 0, 0))
            for (s in listOf("", "", family, "", features)) string(s)
            return words.toIntArray()
        }
    }

    @Test fun displayedFieldsApplyAndClearNumericFeatures() {
        val fields = Fields(RuntimeEnvironment.getApplication())
        fields.told(statement("'tnum' 1"))
        val text = (fields.getChildAt(0) as FrameLayout).getChildAt(0) as EditText
        assertEquals("'tnum' 1", text.fontFeatureSettings)
        assertEquals(text.paint.measureText("1111"), text.paint.measureText("8888"), 0.01f)
        fields.told(statement("'tnum' 0"))
        assertEquals("'tnum' 0", text.fontFeatureSettings)
        val normal = text.paint.measureText("1111")
        assertTrue(normal < text.paint.measureText("8888"))
        fields.told(statement("'tnum' 1"))
        fields.told(statement(""))
        assertNull(text.fontFeatureSettings)
        assertEquals(normal, text.paint.measureText("1111"), 0.01f)
    }

    @Test fun measurementAppliesAndClearsTheDisplayedFieldsFeatures() {
        val before = Fields.context
        Fields.context = RuntimeEnvironment.getApplication()
        try {
            for (flags in listOf(0, 2)) {
                fun width(text: String, features: String): Long =
                    Fields.measure(40f, 400, flags, 0f, 0f, family, "", features, text, -1f) and 0x1fffff
                val normal = width("1111", "")
                assertTrue(normal < width("8888", ""))
                val tabular = width("1111", "'tnum' 1")
                assertEquals(tabular, width("8888", "'tnum' 1"))
                assertTrue(normal < tabular)
                assertEquals(normal, width("1111", "'tnum' 0"))
                assertEquals(tabular, width("1111", "'tnum' 1"))
                assertEquals(normal, width("1111", ""))
            }
        } finally {
            Fields.context = before
        }
    }

    /** The measurer lives as long as exact2's thread, which outlives every activity (LLP 1115.001 D15.1): an activity
     * it measured for, once destroyed, is collected. */
    @Test fun theMeasurerKeepsNoActivity() {
        val before = Fields.context
        var opened: org.robolectric.android.controller.ActivityController<Activity>? =
            Robolectric.buildActivity(Activity::class.java).setup()
        val activity = WeakReference(opened!!.get())
        Fields.context = activity.get()
        val (measured, done) = CountDownLatch(1) to CountDownLatch(1)
        val thread = Thread {
            Fields.measure(16f, 400, 0, 0f, 0f, family, "", "", "x", -1f)
            measured.countDown()
            done.await()
        }
        try {
            thread.start()
            assertTrue(measured.await(10, TimeUnit.SECONDS))
            Fields.context = null
            opened.pause().stop().destroy()
            opened = null
            for (k in 0 until 20) {
                if (activity.get() == null) break
                System.gc()
                Thread.sleep(50)
            }
            assertNull("the destroyed activity is unreachable", activity.get())
        } finally {
            done.countDown()
            thread.join()
            Fields.context = before
        }
    }
}
