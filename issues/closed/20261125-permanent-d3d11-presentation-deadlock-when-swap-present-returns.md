# Permanent D3D11 presentation deadlock when `swap.Present` returns `DXGI_STATUS_OCCLUDED`

**Status:** Closed
**Resolution:** Consume the DXGI waitable latency grant, clear acquired, and update Pace only after accepted_present succeeds on the initial or activation-retry Present in host/ink/src/device/d3d11/composition.rs.
**Systems:** host/ink (Windows D3D11 DirectComposition presentation)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `Composition` in `host/ink/src/device/d3d11/composition.rs` configures DXGI's waitable swap chain with `SetMaximumFrameLatency(2)` (`composition.rs:227`) and tracks unspent latency grants in `held: u8` (`composition.rs:178-182`), maintaining the invariant `semaphore_count + self.held + dxgi_queued == 2`. When `self.pace.limit == 1` (the default at creation and after resize), `Composition::acquire` sets `needed = 2` (`composition.rs:334`) and loops in `wait_for_grants(needed, deadline)` (`composition.rs:303-313`) until `self.held >= 2`.

In `D3d11::present_frame` (`composition.rs:634-654`), the code unconditionally decrements `composition.held -= 1` (`L636`) and records `composition.pace.presented(Instant::now())` (`L637`) **before** calling `swap.Present(1, DXGI_PRESENT(0))` (`L638`). When `Present` returns `DXGI_STATUS_OCCLUDED`, `accepted_present` returns `Ok(false)` (`L805-819`) without incrementing submitted presents, and if `!did_present` remains false after the activation retry (`L646-650`), `L651-654` increments `refused_checks` without restoring `composition.held`. The same leak occurs if `swap.Present` returns a non-device-lost error at `L639` or if `activate()?` returns an error at `L644` before the retry at `L649` runs.

### Reachable trigger
On Windows with DirectComposition (`host/painted/src/ink.rs:471-478,920-942` and `host/painted/src/surfaces/window.rs:424-429,530-550,581-601,610-626`):
1. Initial activation or device handoff (`activating == true`, where `acquire()` skips `DXGI_PRESENT_TEST` at `composition.rs:324-326`) runs while the desktop session is occluded (lock screen, UAC secure desktop, disconnected RDP) while `IsWindowVisible && !IsIconic` remains true, so both `swap.Present(1, DXGI_PRESENT(0))` calls at `L638` and `L649` return `DXGI_STATUS_OCCLUDED`; or
2. On any active frame, the desktop becomes occluded in the window between `DXGI_PRESENT_TEST` (`L326`) and `swap.Present(1, DXGI_PRESENT(0))` (`L638`).

### Actual vs. expected
- **Actual:** An occluded `Present` does not enqueue a frame in DXGI and therefore never signals the frame-latency waitable object to return a grant (the retry at `L646-650` itself relies on an occluded `Present` not consuming a grant). After `L636` decrements `held` from `2` to `1` with no queued present, `semaphore_count == 0`, `dxgi_queued == 0`, and `held == 1`. Once the desktop is unoccluded, every subsequent `acquire()` calls `wait_for_grants(2, deadline)`, which blocks until timeout forever because `self.latency` will never signal again. `ResizeBuffers` (`L371-389`) preserves `held == 1` and resets `pace = Pace::default()` (`limit = 1`, `needed = 2`), so window presentation stays permanently deadlocked (backing off to 1 s retries) until GPU device loss or process restart. (If occlusion occurs at `pace.limit == 2`, the lost grant deadlocks permanently as soon as `Pace` returns to `limit == 1` after 30 fast frames, on resize, or on a second occluded present.)
- **Expected:** `composition.held` and `composition.pace.presented(...)` must stay in 1:1 lockstep with accepted DXGI presents per the `Composition::held` invariant (`composition.rs:178-182`) and DXGI's `GetFrameLatencyWaitableObject` contract: an occluded or failed `Present` must not consume a grant from `held`, allowing presentation to resume immediately once the desktop is visible again.

### Verification and siblings checked
Verified by reading `host/ink/src/device/d3d11/composition.rs`, `host/ink/src/device/d3d11/pace.rs`, commit `7e7dfab0b` (which introduced `held` and `SetMaximumFrameLatency(2)`), `host/painted/src/ink.rs`, and `host/painted/src/surfaces/window.rs` (running native Windows D3D11 tests requires Windows). Siblings checked:
- `host/ink/src/device/metal/present.rs:321-363`: safe (`ring.retire(command)` at `L362` runs only after `nextDrawable()` and encoding succeed; missing drawable returns early at `L330-336`).
- `host/ink/src/device/wgpu/present.rs:236-249,323-324`: safe (`Timeout`/`Occluded`/`Outdated`/`Lost` return early before updating `self.ring` or `self.next`).
- No other waitable-swapchain or `held`-grant presentation path exists in `host/ink/`.

### Suggested fix and regression test
Do not decrement `composition.held` or call `composition.pace.presented(...)` before `swap.Present`. Instead, clear `composition.acquired = false` only after an accepted present (or restore it / leave `held` unchanged on `!did_present`), and decrement `composition.held -= 1` and call `composition.pace.presented(Instant::now())` immediately when `accepted_present(...)` returns `Ok(true)` (on either `L639` or the activation retry at `L649`, before fallible bookkeeping like `submit()`/`activate()`). Add a unit test in `host/ink/src/device/d3d11/composition.rs` extracting the grant/pace accounting helper and asserting that `DXGI_STATUS_OCCLUDED` (and error results) leave `held` unchanged while `S_OK` decrements `held` by 1 and updates `pace`, plus verify a follow-up `wait_for_grants(2, ...)` succeeds without waiting on the handle when `held` remains `2`.

### Siblings checked

Checked host/ink/src/device/metal/present.rs:321-363 (safe: retires ring slot only after drawable acquire and encode succeed) and host/ink/src/device/wgpu/present.rs:236-249,323-324 (safe: early return on Timeout/Occluded/Outdated/Lost before updating ring or index). No other waitable-swapchain path exists in host/ink/.
