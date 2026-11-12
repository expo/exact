# A text shadow is painted over the text's underline, and the underline casts none

**Status:** Closed
**Resolution:** Decorations cast their declaring box's shadow in sibling order, below that box's text and decorations.
**Systems:** host/linux/src/paint/fragments.rs, host/linux/src/paint/text_shadow.rs, host/linux/src/paint/text_decoration.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

A `Text` node paints its decoration lines first and its glyphs after
(`host/linux/src/paint/fragments.rs:129` to `:136`): `text_decorations` fills the underline
and the line-through, then `text_paint` paints the shadow, the fill and the stroke
(`host/linux/src/paint/text_shadow.rs:91` to `:113`). The shadow is an image of the glyphs
alone (`shadow_pass`, `text_shadow.rs:168` to `:195`), composited after the lines.

CSS Text Decoration 3 §4 says "Each layer shadows the element's text and all its text
decorations (composited together)" and that the shadow is painted between the element's
border or background and "the element's text and text decoration"; §5.1 lists shadows
first, under the underlines. LLP 1077 D3 decides the same for this property: "The shadow
also falls under text decorations, as Chrome paints it." Here the order is decoration,
shadow, text, so:

- the glyphs' shadow covers the underline wherever it reaches it (a blurred shadow dims
  the whole line; a hard one cuts it at every stem and descender);
- the underline has no shadow of its own.

## The failure

```
component App
  view
    box width=400 height=400 background-color="#ffffff"
      text "HHHHHH" color="#000000" font-size=40 text-decoration-line="underline" text-shadow="0px 6px 0px #ff0000"
```

Black and red pixels per row, against Chromium 156
(`font: 40px sans-serif; text-decoration: underline; text-shadow: 0 6px 0 #ff0000`):

| | underline rows | below it |
| --- | --- | --- |
| painting host, CPU painter | rows 39 and 40: black 119, red 38 | no red row: the line casts no shadow |
| Chromium | rows 39 to 42: black 173, red 0 | rows 45 to 48: red 173, the line's shadow |

On this host the red stems of the shadow cross the black line; in Chromium the line is whole
and has its own red copy 6px below.

## Verification

A temporary test in `exact-linux`'s library booted the Contract with `PainterChoice::Cpu`,
`Gpu` and `Ink` and counted the black and red pixels of each row of the frame: on all three
the underline is rows 39 and 40 with 38 red pixels in it, and no row below row 41 holds
red. The Chromium rows are counted the same way from a screenshot. Not run: the Android
recorder; a field's decorations (`paint/field.rs:518` to `:532`), which have no shadow pass.

## Direction

Paint the underline inside `text_paint`, after each stretch's shadow pass, and draw the
lines into the shadow's island with the glyphs so they are shadowed too.

## Related

`issues/20261110-line-through-painted-under-glyphs.md` is the same call order seen from the
other side (a line-through belongs over the glyphs); its direction keeps the underline
before `text_paint`, which leaves this defect. QUEUE's "`text-shadow` in a decoration on
native hosts" is a field's decoration run. LLP 1001 declares this host's line metrics and
its one-box painting of like runs, not this order.
