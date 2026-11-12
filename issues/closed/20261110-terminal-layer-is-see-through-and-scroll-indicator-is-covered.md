# The terminal painter shows the page through an open dialog that has no background, and paints a scroller's indicator under its rows

**Status:** Closed
**Resolution:** An open layer whose author wrote no `background-color` blanks its cells to the terminal's default background before its border and children, and a scroller's indicator is painted after its children, inside the same clip.
**Systems:** host/terminal (paint.rs)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`node` (`host/terminal/src/paint.rs:172-319`, at `dd6f5dcd`) paints a box and then its children. In two places the order leaves the wrong thing on top.

**An open dialog or popover with no `background-color` does not cover the page.** `paint` fades the page and paints each open layer through `node` (`paint.rs:89-101`). `node` fills a box's cells only when the box has a `background_color` (`paint.rs:213-220`), and a `dialog` has none unless the author writes one. The layer's glyphs replace the cells they land on; every other cell inside it keeps the page's text. On the other hosts an unstyled `dialog` gets the platform's opaque surface (`docs/contract-for-agents.md`: a sheet on the Mac, an alert on iOS, the web's modal `dialog`, whose user-agent style is `background-color: Canvas`), and LLP 1101 §4 has the top layer "painted in order over the grid".

**A scroller's indicator is painted before its children.** LLP 1101 §4 gives `overflow` "a one-column indicator unless `scrollbar-width: none`". `node` draws it in the scroller's last inner column (`paint.rs:286-309`) and then paints the children (`paint.rs:311-315`). Layout reserves no column for it, so a child that reaches the last column replaces it.

## The failure

```contract
component App
  view
    column width="100%" height="100%"
      text "aaaaaaaaaaaaaaaaaaaaaaaa"
      text "bbbbbbbbbbbbbbbbbbbbbbbb"
      text "cccccccccccccccccccccccc"
      text "dddddddddddddddddddddddd"
      button commandfor="d" command="show-modal" testId="open"
        text "open"
      dialog id="d" width="10ch" height="2lh"
        text "hi"
```

After pressing `open` on a 24×6 screen, all that shows of the ten by two dialog is `hi`:

```
bbbbbbbbbbbbbbbbbbbbbbbb
ccccccchiccccccccccccccc
dddddddddddddddddddddddd
```

With `border="thin solid"` the frame is drawn and the page's `c` and `d` fill it. A `popover="auto"` box does the same. Every dialog in the repository's terminal apps writes a `background-color`, so none shows it.

```contract
component App
  view
    scroll width="20ch" height="4lh"
      button width="100%"
        text "choice 0"
      button width="100%"
        text "choice 1"
      // six more, "choice 2" to "choice 7"
```

Four rows of reverse video and no indicator: nothing says four more choices are below. A full-width row with a `background-color` removes the indicator's cell on its own row, and `text "0123456789"` rows in a `10ch` scroller remove all of it.

## Verification

Temporary tests compiled each source with `contract::compile_terminal`, booted it full screen and printed the grid. For the dialog they also printed the screen a `vt100` parser makes of the writer's bytes; the rows above are that output, alike in both. The scroller of eight buttons printed four rows of `choice N` with no `┃` or `│`; a scroller of eight `row width="100%"` boxes, the second with a background, printed:

```
row 0              ┃
row 1
row 2              │
row 3              │
```

## Direction

For a layer (`layer == true`) with no background, clear its cells to the terminal's default background before its border and children; `Grid::fill` takes a colour today and needs a form with none. For the indicator, reserve its column in layout when the scroller overflows, as the border rule reserves a border's cell, or paint it after the children inside the same `child_clip`; painting it last hides a glyph that reaches the last column.

## Related

LLP 1101.002 P7 gives a terminal entry's bare field a user-agent sheet and its Q2 asks about buttons; neither mentions a dialog's surface. The host's own tests strip `│` and `┃` from the end of a row before comparing (`host/terminal/tests/it/reading.rs:62`, `collections.rs:265`), which holds only where the row leaves the last column free.
