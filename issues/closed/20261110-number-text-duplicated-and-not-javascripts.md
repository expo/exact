# `number_text` and `js_number` are one function in two crates, each with a branch that cannot run, and differ from the `String(v)` the canvas recorder writes

**Status:** Closed
**Resolution:** Both copies are deleted, and every caller writes number text with `exact_num::js`, which is JavaScript's `String(n)` for every value.
**Systems:** color/src/lib.rs, motion/src/color/css.rs, canvas/src/font.rs, canvas/recorder.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`exact_color::number_text` (`color/src/lib.rs:690` at `dd6f5dcd`) and
`exact_motion::color::css::js_number` (`motion/src/color/css.rs:146`) have the same body, and
each doc begins "A number as JavaScript's `String(n)` writes it", the first "for finite,
moderate values", the second for "finite, not huge" ones.

```rust
if v == v.trunc() && v.abs() < 1e15 { return format!("{}", v as i64); }
let mut s = format!("{v}");
if s.contains('e') { s = format!("{v:.6}"); }
s
```

Rust's `{}` never writes an `f64` with an exponent, so the third line cannot run. The result
is `String(n)` only where JavaScript writes plain decimals: it differs for a nonzero
magnitude below 1e-6 and from 1e21 up.

The JavaScript half of the canvas recorder writes the same texts with `String(v)`
(`canvas/recorder.js:246`: a colour's components at `:200`, a font's size at `:340`), so the
two halves differ for those values.

`exact_num::js` (`num/src/js.rs:15`) is `String(n)`. `exact-color`, `exact-motion` and
`exact-canvas` already depend on `exact-num`, and the last two on `exact-color`.

Callers of `number_text`: `color/src/lib.rs:265` and `:545` (`Wide::css`),
`kernel/src/gradient.rs:756`, `kernel/src/style/profiled.rs:100`, `tamber/src/color_fn.rs:360`
and `:425`, `tamber/src/ir.rs:14` and `:15`. Callers of `js_number`: `canvas/src/font.rs:140`
and `canvas/src/context/text.rs:28`, through the re-export at `canvas/src/font.rs:20`.
`exact-motion` does not call its own copy, and no Swift, Kotlin, JavaScript or C source names
either.

## The failure

```rust
exact_color::number_text(1e-7)    // "0.0000001"
exact_num::js(1e-7)               // "1e-7", as String(1e-7)
exact_color::number_text(1.5e21)  // "1500000000000000000000"; String(1.5e21) is "1.5e+21"
```

## Verification

A temporary test of `exact-color` printed the two `number_text` lines; `num/src/js.rs`'s own
test pins `js(1e-7)`. Then, as a trial, `js_number` was deleted, `canvas/src/font.rs:20`
re-exported `exact_color::number_text` under that name, and `number_text` returned
`exact_num::js(v)`: `cargo check -p exact-color -p exact-motion -p exact-canvas -p exact-tamber
--all-targets` built, those four crates' tests passed, and the kernel's gradient and wide
colour unit tests passed (`cargo test -p exact-kernel --lib -- gradient profiled wide`, 19).
The trial was reverted. That the branch is dead is by reading: `Display` for `f64` is decimal
for every value.

## Direction

Make that trial the change: one function, which calls `exact_num::js`.

## Related

Not in `issues/` or `QUEUE.md`.
