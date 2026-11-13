# The steps() easing parser silently discards extra arguments

**Status:** Closed
**Resolution:** The shared easing parser refuses a steps() call with other than one or two arguments, so a third argument or a trailing comma is a parse error in transitions, animations and keyframes.
**Systems:** motion
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, `motion/src/parse.rs:220-240` reads a `steps()` call's first
argument as its count and second as its position, but never checks the
argument count. The shared parser therefore accepts `steps(4, end, extra)`
and `steps(4, end,)` as `steps(4, end)` in transitions, animation shorthands,
animation longhands and keyframe timing functions. An authored syntax error
silently becomes a different valid declaration.

[CSS Easing 1's steps() grammar](https://www.w3.org/TR/css-easing-1/#step-easing-functions)
permits a count and one optional position. Exact's own diagnostic describes
the same grammar; the `cubic-bezier()` and spring branches already enforce
their argument counts.

**Reproduction.** A small Rust executable linked the existing `exact-motion`
artifact and printed the results of `Transitions::parse` and
`Animations::parse`. All these returned `Ok`, with count 4 and `JumpEnd`:

```text
opacity 1s steps(4,end,extra)
opacity 1s steps(4,end,)
fade 1s steps(4,end,extra)
fade 1s steps(4,end,)
```

The valid `opacity 1s steps(4,end)` returned the same transition. Probe and
output: `/tmp/exact-review-20261112/motion-parsing-review.rs` and
`/tmp/exact-review-20261112/motion-parsing-review.out`. No browser was launched.

**Direction.** Require one or two arguments before parsing them. Add cases for
a third argument and trailing comma through the shared transition/animation
entry points, keeping valid one- and two-argument forms accepted.

**Duplicates checked.** Searched open and closed filesystem reports and all
275 cached GitHub issues for `steps()`, extra easing arguments and parser
arity. GitHub #49 covered diagnostics and ASCII case folding; it does not
report this missing argument-count check.
