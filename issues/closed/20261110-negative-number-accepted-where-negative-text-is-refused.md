# `font-size=-4` is accepted where `font-size="-4px"` is refused: three lists of rows with no negative values disagree

**Status:** Closed
**Resolution:** `calc::nonnegative_row` is the one list of rows CSS gives no negative value, now with `font-size`, `line-height`, the radii, `border-spacing`, `shape-margin` and `stroke-width`, and a number, a `px`, `rem`, `em`, `ch` or `lh` text and a math function's clamp all read it, so `font-size=-4` is refused as `font-size="-4px"` is.
**Systems:** kernel/src/style/relative.rs, kernel/src/style/calc/parse.rs, kernel/src/style/calc/mod.rs, kernel/src/style.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

Which rows CSS gives no negative value is written three times, and each list is read
by a different spelling of the value:

- `relative::nonnegative` (`kernel/src/style/relative.rs:290`): `px`, `rem` and `em`
  text on an `f32` row. It has `FontSize`, `LineHeight`, the gaps, border widths,
  radii, `ShapeMargin` and hit slop; `StrokeWidth` by a special case at
  `relative.rs:423`.
- `calc::nonnegative_row` (`kernel/src/style/calc/parse.rs:720`): a number on an `f32`
  row (`StyleValue::f32`, `kernel/src/style.rs:397`) and a literal on a dimension row.
  It has the sizes, padding, flex factors, gaps, border widths, `ColumnRuleWidth` and
  hit slop.
- `calc::no_negative_lengths` (`kernel/src/style/calc/mod.rs:481`): a math function's
  clamp. It has the sizes, padding, radii, `R`, `Rx`, `Ry` and hit slop.

A row on one list and not another takes a negative value in one spelling and refuses
it in the next.

## The failure

Each row set through `StyleProps::set_dynamic`:

```
row               number -5   "-5px"     calc(-5px)
FontSize          -5          refused    (takes no calc)
StrokeWidth       -5          refused    (takes no calc)
BorderSpacing     -5          (takes no px)
ScrollPaddingTop  -5          -5         -5
R                 -5          -5         0
```

CSS refuses a negative on all five (`font-size`: CSS Fonts 4 §2.5; `border-spacing`:
CSS 2.1 §17.6.1; `scroll-padding`: CSS Scroll Snap 1 §4.2; `stroke-width` and `r`:
SVG 2), so a browser drops the declaration, and clamps a math function to 0.

In Contract these compile with no diagnostic:

```
text "a" font-size=-4
table border-spacing=-4
column scroll-padding-top=-4
svg width=20 height=20
  circle r=-4 stroke-width=-2
```

and `font-size="-4px"` is refused ("expected a nonnegative length"). A `font-size`
bound to a number that goes negative at run time reaches the same row. On the Linux
host the text's frame is then 1 px tall and draws no ink. Under the kernel's
monospace measurer `Kernel::compute_layout` returns
`Err(Layout(InvalidTextMetrics(2)))` and every frame of the tree stays 0 by 0.

## Verification

A temporary kernel test printed the table and the layout error. `contract build`
compiled the lines above and refused `font-size="-4px"`. A temporary test in
`host/linux/tests/pinned/paint.rs` painted a `text` with `font-size=-16` below another
line of text: its frame was 200 by 1 where `font-size=16` gave 200 by 19, and the
window held 133 dark pixels (the other line's) against 235. In headless Chrome 156
`CSS.supports` is `false` for `font-size: -4px`, `stroke-width: -5px`, `r: -5px`,
`scroll-padding-top: -4px` and `border-spacing: -2px`. Not run: the Apple hosts with a
negative size.

## Direction

Keep one list: a row's sign rule beside its codec (a schema flag, as `admitsAuto` and
`refusesPercent` are, or one function), read by `StyleValue::f32`, `relative::of`,
`pixels_text` and `calc::signed` alike.

## Related

LLP 1053 G3 (cited at `style.rs:395`) made flex factors, gaps and border widths refuse
a negative number; `font-size` was not among them. No issue or QUEUE entry names a
negative `font-size`, `border-spacing` or `scroll-padding`.
`issues/20261110-negative-scroll-padding-admitted.md` covers `scroll-padding`, which is on none of the three lists.
