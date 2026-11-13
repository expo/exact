# A footnote inserted at the end of a source that ends in a blank line has no definition

**Status:** Closed
**Resolution:** When the reference goes at the end of the source, the footnote command puts a blank line between it and the definition, so the definition starts its own paragraph.
**Systems:** markdown/src/edit.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`footnote` inserts the reference at the selection's end and the definition at the end of the
source, after a gap that makes the definition a paragraph of its own
(`markdown/src/edit.rs:485` to `:510` at `dd6f5dcd`). The gap is chosen from how the old
source ends (`:495`). When the caret is at the end of the source, the reference is inserted
there first, so the definition follows the reference, not the old ending. If the old source
ends in `\n\n` the gap is empty and the two land on one line.

## The failure

```rust
edit("Text\n\n", Range::caret(6), Command::Footnote).apply("Text\n\n")
// "Text\n\n[^1][^1]: "
```

A definition must start its line (`block::footnote_def`, `markdown/src/block.rs:212`), so the
styler reads two references to `1` and no definition. Expected `Text\n\n[^1]\n\n[^1]: `. The
caret is left after `[^1]: `, where what the person types becomes body text.

A person reaches it by typing a paragraph, pressing Return twice and choosing the footnote
command.

## Verification

In the web editor in headless Chromium (mounted as `host/web/tests/markup-editor.test.mjs`
mounts it): typed `Text`, pressed Return twice and ran `format("footnote")`. The source was
`Text\n\n[^1][^1]: ` and the line showed `[^1][^1]: ` as text; typing `note` gave
`[^1][^1]: note`. A temporary test of `exact_markdown::edit` gave the same for `"Text\n\n"`
and `"Text\n\nMore\n\n"` with the caret at the end, and the styler's footnote had two
references and no definition. With the caret elsewhere, or a source ending in `Text` or
`Text\n`, the definition is on its own line and is found.

## Direction

When the reference is inserted at the end of the source, choose the gap as for a source that
does not end in a newline.

## Related

`issues/closed/20260924-markdown-footnote-index-quadratic.md` is about numbering cost. Not
otherwise in `issues/` or `QUEUE.md`.
