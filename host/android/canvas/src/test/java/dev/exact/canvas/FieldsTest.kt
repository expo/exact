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
        fun statement(features: String = "", x: Float = 0f, y: Float = 0f, w: Float = 0f, h: Float = 0f, flags: Int = 0, align: Int = 0, revision: Int = 0, caret: Int = 0): IntArray =
            statement(listOf(Told(1, x, y, w, h, if (w > 0f) 1 else 0, flags, "1111", features, align, revision, caret)))

        /** One field of a statement: its id, box, state (1 placed, 2 inert, 4 its node takes drops, 8 one above it
         * does), look flags, value, font features, alignment, selection revision and caret, and its clip
         * (x, y, width, height; none when null). */
        data class Told(val id: Int, val x: Float, val y: Float, val w: Float, val h: Float, val state: Int, val flags: Int = 0,
            val value: String = "$id", val features: String = "", val align: Int = 0, val revision: Int = 0, val caret: Int = 0,
            val clip: FloatArray? = null, val edit: Int = 0, val keyboard: Int = 0)

        fun statement(told: List<Told>): IntArray {
            val words = mutableListOf(told.size)
            fun string(s: String) {
                val bytes = s.toByteArray(Charsets.UTF_8)
                words.add(bytes.size)
                for (part in bytes.toList().chunked(4)) {
                    words.add(part.foldIndexed(0) { i, word, b -> word or ((b.toInt() and 255) shl (i * 8)) })
                }
            }
            for (t in told) {
                val clip = t.clip ?: floatArrayOf(0f, 0f, -1f, -1f)
                words.addAll(listOf(t.id, 0, t.state, 0, t.x.toBits(), t.y.toBits(), t.w.toBits(), t.h.toBits()) + clip.map(Float::toBits) + t.edit)
                string(t.value)
                words.addAll(listOf(t.flags, t.keyboard, 0, 0, -1, t.align, -16777216, 0, 0, 0, 0, 40f.toBits(), 400, 0, 0, t.revision, t.caret, t.caret))
                for (s in listOf("", "", family, "", t.features)) string(s)
            }
            return words.toIntArray()
        }
    }

    @Test fun anOlderStatementCannotMoveTheCaretInANewerEdit() {
        val fields = Fields(RuntimeEnvironment.getApplication())
        fields.told(statement())
        val text = (fields.getChildAt(0) as FrameLayout).getChildAt(0) as EditText
        text.append("x")
        text.setSelection(5)
        fields.told(statement(revision = 1, caret = 2))
        assertEquals("1111x", text.text.toString())
        assertEquals(5, text.selectionStart)
        assertEquals(5, text.selectionEnd)
    }

    @Test fun aFilteredValueCarriesTheEndpointsOfTheNativeSelection() {
        val fields = Fields(RuntimeEnvironment.getApplication())
        var edit = 0
        fun show(value: String) {
            fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, value = value, edit = edit))))
        }
        show("")
        val text = (fields.getChildAt(0) as FrameLayout).getChildAt(0) as EditText
        fun check(typed: String, start: Int, end: Int, accepted: String, afterStart: Int, afterEnd: Int) {
            text.text.replace(0, text.length(), typed)
            edit++
            text.setSelection(start, end)
            show(accepted)
            assertEquals(accepted, text.text.toString())
            assertEquals(afterStart, text.selectionStart)
            assertEquals(afterEnd, text.selectionEnd)
        }
        check("40a", 3, 3, "40", 2, 2)
        check("4a0", 2, 2, "40", 1, 1)
        check("a0", 1, 1, "0", 0, 0)
        check("47x0", 3, 3, "470", 2, 2)
        check("hello", 4, 1, "HELLO", 4, 1)
    }

    @Test fun fieldsApplyDirectionWithoutReversingPhysicalAlignment() {
        val context = RuntimeEnvironment.getApplication()
        context.applicationInfo.flags = context.applicationInfo.flags or android.content.pm.ApplicationInfo.FLAG_SUPPORTS_RTL
        val fields = Fields(context)
        fields.layoutDirection = android.view.View.LAYOUT_DIRECTION_RTL
        for (rtl in listOf(true, false, true)) {
            for (align in listOf(0, 2)) {
                fields.told(statement(x = 24f, y = 16f, w = 100f, h = 48f, flags = if (rtl) 512 else 0, align = align))
                fields.measure(android.view.View.MeasureSpec.makeMeasureSpec(400, android.view.View.MeasureSpec.EXACTLY),
                    android.view.View.MeasureSpec.makeMeasureSpec(200, android.view.View.MeasureSpec.EXACTLY))
                fields.layout(0, 0, 400, 200)
                val port = fields.getChildAt(0) as FrameLayout
                val text = port.getChildAt(0) as EditText
                // Where it stands among the fields: its port is its box (`Fields.position`)
                assertEquals(listOf(24f, 16f, 24f, 16f), listOf(port.x + text.x, port.y + text.y, port.x, port.y))
                assertEquals(if (rtl) android.view.View.LAYOUT_DIRECTION_RTL else android.view.View.LAYOUT_DIRECTION_LTR, text.layoutDirection)
                assertEquals(if (rtl) android.view.View.TEXT_DIRECTION_RTL else android.view.View.TEXT_DIRECTION_LTR, text.textDirection)
                assertEquals(if (align == 2) android.view.Gravity.RIGHT else android.view.Gravity.LEFT,
                    android.view.Gravity.getAbsoluteGravity(text.gravity, text.layoutDirection) and android.view.Gravity.HORIZONTAL_GRAVITY_MASK)
            }
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

    /** A selection made backward, which Android gives anchor first, is the same range. */
    @Test fun aBackwardSelectionIsReadStartFirst() {
        assertEquals(2 to 3, ordered(3, 2))
        assertEquals(2 to 3, ordered(2, 3))
        assertEquals(0 to 0, ordered(-1, -1))
    }

    @Test fun readOnlyFieldIsSelectableAndTogglesBackToEditableCleanly() {
        val fields = Fields(RuntimeEnvironment.getApplication())
        // Initial read-only field (flags = READONLY = 8).
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 8, value = "Solo lectura"))))
        val text = (fields.getChildAt(0) as FrameLayout).getChildAt(0) as EditText
        assertNull(text.keyListener)
        assertTrue(text.isTextSelectable)
        assertFalse(text.showSoftInputOnFocus)
        assertTrue(text.isFocusable)
        assertTrue(text.isFocusableInTouchMode)
        assertTrue(text.isLongClickable)

        // Unrelated style changes while still read-only must keep the active selection intact.
        text.setSelection(0, 4)
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 8, value = "Solo lectura", align = 1))))
        assertNull(text.keyListener)
        assertTrue(text.isTextSelectable)
        assertEquals(0, text.selectionStart)
        assertEquals(4, text.selectionEnd)

        // Programmatic value updates still apply while read-only.
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 8, value = "Nuevo valor", align = 1))))
        assertEquals("Nuevo valor", text.text.toString())

        // Leaving read-only restores editable input, movement, touch focus, click and long-click.
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 0, value = "Nuevo valor", align = 1))))
        assertFalse(text.isTextSelectable)
        assertNotNull(text.keyListener)
        assertTrue(text.showSoftInputOnFocus)
        assertTrue(text.movementMethod is android.text.method.ArrowKeyMovementMethod)
        assertTrue(text.isFocusable)
        assertTrue(text.isFocusableInTouchMode)
        assertTrue(text.isClickable)
        assertTrue(text.isLongClickable)
        text.text.append(" ok")
        assertEquals("Nuevo valor ok", text.text.toString())
    }

    @Test fun inertReadOnlyFieldStaysNonFocusable() {
        val fields = Fields(RuntimeEnvironment.getApplication())
        // state = PLACED (1) | INERT (2) = 3, flags = READONLY (8).
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 3, flags = 8, value = "covered"))))
        val text = (fields.getChildAt(0) as FrameLayout).getChildAt(0) as EditText
        assertTrue(text.isTextSelectable)
        assertFalse(text.isFocusable)
        assertFalse(text.isFocusableInTouchMode)
        // Becoming interactive restores focusability.
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 8, value = "covered"))))
        assertTrue(text.isFocusable)
        assertTrue(text.isFocusableInTouchMode)
    }

    @Config(shadows = [ScrollTouchTest.Host::class])
    @Test fun keyboardZeroForHostedReadOnlyFieldPreservesFocusAndSelection() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        activity.intent.putExtra("warm", "0")
        val view = ExactView(activity, false) {}
        val fields = Fields(activity).also { it.view = view }
        val root = FrameLayout(activity)
        root.addView(view, FrameLayout.LayoutParams(400, 400))
        root.addView(fields, FrameLayout.LayoutParams(400, 400))
        activity.setContentView(root)
        view.fields = fields
        view.keys = Keys(view)
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 8, value = "Solo lectura"))))
        val text = fields.text(1)!!

        // Simulate a long-press on an unfocused read-only field: Android focuses the field and selects a word,
        // then exact2 responds with Request::Keyboard(state = 0, field = 1). That response must hide any
        // keyboard without blurring the read-only field or clearing its selection.
        val imm = activity.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
        val shadowImm = org.robolectric.Shadows.shadowOf(imm)
        assertTrue(text.requestFocus())
        imm.showSoftInput(text, 0)
        assertTrue(shadowImm.isSoftInputVisible)
        text.setSelection(0, 4)
        view.keyboard(0, 1)
        assertTrue("read-only field kept focus after keyboard(0, 1)", text.isFocused)
        assertFalse("keyboard(0, 1) hid open IME", shadowImm.isSoftInputVisible)
        assertEquals(0, text.selectionStart)
        assertEquals(4, text.selectionEnd)

        // Copy succeeds while cut and paste are refused and do not mutate a read-only field or bump edits.
        val clipboard = activity.getSystemService(android.content.ClipboardManager::class.java)
        assertTrue(text.onTextContextMenuItem(android.R.id.copy))
        assertEquals("Solo", clipboard.primaryClip?.getItemAt(0)?.text?.toString())
        clipboard.setPrimaryClip(android.content.ClipData.newPlainText("test", "OTHER"))
        assertFalse(text.onTextContextMenuItem(android.R.id.cut))
        assertFalse(text.onTextContextMenuItem(android.R.id.paste))
        assertFalse(text.onTextContextMenuItem(android.R.id.pasteAsPlainText))
        assertEquals("Solo lectura", text.text.toString())
        assertEquals("OTHER", clipboard.primaryClip?.getItemAt(0)?.text?.toString())

        // Blurring (state = 0, field = 0) moves focus off the hosted field.
        view.keyboard(0, 0)
        assertFalse("read-only field blurred after keyboard(0, 0)", text.isFocused)

        // Programmatic focus of an unfocused read-only field (state = 0, field = 1) focuses it without keyboard.
        imm.showSoftInput(view, 0)
        assertTrue(shadowImm.isSoftInputVisible)
        view.keyboard(0, 1)
        assertTrue("read-only field focused by keyboard(0, 1)", text.isFocused)
        assertFalse("programmatic focus of read-only field keeps IME hidden", shadowImm.isSoftInputVisible)

        // Focused read-only field toggling back to editable preserves focus and selection.
        text.setSelection(1, 5)
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 0, value = "Solo lectura"))))
        assertTrue("focus survives readonly -> editable transition", text.isFocused)
        assertEquals(1, text.selectionStart)
        assertEquals(5, text.selectionEnd)

        // Already-selectable read-only field changing between password and plain with unchanged text keeps focus and selection.
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 10, value = "secret"))))
        assertTrue(text.transformationMethod is android.text.method.PasswordTransformationMethod)
        assertTrue(text.requestFocus())
        text.setSelection(1, 5)
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 8, value = "secret"))))
        assertFalse(text.transformationMethod is android.text.method.PasswordTransformationMethod)
        assertTrue("focus survives read-only password -> plain", text.isFocused)
        assertEquals(1, text.selectionStart)
        assertEquals(5, text.selectionEnd)
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 10, value = "secret"))))
        assertTrue(text.transformationMethod is android.text.method.PasswordTransformationMethod)
        assertTrue("focus survives read-only plain -> password", text.isFocused)
        assertEquals(1, text.selectionStart)
        assertEquals(5, text.selectionEnd)
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 8, value = "plain"))))
        assertFalse(text.transformationMethod is android.text.method.PasswordTransformationMethod)

        // Read-only field blocks accessibility ACTION_SET_TEXT and autofill mutation.
        val info = android.view.accessibility.AccessibilityNodeInfo.obtain()
        try {
            text.onInitializeAccessibilityNodeInfo(info)
            assertFalse("read-only field reports non-editable in accessibility", info.isEditable)
            assertFalse("read-only field omits ACTION_SET_TEXT", info.actionList.contains(android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_TEXT))
        } finally {
            info.recycle()
        }
        val args = android.os.Bundle().apply { putCharSequence(android.view.accessibility.AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, "tampered") }
        assertFalse(text.performAccessibilityAction(android.view.accessibility.AccessibilityNodeInfo.ACTION_SET_TEXT, args))
        assertEquals(android.view.View.AUTOFILL_TYPE_NONE, text.autofillType)
        text.autofill(android.view.autofill.AutofillValue.forText("tampered"))
        assertEquals("plain", text.text.toString())

        // inputmode="none" (keyboard = 7) hides the IME even when focus is requested with keyboard = true.
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 0, value = "plain", keyboard = 7))))
        imm.showSoftInput(text, 0)
        assertTrue(shadowImm.isSoftInputVisible)
        fields.focus(1, keyboard = true)
        assertTrue(text.isFocused)
        assertFalse("inputmode=none hides IME on focus", shadowImm.isSoftInputVisible)

        // Unfocusable inert field still hides an open IME on keyboard = false and retries focus once interactive.
        fields.blur(false)
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 3, flags = 8, value = "plain"))))
        imm.showSoftInput(view, 0)
        assertTrue(shadowImm.isSoftInputVisible)
        fields.focus(1, keyboard = false)
        assertFalse("inert field could not take focus yet", text.isFocused)
        assertFalse("failed requestFocus with keyboard=false still hid IME", shadowImm.isSoftInputVisible)
        fields.told(statement(listOf(Told(1, 0f, 0f, 200f, 48f, 1, flags = 8, value = "plain"))))
        assertTrue("pending focus succeeded once field became interactive", text.isFocused)

        // When a WebView owns focus, keyboard(0, 1) keeps the WebView guard and does not focus the hosted field.
        val web = object : android.webkit.WebView(activity) { override fun findFocus(): android.view.View = this }
        val inner = android.view.View(activity).apply { isFocusable = true; isFocusableInTouchMode = true }
        web.addView(inner, FrameLayout.LayoutParams(200, 200))
        root.addView(web, FrameLayout.LayoutParams(200, 200))
        assertTrue(inner.requestFocus())
        assertTrue(view.rootView.findFocus() is android.webkit.WebView)
        view.keyboard(0, 1)
        assertFalse("WebView guard prevented hosted focus steal on keyboard(0, 1)", text.isFocused)
    }
}
