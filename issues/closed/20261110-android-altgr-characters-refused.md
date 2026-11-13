# On Android a hardware keyboard's AltGr characters are not typed into a field exact2 paints

**Status:** Closed
**Resolution:** `Keys.typed` refuses Alt only when the key types the same character, or none, without it, so a layout's AltGr characters (its `ralt`) are typed; Ctrl and Meta chords and dead keys are still refused.
**Systems:** host/android/canvas/src/main/java/dev/exact/canvas/Keys.kt
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

At `dd6f5dcd`. `Keys.down` types a hardware key into the focused field that exact2 paints. On Android that is a Markdown editor, `textarea markup="markdown"`: the reader hosts every other field as an `EditText` (`host/linux/src/fields.rs:112`). `down` refuses every key pressed with Alt before it asks what the key types (`Keys.kt:61` to `:64`):

```kotlin
if (e.isCtrlPressed || e.isMetaPressed || e.isAltPressed) return false
val c = e.unicodeChar
if (c == 0 || c and android.view.KeyCharacterMap.COMBINING_ACCENT != 0) return false
Exact.key(c, false)
```

`isAltPressed` is true for either Alt key (it tests `META_ALT_ON`), and on Android's layouts the right Alt is how a key's third character is typed: a key character map gives it under `ralt`, "Applies when the RIGHT ALT modifier is pressed" ([Key Character Map Files](https://source.android.com/docs/core/interaction/input/key-character-map-files)), and `KeyEvent.getUnicodeChar()` returns that character for the event's meta state. AOSP's German layout has `key Q { … ralt: '@' }` (`frameworks/base/packages/InputDevices/res/raw/keyboard_layout_german.kcm`).

**The failure.** A tablet with a hardware keyboard set to German, the caret in a Markdown editor. Press AltGr+Q.

- observed (by reading): `down` returns false at its first line, and nothing is typed. The same holds for `€`, `[`, `]`, `{`, `}`, `\`, `|` and `~` on that layout, several of which Markdown is written with, and for the AltGr characters of the French, Spanish and Polish layouts;
- expected: `@`, as the hosted `EditText` beside it types.

**Verification.** Read only: `Keys.kt`, and AOSP's `KeyEvent.java` and German layout. Not run: no Android device or hardware keyboard was available. The device check: a keyboard with the German layout chosen in Settings, AltGr+Q in a `textarea markup="markdown"`.

**Direction.** Refuse Alt only where it types nothing of its own: take the key when `e.unicodeChar` differs from `e.getUnicodeChar(e.metaState and KeyEvent.META_ALT_MASK.inv())`, the character without Alt.

**Related.** `issues/20261110-android-hardware-tab-is-typed-as-a-character.md` is another key in the same function.
