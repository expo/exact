# Canvas2D shadows can request gigabytes and trillions of shader iterations

**Status:** Open
**Systems:** canvas/vello
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, the Vello Canvas2D backend expands each shadow's offscreen
rectangle by `ceil(3 * sigma) + 1` on every side, where
`sigma = shadowBlur * scale / 2`. Its only upper bound is 16,384 pixels per
dimension (`canvas/vello/src/paint.rs:332-353`). A large finite `shadowBlur`
passes both recorders and this guard even when the canvas is tiny.

Each accepted job requests three full-sized textures, with four bytes per
pixel each (`canvas/vello/src/gpu.rs:421-438`). The two blur shaders then
iterate from `-ceil(3 * sigma)` through `ceil(3 * sigma)` for every output
pixel, including an exponential in every iteration
(`canvas/vello/shaders/shadow_blur_x.wgsl:31` and
`canvas/vello/shaders/shadow_blur_y.wgsl:31`). There is no downsampling or
combined memory/work budget. Multiple draws also retain each finished
shadow texture until the main scene is rendered (`gpu.rs:305-343`).

A 10 by 10 rectangle at `(10, 10)` on a 100 by 100 canvas at scale 1 is
enough to produce these requests:

| shadowBlur | Offscreen dimensions | Three texture payloads | Blur loop iterations |
| --- | --- | --- | --- |
| 1,000 | 3,012 by 3,012 | 108,865,728 bytes | 54,451,008,288 |
| 5,000 | 15,012 by 15,012 | 2,704,321,728 bytes | 6,761,255,040,288 |

These are accepted rendering jobs, not dimension-guard refusals. A valid
canvas draw can therefore exhaust GPU memory or keep the shared renderer
busy for excessive time. A device with a smaller texture limit can reject
the draw instead. The work is disproportionate to the 40,000-byte canvas.

A local Rust probe used the actual `exact_canvas::Context2d` to confirm
the setter accepts both values and emits a draw list. It executed the
unchanged geometry portion of `Replayer::shadow` with inert rectangle
operations and calculated the texture payload and shader iteration count
from the implementation. Its output is the table above. The probe is
`/tmp/exact-review-20261112/canvas-shadow-budget.rs`. It did not allocate
textures, run GPU work, or measure a device failure; these figures are
requested payloads and loop counts, excluding renderer overhead.

Downsample large shadows before convolution, then composite at the correct
size. Bound the total temporary texture bytes and shader work across a
frame. Test large radii on tiny canvases, high display scales, and many
shadowed draws, while preserving the ordinary-radius pixel checks.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies. The closed
`20261102-backdrop-blur-allocates-the-whole-pad.md` fixed SVG/Linux CPU blur
and text-shadow paths; this Canvas2D Vello path still performs the unbounded
full-resolution convolution. GitHub #145 concerns Linux Ink scene and
presentation performance, not this Canvas2D shadow implementation.
