# `border-spacing="2px"` is refused, `"0.5rem"` is refused as "no length", and a terminal entry can write no spacing but 0

**Status:** Closed
**Resolution:** `border-spacing` takes CSS lengths as `row-gap` does (`px`, `rem`, `em`, `ch`, `lh`, a terminal entry's cells among them) and refuses a negative one, and a row that takes `px` and no relative unit says to write px.
**Systems:** kernel/src/style/relative.rs, kernel/src/style/cells.rs, kernel/tables/schema.json, contract/cli/src/terminal.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`relative::admits_relative` (`kernel/src/style/relative.rs:283`) lists the `f32` rows
that take a CSS length as text: `px` (`pixels_text`), `rem` and `em` (`of`), `ch` and
`lh` (`cells::of`). `border_spacing` is a length row (its schema comment: "CSS 2.1
§17.6.1 `border-spacing` as one length") and is not on the list, so it takes only a
bare number and refuses every spelling CSS has.

The guide says otherwise (`docs/contract-for-agents.md:4998`): "A bare number on a
length row is pixels …, and the row takes CSS's spellings too (`font-size="14px"`, …)".
LLP 1102 §3.10 settled the same gap for `font-size` and `letter-spacing`: "It is an
inconsistency, and CSS takes both."

## The failure

```
table border-spacing="2px"      // expected number
table border-spacing="0.5rem"   // expected no length: `rem` and `em` are lengths
table border-spacing="1ch"      // expected no length: `ch` and `lh` are lengths
```

The second and third messages (`relative.rs:385`, `kernel/src/style/cells.rs:108`) say
the row takes no length. A length is all it takes. The `Perspective`, `StrokeWidth`
and `TextStrokeWidth` rows take `px` and get the same false sentence for `rem`
(`perspective="50rem"`: "expected no length: `rem` and `em` are lengths").

The schema marks `border_spacing` `"terminal": "cell"`
(`kernel/tables/schema.json:4548`): in a terminal entry its length is in cells. The
terminal profile enforces that (`contract/cli/src/terminal.rs:94`, `129`) and the
kernel refuses cells, so the two refusals point at each other:

```
border-spacing="1ch"   expected no length: `ch` and `lh` are lengths
border-spacing=8       a terminal length is in cells — `Nch` (columns), `Nlh` (rows), a percentage, `auto` or 0
border-spacing=0       compiles
```

Expected: `border-spacing` takes `px`, `rem`, `em`, `ch` and `lh` as `row-gap` does,
and a row that takes `px` but not `rem` says that `rem` is not built there.

## Verification

`contract build` printed the three refusals above and the `perspective` one, and
compiled `border-spacing=2`. A temporary test calling `contract::compile_terminal` on
a `table` with each attribute printed the three terminal lines. A temporary kernel
test printed the same refusals from `StyleProps::set_dynamic` for `BorderSpacing`,
`Perspective`, `TextStrokeWidth` and `StrokeWidth`. In headless Chrome 156
`CSS.supports('border-spacing', v)` is `true` for `2px`, `0.5rem` and `1ch`, and
`false` for `2`.

## Direction

Add `BorderSpacing` to `relative::nonnegative`, which admits the units and refuses a
negative one in the same place. Where a row takes `px` and no relative unit, have
`of` say "`rem` and `em` are not built on this row; write px".

## Related

`QUEUE.md:897` ("`rem`/`em` where the kernel still takes only `px`") holds the missing
units on `stroke-width` and the SVG number rows; this is the wrong sentence there and
`border-spacing`, which it does not name. `QUEUE.md:48` ("A `table` takes no
`border-collapse`") covers `border-collapse` only.
