# Two colours exactly opposite in hue mix round the far side of the hue wheel

**Status:** Closed
**Resolution:** One `exact_color::hue_arc`, which the kernel's gradients and Forma's `color-mix()` share, leaves hues exactly 180° apart unadjusted under `longer` as well as `shorter`, `mix` takes a colour already in the mix space as written, and a pair with a converted end counts a difference within 1e-6° of ±180° as ±180°.
**Systems:** color/src/mix.rs, kernel/src/gradient.rs, forma/src/color_fn.rs
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, two rules decide the arc between hues 180° apart, and both miss CSS Color 4,
"Hue Interpolation" (§12.4; §13.6 in the draft of 3 November 2026), which LLP 1100 D2 adopts
("every CSS Color 4 §12 space and hue method").

`Longer`'s second arm takes −180 itself: `(-180.0..=0.0)` (`color/src/mix.rs:299`). CSS adjusts
only for `-180 < θ₂ - θ₁ <= 0`. Forma's `step` copies the bound on purpose
(`forma/src/color_fn.rs:746`).

`exact_color::mix` (`mix.rs:282`) reads each end through `endpoint` (`:265`), which takes the
colour to linear sRGB and back into the mix space even when it is written in that space. CSS
converts only "if required". The round trip leaves `lch(70 30 0)` at hue 359.99999999999994,
so hues written 180° apart differ by 180 ± ε, and the sign of ε picks the arc for `Shorter`
(`:296`, `:297`). Forma's `components` (`forma/src/color_fn.rs:287`) keeps such a colour as
written, so its `color-mix()` and the kernel's gradient (`kernel/src/gradient.rs:537`, the
only caller of `mix`) answer differently for one pair.

## The failure

The middle of each, and what Chrome 157 gives:

| input | Exact | Chrome's `color-mix()` | Chrome's drawn gradient |
| --- | --- | --- | --- |
| `linear-gradient(in hsl longer hue, cyan, red)` | `rgb(127, 0, 255)` | `rgb(128, 255, 0)` | `rgb(128, 255, 0)` |
| `linear-gradient(in hsl, #fff000, #000fff)` | `rgb(255, 0, 142)` | `rgb(0, 255, 112)` | `rgb(0, 255, 113)` |
| `linear-gradient(in lch, lch(70 30 0), lch(70 30 180))` | `rgb(146, 173, 225)`, hue 270 | `lch(70 30 90)` | `rgb(145, 172, 225)` |
| Forma `color-mix(in lch longer hue, lch(70 30 180), lch(70 30 0))` | `lch(70 30 270)` | `lch(70 30 90)` | |

`blue, yellow` and `magenta, lime` under `longer hue` fail as the first row does.

Chrome's drawn gradients are no oracle for the third row: it converts stops through sRGB
floats and has the same instability. Its `color-mix()` follows the specification.

## Verification

Sampled `BackgroundImage::check(css)` through `Gradient::resolved` in a temporary kernel test,
and folded the Forma line with `exact_forma::check`. For hues 0, 5, … 175 against the hue 180
further on, both orders, in `lch`, `oklch` and `hsl`, `shorter` and `longer` (432 pairs),
`mix` is a half turn from Chrome's `color-mix()` on 193, and Chrome's `color-mix()` is the
plain average on all 432. Chrome's drawn gradient, read from a headless screenshot, is a half
turn from its own `color-mix()` on 180, and Exact's gradient matches the drawn one on 225. No
host was run.

## Direction

Make `Longer`'s lower bound exclusive in `mix` and in Forma's copy. Take an end's components
as written when its space is the mix space, as Forma does; for an end that had to be
converted, treat a difference within a rounding of ±180 as 180.

## Related

No issue or QUEUE entry names hue interpolation. `color/src/tests.rs:499` tests hues 10 and
350 only.
