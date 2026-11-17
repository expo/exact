# A link destination that crosses a line hides the next line

**Status:** Closed
**Resolution:** A line ending inside a parenthesized destination ends the link (CommonMark 0.31.2 §6.3, example 490), and a valid link whose closer still spans a line break (a title on the next line) keeps its syntax out of styled.hidden, as link_destinations already does.
**Systems:** markdown
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A parenthesized link destination is closed by the matching `)`, including one on a later line. The construct hides from the end of the label through that `)`. `lines` paints every unit inside `styled.hidden` as hidden.

`link_destinations` omits a closer that contains a newline, and its comment says a hidden newline would leave an empty line. The editor restyles with nothing revealed, so it uses `styled.hidden` instead. The next source line draws empty. An angle destination already rejects a raw newline.

## Where

`markdown/src/inline.rs`, `destination_end` and `construct`. `markdown/editor/src/view.rs`, `lines`. `markdown/editor/src/lib.rs`, `link_destinations`.

## How to reproduce

Write `[label](http://example.com` and press Return, then type `more)`. The second line is hidden. Reveal nothing. The line stays blank until the `)` is deleted.

## Direction

Do not close a parenthesized destination across a newline. Leave that closer out of `styled.hidden`, as `link_destinations` already leaves it shown.
