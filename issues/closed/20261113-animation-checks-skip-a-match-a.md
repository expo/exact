# Animation checks skip a match, a let, and a type ascription

**Status:** Closed
**Resolution:** The three animation checks walk every string a value can be, through a ternary, a `match`, a type ascription and a `let`, with the length checks' bound.
**Systems:** contract lower, SVG and animation
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13
**Related:** issues/20261110-compiler-walks-views-and-expressions-by-hand.md

## What is wrong

`check_animation_names`, `check_timeline_rows`, and `exit_ends` walk `Expr::Str` and `Expr::Ternary` only. `Match`, `Let`, and `Typed` hit `_ => Ok(())`.

A ternary arm that names a missing keyframe, animates `r` on something other than a `circle`, follows an `animation-timeline` with a layout row or an endless iteration, or is an infinite or paused `-exact-exit-animation` is refused. The same string in a `match` arm is compiled. The length checks in the same file already walk `match`, `let`, and a type ascription.

The timeline rule is compiler-only. A timeline-bound layout animation in a match arm runs, which is the case LLP 1057.003 Q1 refuses because it would lay the page out on every frame. An infinite exit is refused when the kernel applies it (`kernel/tests/it/presence.rs`, `an_exit_that_never_ends_is_refused_and_applies_nothing`), so the node leaves with no exit.

## Where

`contract/lower/src/svg.rs`: `check_animation_names`, `check_timeline_rows`, `exit_ends`.

## How to reproduce

```
keyframes spin
  to rotate=90deg
state on = true
view animation=(on ? "spin 1s linear" : "spin 1s linear infinite") animation-timeline=--drag
```

is `lower-timeline-endless`. The same two strings as the arms of `match on { some _: "spin 1s linear infinite" none: "spin 1s linear" }` compile.

## Direction

Walk `Match`, `Let` (the bound value when the body reads it), and `Typed` the way `Ternary` is walked, and apply the refusals that already exist for a string.
