# First-frame hold state bugs in `motion::Engine`: leaked `self.pending` keys, `unpresentable` bypass on timeline unbind, and missed overdue clocks

**Status:** Closed
**Resolution:** Remove self.pending keys on hold takeover and path/filter cancellation, drop stale unshown keys in start_pending, respect unpresentable on timeline unbind, and iterate all pending clock origins in release_overdue.
**Systems:** motion engine first-frame holds, gestures, path/filter transitions & timelines
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Two sources of truth)

Verification: verified by reading.

At `9c0431fd5`, three coupled bugs break `exact_motion::Engine`'s first-frame hold state machine (LLP 1003.001 D9):

1. **Stale `self.pending` keys after hold takeover or path/filter cancellation:**
   - `Engine::begin_transform_hold_with_counter` (`motion/src/engine/hold.rs:146-154`) and `Engine::begin_hold_full` (`motion/src/engine/hold.rs:263-274`) clear `slot.set_running(None)` and remove `key` from `self.running` and `self.played`, but never call `self.pending.remove(&key)`.
   - `Engine::observe_track` (`motion/src/engine/path.rs:161-168` for `None`/non-finite values, and `motion/src/engine/path.rs:220-230` for non-interpolable or uncovered target changes) removes `key` from `self.running`, `self.slots` (and `self.played` at `:164` / `:203`), but omits `self.pending.remove(&key)` (unlike the unchanged-target cancellation branch at `motion/src/engine/path.rs:196`).
   - Because `Engine::at_rest` (`motion/src/engine.rs:440-469`, specifically `:467`) checks `none(&self.pending)`, a settled node retains a phantom pending key until `start_pending` drains it—and indefinitely while the node is in `self.unshown`, because `Engine::start_pending` (`motion/src/engine/first_frame.rs:255-258`) re-inserts `unshown` keys into `self.pending` before checking whether the slot still exists or has a pending curve.
2. **Timeline unbind ignores `self.unpresentable`:**
   - `Engine::set_animation_timeline_full` (`motion/src/engine/timeline.rs:150-159`, specifically `:151`) computes `let pending = self.start_on_frame.then_some(now);` instead of `self.holds().then_some(now)` (`self.start_on_frame && !self.unpresentable`, `motion/src/engine/first_frame.rs:35-37`).
3. **`release_overdue` inspects only the earliest pending clock origin:**
   - `Engine::release_overdue` (`motion/src/engine/first_frame.rs:194-216`, specifically `:209`) chains `self.clock_held_since()`, which returns only `self.clocks.pending.values().copied().reduce(f64::min)` (`motion/src/engine/clock.rs:256-258`), instead of all pending clock origins.

### Trigger
With `start_on_frame` enabled (the default on graphical hosts via `Host::hold_from_boot` / `exact_start_on_frame` / `m_start_on_frame`):
1. Start a `clip-path` (`d`) or `filter` transition (`observe_path` / `observe_filter`) and cancel it to `None` or a non-interpolable value before `present_frame`; or call `begin_hold` / `begin_transform_hold` on a pending transition and settle the hold at its target. Then renew the node through `MotionSync::apply_rows` (`kernel/src/motion.rs:186-193`) or query `engine.at_rest(node)`.
2. Call `engine.set_presentable(false)` (e.g. page hidden/frozen or surface lost via `host/painted/src/presenter/present.rs:74` -> `host/painted/src/first_frame.rs:104-116`, or `host/web-js/motion/src/lib.rs:83-87`), then unbind an animation from a scroll/view/drag timeline (`animation-timeline: auto` -> `MotionSync::apply_rows` at `kernel/src/motion.rs:240`, `host/apple/src/timeline_scroll.rs:28`, or `host/painted/src/height.rs:158`).
3. Create two pending `-exact-clock(...)` origins at `t1 = 0.0` and `t2 = 0.5` whose nodes have no overdue play at `t2` (for example, paused plays joining idle clocks per LLP 1003.001 D8 at `motion/src/engine/clock.rs:133-138, 163`, or a node removed/rebound after starting clock B), then call `engine.release_overdue(2.6)` (`t2 + HOLD_DEADLINE <= 2.6`).

### Actual vs. Expected
- **Actual:** (1) `engine.at_rest(node)` returns `false` for a fully settled node, causing `MotionSync::apply_rows` (`kernel/src/motion.rs:189-191`) to call `engine.remove(*node)` instead of keeping the renewed list row in place, discarding its settled baseline and marking all four properties dirty again. (2) Unbinding a timeline while unpresentable sets `play.pending = Some(now)` even though `set_presentable(false)` is active, freezing the animation until `HOLD_DEADLINE` (2 s) (and waking sleeping host loops via `held_deadline()`) or indefinitely if no present/overdue release runs. (3) `release_overdue(2.6)` computes `latest = Some(0.0)` and calls `release_held(0.0)`, releasing only clock A while leaving overdue clock B (`0.5`) held in `self.clocks.pending`.
- **Expected (LLP 1003.001 D9; `motion/src/engine/first_frame.rs:33-37, 74-77, 192-194`; `motion/src/engine.rs:430-439`):** (1) Cancelling or taking over a pending curve removes `key` from `self.pending`, and `start_pending` drops keys with no live pending curve even for `unshown` nodes. (2) No new hold is created while `self.unpresentable` is true. (3) `release_overdue(now)` releases every pending curve, play, and clock origin satisfying `begin + HOLD_DEADLINE <= now` in one pass.

### Verification & Siblings Checked
Verified by reading at `9c0431fd5`. Checked all other `self.pending` removal sites (`motion/src/engine.rs:489, 515, 657`, `motion/src/engine/played.rs:164`, `motion/src/engine/path.rs:196`: all remove `key` from `self.pending`; `motion/src/engine.rs:728-731` does not, though `Transition::starts()` at `motion/src/transition.rs:215-220` prevents zero-remaining easings from starting pending), all other hold-creation sites (`motion/src/engine.rs:551`, `motion/src/engine/path.rs:257`, `motion/src/engine/animate.rs:98, 337`, `motion/src/engine/clock.rs:134`, `motion/src/engine/hold.rs:469`: all respect `!self.unpresentable`), and `held_since` / `held_latest` (`motion/src/engine/first_frame.rs:163, 181`: min/max clock aggregations are appropriate there).

### Suggested Fix & Regression Test
1. Add `self.pending.remove(&key);` in `begin_transform_hold_with_counter` (`hold.rs:152`), `begin_hold_full` (`hold.rs:272`), and both cancellation branches of `observe_track` (`path.rs:165, 226`); in `start_pending` (`first_frame.rs:255-267`), check whether the slot still has a pending curve before re-inserting an `unshown` key.
2. In `set_animation_timeline_full` (`timeline.rs:151`), use `let pending = self.holds().then_some(now);`.
3. Expose an iterator over all pending clock begins (e.g. `self.clocks.pending.values().copied()`) and chain it into `release_overdue` (`first_frame.rs:209`).
4. Add unit tests in `motion/tests/it/first_frame.rs` covering: `at_rest` after cancelling pending path/filter transitions and after hold takeover/cancel (including with `set_unshown`); timeline unbind after `set_presentable(false)` asserting `play.pending.is_none()` and `held_since().is_none()`; and `release_overdue` with three pending clocks `(0.0, 0.5, 10.0)` at `now = 2.6` asserting both `0.0` and `0.5` are released while `10.0` stays pending.

### Siblings checked

Checked all self.pending removal sites (engine.rs:489, 515, 657, 728-731; played.rs:164; path.rs:196), all hold-creation sites (engine.rs:551; path.rs:257; animate.rs:98, 337; clock.rs:134; hold.rs:469), and clock hold queries (first_frame.rs:151-183; clock.rs:250-258), plus host callers in host/painted/src/first_frame.rs, host/web-js/motion/src/lib.rs, host/apple/src/timeline_scroll.rs, and host/painted/src/height.rs.
