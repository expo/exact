# Keyframes convert modern colors to legacy sRGB before interpolation

**Status:** Open
**Systems:** motion
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, a keyframe authored with `color()`, `oklab()`, `oklch()`,
`lab()` or `lch()` loses its color space before the animation is sampled.
`motion/src/animation/parse.rs:709-718` sends colors through
`motion::color::parse`. That function treats `Parsed::Wide` exactly like a
legacy color and constructs `Value::rgba` from 8-bit sRGB channels
(`motion/src/color.rs:16-22`). The canvas-oriented parser has already clipped
the wide color with `w.srgb8()` (`motion/src/color/css.rs:137-141`).

This changes both the animation's intermediate colors and any colors outside
sRGB. Modern colors should interpolate in Oklab, retaining their values until
presentation. The ordinary style-transition path does preserve this distinction
(`kernel/src/motion.rs:284-304`), and `Value` already supports it. The keyframe
path instead emits legacy `rgba()` when serialized back to CSS, so the browser
cannot recover the intended interpolation either.

**Reproduction.** A small Rust executable linked the existing `exact-motion`
artifact, called `color::css::link_wide()`, and ran:

```rust
let frames = Keyframes::parse(
    "from{color:color(srgb 1 0 0)}to{color:color(srgb 0 0 1)}"
).unwrap();
let mut animations = Animations::parse("fade 1s linear both").unwrap();
animations.resolve(|_| Some(&frames));
let midpoint = animations.0[0].sample(0.5, Property::Color, Value::ZERO).unwrap();
```

Observed: `midpoint.oklab == false`, with RGBA `[128, 0, 128, 255]`.
Interpolating the same endpoints through the library's existing
`Value::to_oklab()` gives `[140, 83, 162, 255]`. `frames.css()` has already
rewritten the authored colors as `rgba(255, 0, 0, 1)` and
`rgba(0, 0, 255, 1)`. This is a pure parser/sampler reproduction; no graphical
host was launched. The probe and output are at
`/tmp/exact-review-20261112/motion-parsing-review.rs` and
`/tmp/exact-review-20261112/motion-parsing-review.out`.

[CSS Color 4's interpolation rules](https://www.w3.org/TR/css-color-4/#interpolation-space)
distinguish modern colors from legacy sRGB, and require interpolation without
clipping inputs. This also agrees with the repository's Oklab transition path
and `motion/tests/it/paint.rs`'s modern-color test.

**Direction.** Preserve the modern color's unclipped value and encoding through
keyframe parsing and CSS serialization. Keep the canvas getter's sRGB conversion
at its own boundary. Validate an animation using modern red and blue, an
out-of-sRGB endpoint, a mixed legacy/modern pair, and a CSS serialization round
trip; compare native sampling with the browser's equivalent authored rule.

**Duplicates checked.** Searched open and closed filesystem reports and all
275 cached GitHub issues for keyframe color-space loss, Oklab, wide-color
clipping and the parser paths. No matching report was found. The closed number
formatting report covering `motion/src/color/css.rs` addresses serialization
of numbers, not color-space loss.
