# The terminal's cell policy gives a zero-width format character a column and a preserved tab only one

**Status:** Closed
**Resolution:** A cluster of width 0 under UAX #11 takes no column and is not drawn, in text and in a field, and a preserved tab takes the columns to the next stop of eight in measurement, wrapping and the preserved painter.
**Systems:** host/terminal (measure.rs, paint.rs)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`measure::cell` (`host/terminal/src/measure.rs:315-331`, at `dd6f5dcd`) decides how many columns each grapheme cluster takes, for the measurer and the painter alike. Two of its arms are wrong.

**A zero-width cluster takes one column.** The last arm is `cluster.width().max(1)`, and the field painter's `clusters` repeats it (`host/terminal/src/paint.rs:865-877`). A format character (category Cf: U+FEFF, U+200E, U+200F, U+2060, U+202A to U+202E) is a cluster of its own under UAX #29, is not `char::is_control` (Cc only) and has width 0, so it gets a grid cell holding that character. A terminal advances no column for it. The writer positions once per changed run and then emits cells in order (`host/terminal/src/grid.rs:380-395`), so the rest of the row lands one column to the left.

**A preserved tab is one space.** The whitespace arm (`measure.rs:320-323`) maps every whitespace cluster to one column. That is right for collapsing text. Under `white-space: pre` and `pre-wrap` LLP 1001 holds every host to CSS: "A preserved tab stops every eight spaces (`tab-size`'s initial value; no row sets another)". The Linux host's `pre_keeps_spaces_and_tabs_and_breaks_only_at_line_feeds` (`host/linux/src/text/css_tests.rs:149`) pins it there.

## The failure

```contract
component App
  view
    column width="8ch" border="thin solid"
      text "a\u{feff}b"
```

On row 1 the right border `│` is at grid column 9 and at column 8 of the terminal. A file that starts with a byte order mark, or a name carrying a direction mark, brings these into text the app did not write. An `input` whose value holds the character shifts its row the same way.

```contract
component App
  view
    column
      text "a\tb\n\tc" white-space="pre"
```

The grid reads `a b` and ` c`. Expected: `b` and `c` at column 8. Tab-indented code and tab-aligned output lose their indentation. The coding harness does not show it only because its data module expands tabs before the text reaches the host (`apps/harness/data/src/highlight.rs:136`).

## Verification

A temporary test compiled each source with `contract::compile_terminal`, painted it full screen, wrote `grid.diff(None)` into a `vt100` parser (the writer's oracle, LLP 1101 §5) and printed the column of the last `│` on row 1:

```
a c b:        grid column Some(9), emulator column Some(9)
a U+FEFF b:   grid column Some(9), emulator column Some(8)
a U+200E b:   grid column Some(9), emulator column Some(8)
a U+2060 b:   grid column Some(9), emulator column Some(8)
a U+202E b:   grid column Some(9), emulator column Some(8)
a U+200B b:   grid column Some(9), emulator column Some(9)
input value a U+FEFF b: grid column Some(9), emulator column Some(8)
```

U+200B is there for comparison: `cell` already gives it no columns. The tab text painted as `a b` and ` c` under `pre` and under `pre-wrap`. No real terminal and no browser was run.

## Direction

In `cell` and `clusters`, a cluster of width 0 takes no columns and is not drawn, as U+00AD and U+200B already are. Where the paragraph preserves white space, give a tab `8 - (col % 8)` columns in `Wrapped::cluster`, `Wrapped::line_end` and `visit_preserved`, which know the line's column; `cell` does not.

## Related

`issues/closed/20261006-terminal-nonbreaking-spaces.md` changed the whitespace arm for non-breaking spaces. It left tabs as they were, and it took U+FEFF out of that arm without settling its width.
