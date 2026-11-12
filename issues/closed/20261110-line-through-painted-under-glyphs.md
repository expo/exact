# The painting host draws `line-through` under the glyphs it strikes

**Status:** Closed
**Resolution:** A strike paints above its declaring box's glyphs and shadows, before later siblings.
**Systems:** host/linux/src/paint/fragments.rs, host/linux/src/paint/text_decoration.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

CSS paints a text's decorations in a fixed order, bottom first: shadows, underlines,
overlines, the text, emphasis marks, then line-throughs (CSS Text Decoration 3 §5.1,
"Painting Order of Text Decorations"). A line-through is over the glyphs.

`Painter::text_node` paints every decoration before the glyphs
(`host/linux/src/paint/fragments.rs:129`, `text_decorations`, then `roll_glyphs` or
`text_paint` at `:132` and `:135`), and `text_decorations` draws the underline and the
strike in one loop (`host/linux/src/paint/text_decoration.rs:81` and `:88`). The
glyphs then cover the strike wherever they cross it.

The two orders look the same when the line and the glyphs share one opaque colour.
They differ when the line comes from an ancestor with another `color`, which is how
CSS colours a decoration (the line takes the colour of the box that declares it), and
when the colour is translucent.

## The failure

```
text color="#ff0000" text-decoration-line="line-through" font-size=40
  text "HHHHHH" color="#000000"
```

A red line through black capitals. Counting red and black pixels in the rows the line
occupies:

```
painting host, CPU   row 25: red 119, black 36
painting host, GPU   row 25: red 117, black 37
painting host, Ink   row 25: red 119, black 36
web host, Chrome 156 rows 24 to 27: red 170, black 0
```

On the painting host the twelve stems of the six letters cut the line. Chrome paints an
unbroken red line.

## Verification

A temporary unit test in `host/linux/src/paint/` booted the Contract above with each
of `PainterChoice::Cpu`, `Gpu` and `Ink` and counted pixels per row of the frame; the
first three lines are its output. The same Contract built for the web
(`bun host/web/build.mjs`) and screenshotted in Chromium 156 through
`scripts/agent.mjs web` gave the fourth; two bare `<span>`s in the same Chromium gave
red 173, black 0. The Android Canvas recorder takes the same order from the walk and
was not run.

## Direction

Draw the strikes after `text_paint` (or `roll_glyphs`) and keep the underlines where
they are: split the loop in `text_decorations` in two, or pass which line to draw.

## Related

`issues/closed/20261020-text-decoration-propagation.md` gave each line its decorating
box's colour, which is what makes the order visible; it did not change the order.
LLP 1001 declares the painting host's line metrics and the missing underline
`skip-ink`, not the order. Nothing in `QUEUE.md` or `docs/agent-pitfalls.md` covers it.
