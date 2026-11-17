# On Android a hardware keyboard's Tab on the page is sent as the character U+0009 and moves no focus

**Status:** Closed
**Resolution:** `Keys.named` sends Tab on the page by name, as a remote's keys go, with Shift held for that key alone in `CanvasHost::named_key`, so Tab and Shift+Tab move exact2's focus; a chord with Ctrl, Alt or Meta is left to Android.
**Systems:** host/android/canvas/src/main/java/dev/exact/canvas/Keys.kt, host/linux/src/canvas/keys.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

At `dd6f5dcd`. When the reader's view holds the focus, `Keys.down` (`Keys.kt:55`) names only a TV remote's keys. On a phone or tablet every other key goes through its `else` arm:

```kotlin
val c = e.unicodeChar
if (c == 0 || c and android.view.KeyCharacterMap.COMBINING_ACCENT != 0) return false
Exact.key(c, false)
…
return true
```

`KEYCODE_TAB`'s character is U+0009 in Android's key character map (AOSP `frameworks/base/data/keyboards/Generic.kcm`: `key TAB { label: '\t' base: '\t' }`), with Shift held too. So Tab is sent as `Native.key(9, false)`, which is `Presenter::key(Some('\t'), …)` (`host/linux/src/canvas/keys.rs:26`). The presenter names a key by its character (`host/linux/src/presenter/typing.rs:599`), so the keydown is named `"\t"`, not `"Tab"`, and its default action moves the focus only for `"Tab"` (`typing.rs:721`). `Keys.down` returns true, so Android's own Tab navigation, which runs only for a key the view left unhandled, does not run either.

The reader's view holds the focus whenever no hosted field does: `Fields.yielded` gives it the focus each time exact2 moves the focus off a field (`Fields.kt:621`).

**The failure.** A form with a text field, then a checkbox and a Save button, on a tablet with a keyboard. Tab in the field moves to the checkbox (the field's own key listener sends `fieldAction` 16, `Fields.kt:374`). Tab again:

- observed: nothing moves; an app's `key` handler hears a key named `"\t"`;
- expected: the focus moves to Save, as LLP 1115.001 D7.6 decides ("A hardware keyboard moves focus between controls outside a text field") and as Tab does in any Android form ([Support keyboard navigation](https://developer.android.com/develop/ui/views/touch-and-input/keyboard-input/navigation)).

**Verification.** The presenter's half was run on macOS, in a temporary test in `host/linux/src/presenter/field_tests.rs`:

```rust
let mut p = boot();
p.key(Some('\t'), false, 1.);               // focus: None
p.hardware_key("Tab", "Tab", true, false);  // focus: Some(3)
```

With the focus on a radio, `p.key(Some('\t'), …)` left it there. The Kotlin half is read, with AOSP's key character map; no Android device or emulator was available. The check to make on one: with a button focused on the page, `adb shell input keyevent KEYCODE_TAB`.

**Direction.** Send Tab by name, as the remote's keys go (`Exact.namedKey("Tab", "Tab", …)`), and Shift+Tab with Shift told to the presenter, as the field's action 17 and a surface's key 18 already tell it.

**Related.** LLP 1115.001 D7.6 says the presenter "today hears only characters, Enter and Backspace"; S1's record lists D7.6 for its focus ring only. Tab has since been built from a field and in a surface's window, not on the page. D7.6 also names the arrows, which LLP 1115.001.003 D3 leaves to Android on a phone. No issue or QUEUE entry names Tab on the page.
