# An Apple window changes other sessions' canvas resolution

**Status:** Open
**Systems:** host/apple/src/canvas2d.rs, host/apple/src/abi.rs, host/apple/Sources/ExactKit/Mac/ExactViewMac.swift, host/apple/Sources/ExactSurfaces/Canvas2DImage.swift
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, the Apple host stores Canvas 2D's display scale in one
process-wide atomic (`canvas2d.rs:9-25`). Every session's canvas turn reads
that value (`:46-54`, `:85-89`). `Bridge::canvas_display` changes the global
value and immediately updates only its own host (`abi.rs:1253-1261`).

Two sessions can have different scales: they may occupy different displays,
or have different page zoom on the same display. The Mac derives scale from
the window's backing scale and the session's viewport magnification
(`SessionControlTextMac.swift:20-28`), and publishes it only when that
view's scale changes (`ExactViewMac.swift:268-275`). After B publishes its
scale, A's next canvas turn adopts B's value despite no change to A.

This unnecessarily replaces A's bitmap generation and changes its pixel
dimensions. A lower scale produces a lower-resolution canvas; a higher
scale allocates and draws more pixels. Repeated scale changes also reset
incremental drawing state. The replay's mismatch correction does not make
this safe: it reports a given actual scale only once per `Canvas2DHost`
(`Canvas2DImage.swift:215-219`). Another session can overwrite the global
again after that correction.

A local probe linked the existing Apple host artifacts, booted two
`Bridge` instances with the same 100 × 50 CSS-pixel canvas, and called
`canvas_draw` on A after booting B. The emitted operations were:

```text
A boot:      scale=2, width=200, height=100, generation=0
B boot:      scale=1, width=100, height=50,  generation=0
A next draw: scale=1, width=100, height=50,  generation=1, fresh=true
```

A's display was never changed. The probe bound A's role scope before its
draw, as the ABI registry does. This verifies the Rust host transition and
output; it does not claim a Swift presentation or pixel test. Probe source,
plan, compiler arguments and output are in
`/tmp/exact-review-20261112/canvas-session-scale.{rs,contract,plan,out}` and
`canvas-session-scale-rustc.json` in that directory.

Keep the display scale on the session or host that owns the canvas. Device
physical memory can remain shared. Add a two-session regression with
different scales, alternating draws and a scale change to each session;
only the session whose scale changed should replace its canvas generation.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies for shared canvas scale, display
scale and multiple sessions. Existing device-pixel-ratio and widget layout
reports do not cover this process-wide canvas state.
