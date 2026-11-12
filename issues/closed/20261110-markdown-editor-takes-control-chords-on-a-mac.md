# On a Mac, Control+A, B, K and Y in a web Markdown editor select all, bold, ask for a link and redo

**Status:** Closed
**Resolution:** The web Markdown editor's command key is ⌘ on an Apple platform and Control elsewhere, ⌃K deletes to the line's end (at the end, the line break), and ⌃Y inserts what ⌃K deleted, so on a Mac a Control chord does what it does in a plain field.
**Systems:** host/web/markup-editor.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at `dd6f5dcd`.

The editor's `keydown` handler takes either modifier as the command key on every platform
(`host/web/markup-editor.js:623`):

```js
const mod = e.metaKey || e.ctrlKey, key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
```

and then handles `a` (Select All), `z` and `y` (undo, redo), `b`, `i`, `]`, `[`, `k` (the link prompt) and
Shift+`c` (`:625-638`), cancelling the key. On a Mac the command key is ⌘, and Control with a letter is the
text system's own binding in every text field, a browser's included: `^a` moves to the start of the paragraph,
`^b` moves back, `^k` deletes to the end of the paragraph, `^y` yanks it back (Apple, "Text System Defaults and
Key Bindings"; AppKit's `StandardKeyBinding.dict`). The other five have no binding there. LLP 1045 names the
chords as ⌘B, ⌘I, ⌘K and ⌘⇧C on macOS and "Mod-Shift-C" on the web, and Mod is ⌘ on a Mac.

## The failure

`alpha beta gamma\nsecond line here` in a `textarea markup="markdown"` and in a plain `textarea`, the same keys
to each, in the pinned Chromium on macOS:

| Keys | Plain field | Markdown editor |
| --- | --- | --- |
| End, Control+A, `x` | `xalpha beta gamma…` | the whole text becomes `x` |
| select `beta`, Control+B | caret before `beta` | `alpha **beta** gamma…` |
| five moves right, Control+K | `alpha\nsecond…` (`deleteHardLineForward`) | nothing; the prompt "Link URL" opens |
| the same, then Home, Control+Y | ` beta gammaalpha…` (`insertFromYank`) | `historyRedo` |
| type `q`, Control+Z | `qalpha…` | `q` is undone |
| select `beta`, Control+I | unchanged | `alpha *beta* gamma…` |

In each editor row the `keydown` is cancelled. Control+E, F, N, D, H, T and O, which the handler does not name,
match the plain field.

Expected: on a Mac the editor's chords are ⌘ chords only, and a Control chord does what it does in the plain
field beside it.

## Verification

Run in the pinned headless Chromium (156) on macOS 27 against the real `markup-editor.js` and its wasm. Keys
reach a headless browser over the DevTools protocol, so Playwright attaches the editing command AppKit binds
to each (`^a` `moveToBeginningOfParagraph:`, `^b` `moveBackward:`, `^k` `deleteToEndOfParagraph:`, `^y`
`yank:`), as Chrome does for a real keyboard; this Mac's `StandardKeyBinding.dict` lists the same four. Not
run: a windowed Chrome, another browser, or Windows and Linux, where Control is the command key and the
handler is right.

## Direction

Take `mod` as `e.metaKey` on an Apple platform (`host/web/textflow-glue.js:363` tests `navigator.platform`) and
`e.ctrlKey` elsewhere. Run with that change, every row above matches the plain field but two: Control+K
deletes ` beta gamma\nsecon` (Chromium's target range for `deleteHardLineForward` runs to the same offset in
the next line, and the editor applies it, `:595-596`), and Control+Y inserts nothing (`insertFromYank` arrives
with empty data: the browser's own delete, which fills its kill buffer, was cancelled). Map those two inputs
in the editor: delete to the line's end, and keep what was deleted for the yank.

## Related

`issues/20261110-focused-paragraph-takes-every-command-a-and-c-chord.md` is the Mac host's reader and ⌘ chords.
`host/web/list-selection.js:110` reads the modifiers the same way for a list's Select All, outside a text field.
