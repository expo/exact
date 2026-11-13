# Unbinding a timeline leaves a running animation outside the frame scheduler

**Status:** Open
**Systems:** motion, host/linux
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, removing a drag, scroll or view timeline from an animation resumes
its local time but does not resume sampled frames when the engine's first-frame
mode is off. The animation stays at its last painted value until another change
happens to schedule that node again.

`motion/src/engine/timeline.rs:147-170` removes the binding, takes `play.hold` and
sets the new start. It calls `schedule_animations` only when `pending.is_some()`.
That condition is false for the default engine. Binding previously removed the
node from `animating` at line 254. With no named clock to join, nothing adds it
back. `next_change` reports no deadline, `quiescent` reports true, and `advance`
does not mark its animated properties dirty. The Linux host constructs the
default engine (`host/linux/src/host.rs:311`) and passes timeline changes through
`host/linux/src/height.rs:146-148`.

A standalone Rust probe linked the checkout's actual `exact_motion` library. It
started `fade 1s linear both`, bound it to a scroll timeline at 25%, consumed the
frame, removed the binding, and advanced 250 ms. The result was:

```text
after unbind: next=None, quiescent=true
after +250ms: sampled=0.5, frame=[]
```

The mathematical sample advances to 0.5, but the host receives no frame to paint
it. The probe is `/tmp/exact-review-20261112/timeline-unbind.rs`. No native window
was launched. Existing unbinding tests in `motion/tests/it/animation.rs` either
stop at the binding flag or sample once while the preceding timeline change
still has a dirty property; they do not establish continued frame demand.

Recompute animation scheduling whenever a timeline is removed, including when
there is no first-frame wait or named clock. Add a regression that drains the
bound frame before unbinding, then checks the next deadline and multiple later
frames. Cover the default engine, first-frame mode, paused animations and a
named-clock join.

Duplicate check: searched open and closed filesystem issues and all 275 cached
GitHub issue titles and bodies. GitHub #114 and #225 concern adding scroll and
view timeline support; neither covers resuming sampling after a binding ends.
