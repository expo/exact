# resize_at and set_view_at discard committed timer batches in geometry_refused, permanently desynchronizing presenter state

**Status:** Open
**Systems:** host/apple (resize, fold, mirror/batch synchronization)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Halfway failure)

Verification: verified by reading.

1. **Exact location**
At `9c0431fd5`, `Host::resize_at` (`host/apple/src/resize.rs:99-103`) and `Host::set_view_at` (`host/apple/src/fold.rs:285-305`) call `self.settle_resize_clock(Batch::new(), None)` (`host/apple/src/resize.rs:105-125`) before `Host::resize_inner` (`host/apple/src/fold.rs:125-158`) calls `self.runner.set_view(...)`. On any `Err` from `set_view`, `resize_inner` (`fold.rs:139-144`) calls `self.geometry_refused(...)` (`fold.rs:105-121`), which discards the caller's `batch` and allocates a fresh `Batch::new()` at `:112`.

2. **Reachable trigger**
- **Path 1 (`exact_resize`)**: `ExactSession.resize(_ size: CGSize)` (`host/apple/Sources/ExactKit/Session.swift:1384`) → `Bridge.resize` (`host/apple/Sources/ExactCore/Bridge.swift:977`) → `exact_resize` (`host/apple/src/abi/exports.rs:1040`) → `Bridge::resize` (`host/apple/src/abi.rs:1208-1218`) → `Host::resize_at`. Trigger when a timer that creates/destroys views, changes `roots`, or issues a router change/command is due at `live_clock()`, and either `width <= 0 || height <= 0 || !finite` (e.g. `ExactSession.resize(.zero)`) or an `exactViewport()` reader/derivation hits a runtime or budget error (such as `1 / (24 - viewport.width)`).
- **Path 2 (`exact_view`)**: `ExactSession` view update (`host/apple/Sources/ExactKit/Session.swift:1393`) → `Bridge.view` (`host/apple/Sources/ExactCore/Bridge.swift:1115`) → `exact_view` (`host/apple/src/abi/exports.rs:1126`) → `Bridge::view` (`host/apple/src/abi/control_text.rs:75-152`) → `Host::set_view_at` with `size.is_some()` when a timer is due at `now_ms` and `Runner::set_view` fails in an `exactViewport()` reader/derivation.

3. **Actual vs. expected result**
- **Actual**:
  - `settle_resize_clock` (`resize.rs:105-125`) advances timers (`self.runner.advance_timed(self.now_ms)`) and runs `self.commit_tree(&advanced.receipts, error, batch)` (`host/apple/src/commit.rs:52-237`), which mutates `runner`, `kernel`, `self.mirror`, `self.keys`, `self.roots`, and `self.engine`, persists store writes, drains `self.runner.take_router_change()` and `self.runner.take_commands()`, and appends all `create`, `destroy`, `children`, `props`, `style`, `roots`, `router`, `command`, `surface`, and `arrange` ops into `batch`.
  - When `self.runner.set_view` returns `Err(e)`, `resize_inner` (`fold.rs:139-144`) calls `self.geometry_refused(..., previous)`, dropping `batch` completely. `geometry_refused` (`fold.rs:112-120`) starts a fresh `Batch::new()`, runs `self.update(id, &mut batch)` (which diffs against the already-updated `self.mirror` and emits no `create`, `destroy`, `children`, `roots`, `router`, or `command` ops), and runs `self.layout(&mut batch)`. Swift never creates the new views (receiving orphan `frame` ops for nonexistent view IDs), permanently retains destroyed zombie views, and loses the drained router/command/arrange ops.
  - Additionally, `fold.rs:141` (`error.unwrap_or_else(|| format!("viewport: {e:?}"))`) silently drops the viewport refusal error if `settle_resize_clock` already recorded an error.
  - *Note on `set_segments` (disproved part of original report)*: The sibling `Host::set_segments` (`host/apple/src/fold.rs:343-391`) is **not** affected. After `kernel.set_segments(c, r, rects)` succeeds (`:358`), `Env::segments_consistent` (`kernel/src/style/env.rs:705-714`) guarantees `cols >= 1 && rows >= 1`, so `fold.validate()` (`runner/src/runner/viewport.rs:115`) cannot fail. Any failure in `Runner::set_fold` runs `previous.restore(&mut self.kernel)` inside `Runner::replace_viewport` (`runner/src/runner/viewport.rs:147-150`, `runner/src/viewport.rs:915-920`), which restores the kernel's previous segment grid before `geometry_refused` re-diffs `self.mirror`.
- **Expected**: `resize_at` should validate `(width, height)` before settling timers (matching `set_view_at` at `fold.rs:261-274`), `geometry_refused` must preserve and append to any already-committed `batch` rather than discarding ops already absorbed by `self.mirror`, and `fold.rs:141` should preserve both errors when present. Established by `host/apple/src/fold.rs:261-274` and `host/apple/src/height_drag.rs:316-317` ("Keep the pre-action frame operations: the commit mirror has already observed them, so they cannot be reconstructed from the action batch").

4. **Verification and sibling check**
Verified by reading `host/apple/src/resize.rs:99-125`, `host/apple/src/fold.rs:105-158, 244-391`, `host/apple/src/commit.rs:52-237`, `host/apple/src/paragraph.rs:493-541`, `runner/src/runner/viewport.rs:40-66, 114-152`, `runner/src/viewport.rs:599-614, 895-920`, and `host/apple/Sources/ExactKit/Session.swift:1384-1393`.
- Siblings checked: `host/apple/src/fold.rs:75-103` (`set_environment`, clean — no timer settlement before `environment_into`), `host/apple/src/fold.rs:306-332` (`set_view_at` with `size: None`, clean — skips `settle_resize_clock`), `host/apple/src/fold.rs:343-391` (`set_segments`, clean — `Runner::replace_viewport` restores segments via `Geometry::restore`), `host/apple/src/height_drag.rs:316-325` (clean — preserves `batch`), `host/painted/src/host.rs:1110-1130` and `host/painted/src/scale.rs:96-178` (clean — Linux/Windows/Android do not settle timers before `runner.set_view` and do not use a mirror/batch protocol).

5. **Suggested fix and regression test**
- In `Host::resize_at` (`resize.rs:99-103`), validate the candidate `Viewport` before calling `settle_resize_clock` (as `set_view_at` does at `fold.rs:261-274`).
- Change `Host::geometry_refused` (`fold.rs:105-121`) to accept `mut batch: Batch` and append restored style/paragraph/layout ops to it instead of allocating `Batch::new()`, passing the existing `batch` from `resize_inner` (`:140`), `set_view_at` (`:300, :320`), and `set_environment` (`:94`, or a fresh `Batch::new()` only where no timer commit preceded `environment_into`). In `fold.rs:141`, combine `error` and `format!("viewport: {e:?}")` instead of dropping `e`.
- Add unit tests in `host/apple/src/regions_tests.rs` where a timer due at `now_ms` creates and destroys views and runs a command, followed by (a) `host.resize_at(0.0, 0.0, 200.0)` and (b) `host.set_view_at(..., Some((500.0, 900.0)), ..., Some(200.0))` whose `exactViewport()` derivation divides by zero: assert the returned batch carries the `error` AND still contains every `create`/`destroy`/`children`/`command` op from the committed timer (or that `resize_at(0, 0, ...)` refuses before advancing timers).

### Siblings checked

host/apple/src/fold.rs:75-103 (set_environment, clean), host/apple/src/fold.rs:306-332 (set_view_at without size, clean), host/apple/src/fold.rs:343-391 (set_segments, clean because Runner::replace_viewport restores kernel segments via Geometry::restore), host/apple/src/height_drag.rs:316-325 (clean), host/painted/src/host.rs:1110-1130 & host/painted/src/scale.rs:96-178 (clean)
