# On Windows a dead key followed by Space types a space, and followed by a letter it cannot join types the letter alone

**Status:** Closed
**Resolution:** The Windows host types a key's `text` when it differs from the logical key (a dead key's accent before Space or a letter it does not combine with) while the app's `key` handlers still hear the logical key, and names a dead key `"Dead"`.
**Systems:** host/windows/src/window.rs
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

At `dd6f5dcd`. `key_names` (`host/windows/src/window.rs:87`) names a key from winit's `logical_key` and reads the event's `text` only when the logical key is neither a character nor a named key (`:95`). The presenter then inserts that name when it is one character (`host/linux/src/presenter/field.rs:1071`), and a space for `"Space"` (`host/linux/src/presenter/typing.rs:434`).

After a dead key, the text a key press produces is not its logical key. Windows: "If the subsequent keystroke generates a character that cannot be combined with a diacritic, the system generates two WM_CHAR messages", the diacritic and then the character ([Dead-Character Messages](https://learn.microsoft.com/windows/win32/inputdev/about-keyboard-input#dead-character-messages)). winit 0.30.13 puts both in `KeyEvent::text` and says so (`src/event.rs:575`), and for more than one grapheme it sets the logical key to the key without the dead key (`src/platform_impl/windows/keyboard.rs:613`). A dead key followed by Space produces the accent itself as the one `WM_CHAR`, while the logical key stays `Named(Space)` (`keyboard_layout.rs:790`).

**The failure.** US-International layout (or German with `^` and `` ` ``, Spanish, Portuguese, French), focus in a text field:

| typed | `text` from Windows | `key_names` passes | field gets | expected |
|---|---|---|---|---|
| `'` then Space | `'` | `"Space"` | a space | `'` |
| `'` then `t` | `'t` | `"t"` | `t` | `'t` |
| `^` then Space (German) | `^` | `"Space"` | a space | `^` |

So `don't` comes out as `dont`, and the usual sequences for the apostrophe, the double quote, the caret, the backtick and the tilde on these layouts type something else. A dead key that does combine (`'` then `e`) works: winit's logical key is then `é`.

**Verification.** The presenter's half was run on macOS, in a temporary test in `host/linux/src/presenter/field_tests.rs` that makes the calls `window.rs:1306` makes for these events, after a field held `h`:

```rust
p.hardware_key("Quote", "Unidentified", true, false); // the dead key
p.hardware_key("Quote", "Unidentified", false, false);
p.hardware_key("Space", "Space", true, false);
p.hardware_key("Space", "Space", false, false);
```

It printed `"h "`; the dead key and then `hardware_key("KeyT", "t", …)` printed `"h t"`, and then `hardware_key("KeyE", "é", …)` printed `"h té"`. What winit puts in `logical_key` and `text` is read from its source, and was not run: no Windows machine was available. The check to make there: US-International layout, Shelf's search field, `'` then Space, then `'` then `t`.

**Direction.** For a pressed key whose `event.text` is present and is not the logical key's own text, type `event.text` and keep the logical key as the `key` the app's handlers hear. `Key::Dead` should be named `"Dead"`, the DOM's name ([UI Events KeyboardEvent key Values](https://www.w3.org/TR/uievents-key/#keys-composition)), not `"Unidentified"`.

**Related.** None in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`. LLP 1115.004's table lists "`hardware_key` with winit's logical key text" as the host's state before D7; D7's painted field, the default today, still takes its text this way. AltGr's dropped characters are a separate defect in `host/windows/src/keyboard.rs`.
