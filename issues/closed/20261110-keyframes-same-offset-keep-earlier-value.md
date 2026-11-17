# After its end, an animation with two keyframe blocks at 100% shows the first block's value

**Status:** Closed
**Resolution:** A property's track gives a keyframe without a timing function the animation's and merges keyframes at one offset whose timing functions then match, the later value winning, and at a progress of 1 the sampler presents the last keyframe at offset 1, as Chrome does.
**Systems:** motion/src/animation.rs, motion/src/animation/parse.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`Keyframes::grammar` merges a block into an earlier frame only when the offset and the
`animation-timing-function` both match (`motion/src/animation/parse.rs:301` at `dd6f5dcd`), as
CSS Animations 2 §3.2 groups blocks. Two blocks at one offset with different timing functions
stay two frames, each with its own value for a property both declare. Two things then differ
from CSS.

At a progress of 1 or more, `Animation::value_at` steps back past every frame at offset 1 and
interpolates to the first of them (`motion/src/animation.rs:500` to `:505`). Web Animations
§5.3.4 takes the last: "If iteration progress ≥ 1 and there is more than one keyframe … with
a computed keyframe offset of 1, add the last keyframe".

CSS Animations 2 compares the timing functions after giving a block that has none the
animation's own. The parser compares before any animation is known, so a block that names the
animation's own timing function is kept apart from a block that names none, where CSS merges
them and the later value wins.

## The failure

```rust
let frames = Keyframes::parse(
    "from{opacity:0}to{opacity:0.2;animation-timing-function:ease-in}to{opacity:0.9}")?;
// `k 1s linear forwards`
a.sample(0.5, Property::Opacity, base)   // 0.1, as Chrome
a.sample(1.0, Property::Opacity, base)   // 0.2; Chrome 0.9
a.sample(2.0, Property::Opacity, base)   // 0.2; Chrome 0.9
```

With `animation-timing-function:linear` in the first `to` block, the animation's own, Chrome
157 merges the two blocks and computes 0.45 at half way; Exact computes 0.1.

`frames.css()` writes both `100%` blocks, so the web host's browser shows Chrome's values and
every sampling host shows these.

## Verification

A temporary test of `exact-motion` printed 0.1, 0.1998, 0.2 and 0.2 at t = 0.5, 0.999, 1 and 2
for both rules. Headless Chrome 157, the same rules paused at negative delays, computed 0.1,
0.1998, 0.9 and 0.9 for `ease-in`, and 0.45, 0.8991, 0.9 and 0.9 for `linear`; its
`getKeyframes()` lists three frames for the first and two for the second. Two blocks at 0% or
at 50% with different timing functions agree with Chrome at every time tried. No Contract app
was run through a host.

## Direction

At a progress of 1 or more, present the last frame that holds the property. When a track is
built for an animation, treat a frame whose timing function equals the animation's as having
none, so it merges with its neighbour at that offset.

## Related

`issues/closed/20260927-keyframe-folder-exponential.md` concerns the constant folder. Nothing
in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md` covers repeated keyframe selectors.
