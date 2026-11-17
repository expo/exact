package dev.exact.canvas

import android.content.Context
import android.text.InputType
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager

/**
 * Keys and the software keyboard for an [ExactView] whose app has text fields (exact2's `input`/`textarea`).
 *
 * The view is focusable, so the window's key events (a hardware keyboard's, ones injected through the window's
 * dispatch, the IME's) reach it; each character, Enter and Backspace goes to exact2's focused field
 * ([Native.key]), as the Linux host's display keyboard does. While an editable field holds the focus the view
 * shows the platform's keyboard, and hides it when the focus leaves, as a focused `<input>` does in Chrome on
 * Android: exact2 says so as a request ([show], LLP 1115.001 D5), whatever moved the focus (a tap, a key, the app).
 * The IME talks to a [BaseInputConnection] in its dummy mode: what it commits arrives as key events.
 */
class Keys(private val view: ExactView) {
    private val imm = view.context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
    /** The device is a TV, whose remote's D-pad is exact2's ([named]). */
    private val tv = Device.tv(view.context)
    /** What exact2 last asked for ([show]): 0 none, 1 an input, 3 a textarea. */
    private var shown = 0

    init {
        view.isFocusable = true
        view.isFocusableInTouchMode = true
        // The app draws its own focus; the platform's highlight is a grey wash over the whole view whenever the window
        // is not in touch mode (a launch by `am start`, a keyboard): 223 for 254 white in every recording.
        view.defaultFocusHighlightEnabled = false
    }

    fun editing() = shown != 0

    /** exact2 asks for the keyboard ([Requests]): shown for a newly focused field ([now] 1 an input, 3 a textarea),
     * hidden when none holds the focus (0). */
    fun show(now: Int) {
        if (now == shown) return
        shown = now
        if (now != 0) {
            if (!view.hasFocus()) view.requestFocus()
            imm.restartInput(view)
            imm.showSoftInput(view, 0)
        } else {
            imm.hideSoftInputFromWindow(view.windowToken, 0)
        }
    }

    /** A key down: a key exact2 hears by name ([named]), or a character, Enter or Backspace typed at the focused
     * field. Whether it was taken. */
    fun down(e: KeyEvent): Boolean {
        named(e, tv)?.let { (code, key) ->
            Exact.namedKey(code, key, true, e.repeatCount > 0, e.isShiftPressed)
            return true
        }
        when (e.keyCode) {
            KeyEvent.KEYCODE_DEL -> Exact.key(0, true)
            KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> Exact.key('\n'.code, false)
            else -> {
                val c = typed(e)
                if (c == 0) return false
                Exact.key(c, false)
            }
        }
        return true
    }

    /** A key up: the release of a key exact2 hears by name ([named]). Whether it was taken. */
    fun up(e: KeyEvent): Boolean {
        val (code, key) = named(e, tv) ?: return false
        Exact.namedKey(code, key, false, false, e.isShiftPressed)
        return true
    }

    companion object {
        /** A key exact2 hears by the web's `code` and `key`, not as a character, or null. Tab, whose character is
         * U+0009, moves exact2's focus (LLP 1115.001 D7.6), and Shift+Tab moves it back; with Ctrl, Alt or Meta held
         * it is Android's. On a TV ([tv]) a remote's keys (LLP 1115.001.003 D3): the D-pad's arrows, and Select, which a `key` handler
         * hears as Enter, as Chrome on Android sends DPAD_CENTER. A phone keyboard's arrows keep Android's handling. */
        internal fun named(e: KeyEvent, tv: Boolean): Pair<String, String>? = when (e.keyCode) {
            KeyEvent.KEYCODE_TAB -> if (e.isCtrlPressed || e.isAltPressed || e.isMetaPressed) null else "Tab" to "Tab"
            else -> if (tv) remote(e.keyCode) else null
        }

        private fun remote(keyCode: Int): Pair<String, String>? = when (keyCode) {
            KeyEvent.KEYCODE_DPAD_UP -> "ArrowUp" to "ArrowUp"
            KeyEvent.KEYCODE_DPAD_DOWN -> "ArrowDown" to "ArrowDown"
            KeyEvent.KEYCODE_DPAD_LEFT -> "ArrowLeft" to "ArrowLeft"
            KeyEvent.KEYCODE_DPAD_RIGHT -> "ArrowRight" to "ArrowRight"
            KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_BUTTON_SELECT -> "Select" to "Enter"
            else -> null
        }

        /** The character a key types at the focused field, or 0 when it types none here: a chord with Ctrl or Meta, a
         * dead key, or a key that gives nothing with the Alt held. Alt itself is no refusal: the right Alt (AltGr) is
         * how a layout types a key's third character, its key character map's `ralt` (German AltGr+Q is '@'). */
        internal fun typed(e: KeyEvent): Int = typed(
            e.unicodeChar, e.getUnicodeChar(e.metaState and KeyEvent.META_ALT_MASK.inv()),
            e.isCtrlPressed || e.isMetaPressed, e.isAltPressed,
        )

        /** [typed] from the character with the event's meta state ([c]) and with its Alt taken away ([withoutAlt]). */
        internal fun typed(c: Int, withoutAlt: Int, chord: Boolean, alt: Boolean): Int = when {
            chord || (alt && c == withoutAlt) -> 0
            c == 0 || c and KeyCharacterMap.COMBINING_ACCENT != 0 -> 0
            else -> c
        }
    }

    /** Text the IME committed as one event (dummy mode, more than one character). */
    fun multiple(e: KeyEvent): Boolean {
        val text = e.characters ?: return false
        text.codePoints().forEach { Exact.key(if (it == '\r'.code) '\n'.code else it, false) }
        return true
    }

    fun connection(info: EditorInfo): InputConnection? {
        if (shown == 0) return null
        info.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_AUTO_CORRECT or
            (if (shown == 3) InputType.TYPE_TEXT_FLAG_MULTI_LINE else 0)
        info.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN or
            (if (shown == 3) EditorInfo.IME_ACTION_NONE else EditorInfo.IME_ACTION_DONE)
        return BaseInputConnection(view, false)
    }
}
