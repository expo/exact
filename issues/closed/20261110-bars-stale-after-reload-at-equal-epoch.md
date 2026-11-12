# After a dev reload the Android reader's bars keep the old tree's title and targets when the new kernel's epoch equals the old one's

**Status:** Closed
**Resolution:** `Presenter::replaced` makes the bars' tree state anew (`bars::State::replaced`: what they were read from, the scroller the top bar follows, the held and focused targets), and `restore_time` tells the new host the room the bars take, so a reload reads them from the new tree at any epoch.
**Systems:** host/linux/src/presenter/bars.rs, host/linux/src/presenter.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`sync_bars` (`host/linux/src/presenter/bars.rs:303`) decides whether to read the
shell's bars again from the kernel's epoch and whether the content has scrolled:

```rust
let read = (epoch, self.bar_scrolled());
let same = self.bars.seen.as_ref() == Some(&seen);
if same && self.bars.read == Some(read) {
    return;                                   // line 349
}
```

A reload (`reload_module`, `presenter.rs:612`) replaces the host and calls
`replaced()` (`presenter.rs:1386`). `replaced()` knows the hazard for the painter
(it clears `self.brush.paint_epoch` under the comment "A new kernel may have the same
epoch.") and does not clear `self.bars.read`. A kernel's epoch counts the commits that changed
something, so an app reloaded before anyone touched it boots to the epoch it had.
`sync_bars` then returns early and the top app bar and the navigation bar keep the
old plan: the old title and labels, and targets holding the old runner's view ids. It
lasts until a later commit moves the epoch.

Only a shell that shows bars is affected. Among the hosts that is Android's reader
(`serve_bars`, `canvas/surface.rs:53`), and there only the development loop reloads
(`apply_development`, `presenter/delivery.rs:106`): edit a header's title, save, and
the phone's bar does not change. A release build cannot reach it today.
`activate_update` (`presenter/delivery.rs:284`) calls the same `replaced()`, but only
`host/linux-update` attaches an update store, and the reader has none
(`host/android/README.md`: "No update store").

## The failure

A two-tab app whose home route has a `header` holding
`text "Home" role="heading" aria-level=1`, booted with `serve_bars` and a window. The
top bar's plan says `Home` at epoch 1. Reload the same source with the heading changed
to `"Start"`:

```rust
let next = APP.replace("text \"Home\" role", "text \"Start\" role");
p.reload(&crate::test_plan(&next), Nothing).unwrap();
// settle, fitting the window before each frame as the canvas host does
p.bar_plans().0.unwrap().title      // "Home"
// the tree's heading text           // "Start"
// p.host.kernel().epoch()           // 1, as before the reload
```

Expected: the bar says `Start`.

## Verification

A temporary test in `exact-linux`'s library ran the steps above with the CPU painter.
It printed `before reload: (top title, tabs, epoch, body top) = (Some("Home"), Some(2), 1, 64.0)`,
the same line after the reload, and `the tree's heading after reload: Some("Start")`.
The same test with two tab presses before the reload (epoch 3) printed
`after reload: title Some("Start") epoch 1`: the bar follows when the epochs differ.
Not run on a device.

## Direction

In `replaced()`, forget what the bars were read from (`self.bars.read`, with the
scroller and the held and focused targets), as it already forgets `paint_epoch`.

## Related

None in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`; `QUEUE.md`, "Android's
bars, what is owed" does not name a reload. `bars_tests.rs` has no reload.
