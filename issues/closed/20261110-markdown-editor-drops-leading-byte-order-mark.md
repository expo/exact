# A web Markdown editor whose source starts with a byte order mark draws later lines one character short and corrupts the source on the first edit

**Status:** Closed
**Resolution:** The web editor decodes what it reads back from its wasm with `ignoreBOM`, so its copy of the source keeps a leading U+FEFF and agrees with every offset the editor reports.
**Systems:** host/web/markup-editor.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at `dd6f5dcd`.

The web editor keeps the Markdown source in `markup-editor.wasm` and reads it back as UTF-16 with
`new TextDecoder('utf-16le')` (`host/web/markup-editor.js:50`, `read` at `:220`, `text` at `:57`). A
`TextDecoder` made without `ignoreBOM: true` removes a leading U+FEFF from what it decodes (Encoding Standard,
`TextDecoder`, "ignore BOM"). So for a source that starts with U+FEFF the page's copy, `source`, is one code
unit shorter than the editor's, while every offset the editor returns (`mde_view`'s line and segment ranges,
`mde_sel`) still counts the mark. `source.slice` then cuts each line one unit late (`:194`), `el.value` omits
the mark (`:690`), and `reconcile` sends the drawn line's text for a range computed from the short copy
(`:492-496`).

Such a source reaches the editor from a file saved as "UTF-8 with BOM", which Windows tools write (Notepad's
option of that name, Windows PowerShell 5.1's `-Encoding utf8`), when a Rust source decodes it with
`String::from_utf8`, which keeps the mark (`apps/markdown/data/src/lib.rs:223` decodes a document so for its
reader). A TS source that decodes with `new TextDecoder().decode(bytes)` (`docs/start-here.md:261`) has
already lost it and is not affected. No app in `apps/` loads a file into an editor today.

## The failure

```js
textarea.value = '\ufeff# Title\nfirst line\nsecond line';   // then the editor mounts (markup="markdown")
// click the field, move to the end of the document, type "!"
```

Observed in the pinned Chromium, beside a plain `textarea` given the same:

```
each line's text as drawn:    "# Title\n", "irst line\n", "econd line"
the app's `input` hears:      "# Title\nfirst line\necond line!e"
plain textarea:               "\ufeff# Title\nfirst line\nsecond line!"
```

With `'\ufeffhello'` the field reads `hello` and the same edit gives `hello!o`. The same three lines without
the mark draw and edit correctly.

Expected: the lines as written, and `\ufeff# Title\nfirst line\nsecond line!` after the edit.

## Verification

Run in the pinned headless Chromium (156) against the real `markup-editor.js` and its wasm: the value set on
the `textarea` before the editor installs, a click, ⌘↓, one inserted character; it printed the lines above.
In Bun, `new TextDecoder('utf-16le').decode(new Uint16Array([0xfeff, 0x68, 0x69])).length` is 2, and 3 with
`{ ignoreBOM: true }`. Not run: the Apple editor, which shares the crate but not this decoding.

## Direction

Make the decoder with `{ ignoreBOM: true }`, so what is read back is what the editor holds.

## Related

`issues/20261110-byte-order-mark-refused-as-an-invisible-character.md` is the Contract compiler's reading of a
Contract source file, another file type. No issue, `QUEUE.md` entry or pitfall covers the editor's decoding.
