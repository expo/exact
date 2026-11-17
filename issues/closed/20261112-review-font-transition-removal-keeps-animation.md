# Removing a font transition leaves the running animation active

**Status:** Closed
**Resolution:** Fixed by 65605c3c3 (a running transition stops when its declaration no longer covers the property); runner/tests/it/font_axes.rs pins weight, stretch and a variation axis.
**Systems:** runner
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, changing a text node's `transition` to `none` during a
`font-weight` transition leaves its interpolated weight and frame demand
active. The font continues changing until the original transition ends.
This affects hosts using the runner's text-axis animation engine.

`prepare_text_axes` records only changes to the font rows themselves
(`runner/src/runner/text_axes.rs:186-216`), so a transition-only patch
supplies no rows to settle. Receipt processing removes the node from
`watched`, but only reschedules existing keyframe animations
(`runner/src/runner/text_axes.rs:265-287`). It never cancels the running
font transition or restores its target.

The following complete public-API probe starts a 400 ms transition, advances
100 ms, and removes the transition without changing the target weight:

```rust
use exact_kernel::{Kernel, StyleId};
use exact_runner::Runner;

fn weight(r: &Runner<()>) -> u16 {
    let key = r.kernel().find_by_test_id("title")[0];
    r.kernel().node_by_key(key).unwrap()
        .computed_row(StyleId::FontWeight, |s| s.font_weight)
}

fn main() {
    let source = r#"component App
  state bold = false
  state animate = true
  action toggle
    bold = true
  action stop
    animate = false
  view
    text "Library" testId="title" font-weight=(bold ? 700 : 300) transition=(animate ? "font-weight 400ms linear" : "none")
"#;
    let mut r = Runner::boot(contract::compile(source).unwrap(), (),
        Kernel::with_monospace(), Default::default(), "/").unwrap();
    r.act("toggle", vec![]).unwrap();
    r.advance(100.).unwrap();
    println!("before removal: weight={}, frames={}", weight(&r), r.wants_frames());
    r.act("stop", vec![]).unwrap();
    println!("after removal: weight={}, frames={}", weight(&r), r.wants_frames());
    r.advance(200.).unwrap();
    println!("100ms later: weight={}, frames={}", weight(&r), r.wants_frames());
}
```

Linked against the checkout's built libraries, it prints:

```text
before removal: weight=400, frames=true
after removal: weight=400, frames=true
100ms later: weight=500, frames=true
```

Expected after removal: `weight=700, frames=false`, unchanged by the next
advance. CSS requires cancellation when a running transition no longer has
a matching transition property, with the final value taking effect
immediately. See [CSS Transitions, starting transitions](https://www.w3.org/TR/css-transitions-1/#starting).

The probe and captured output are at
`/tmp/exact-review-20261112/font-transition-removal/{main.rs,repro,output.txt}`.
Reconcile existing font transitions when the transition row changes, and
cancel any property no longer covered while restoring its authored or
inherited target. Cover removal during a running transition in
`runner/tests/it/font_axes.rs`, including the other supported font rows.

Duplicate check: searched open and closed filesystem issues and the cached
GitHub issue titles and bodies for font transitions, text axes and removal
of transitions. No matching report was found.
