# A terminal entry's dashed or dotted border compiles and is drawn solid

**Status:** Closed
**Resolution:** The terminal painter draws each side in its own line style, as LLP 1101 §4 maps them (dashed `┄┆`, dotted `┈┊`, their heavy forms on a heavy border), keeping solid corners, which have no dashed or dotted forms.
**Systems:** host/terminal (paint.rs), the terminal profile
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

LLP 1101 §4 gives each admitted line style its cells: "Light `─│`, double `═║`, dashed `┄┆`, dotted `┈┊`". Its summary promises that what a terminal cannot draw is refused: "A refused row is an error you see while writing, never a silent degradation you discover when the app runs."

At `dd6f5dcd` the terminal profile admits the four `border_style_*` rows whole (`kernel/tables/schema.json:3504-3541`, `"terminal": "admit"`), and their enum has held `dotted` and `dashed` since `issues/closed/20261009-dotted-dashed-border-style.md` added them for the GUI painters. `border` in `host/terminal/src/paint.rs:399-479` never reads a side's style. It chooses a glyph set by width and radius alone (`paint.rs:435-444`): heavy `━┃`, or light `─│` with square or rounded corners. A dashed or dotted border is drawn as the solid light line, with no diagnostic.

## The failure

```contract
component App
  view
    column width="6ch" border="thin dashed"
      text "x"
```

Observed, for `dashed` and for `dotted`:

```
┌──────┐
│x     │
└──────┘
```

Expected, per §4: `┄` and `┆` for dashed, `┈` and `┊` for dotted. The longhands paint the same solid box: `border="thin solid" border-style="dashed"` and `border="thin solid" border-top-style="dotted"`.

## Verification

A temporary test compiled each form with `contract::compile_terminal`, booted it full screen and printed `host.frame().grid.text()`: the solid box above for `thin dashed`, `thin dotted`, the two longhand forms and `medium dashed`. `border="thin double"` is refused by the compiler for every profile ("CSS line style `double` in `border` is not implemented by native painters").

## Direction

Read each side's style in `border` and pick the glyphs §4 names; the corners have no dashed forms, so keep the light ones. If the terminal is not to draw them, refuse the two values under the terminal profile instead.

## Related

`issues/closed/20261009-dotted-dashed-border-style.md` admitted the two styles and painted them on AppKit, UIKit and Linux; the terminal painter was not part of it. `QUEUE.md`, "The terminal surface's next work", lists the `border` shorthand among the profile's admission holes: that is the profile not checking a shorthand's pieces (LLP 1101.000 C2), not the painter ignoring a style the profile admits.
