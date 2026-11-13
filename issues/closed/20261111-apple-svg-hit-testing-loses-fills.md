# Apple SVG hit testing loses fills from empty gradients

**Status:** Closed
**Resolution:** The Apple scene sends each shape's hit paints as the kernel decides them by `pointer-events` (`hp`), and the presenter hits by those rather than by what the shape draws.
**Systems:** host/apple/src/svg.rs, host/apple/Sources/ExactKit/SvgScene.swift
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

An empty gradient resolves to no drawable paint (`kernel/src/svg/scene.rs:793`), but the scene separately retains that the declared fill is not `none` (`:859–862`). Apple serialization writes the absent paint as `null` (`host/apple/src/svg.rs:1206–1208`) without retaining that hit-testing fact. `host/apple/Sources/ExactKit/SvgScene.swift:569` reconstructs paintedness from the drawable fill, so `visiblePainted` misses the shape.

## The failure

An SVG contains `<linearGradient id="empty"/>` and a 100×100 rectangle with `fill="url(#empty)"` and a press handler. A press at `(50,50)` returns no target on Apple. It should hit the rectangle: its declared fill is non-none even though the gradient draws no pixels.

## Verification

Ran `bun host/apple/build.mjs --test --only Fix08MacTests,Fix08AlertMacTests,Fix08HeaderMacTests`. The temporary XCTest compiled the equivalent Contract, booted an Exact session and queried the presenter's SVG target at `(50,50)`. It printed `FIX08 empty-gradient target at (50,50): nil`; the non-nil assertion failed.

A temporary Bun script beside the web tests launched headless Chrome through `scripts/agent-launch.mjs` and queried `document.elementFromPoint(50,50)` on the SVG above. It printed `{"target":"target","pointerEvents":"auto"}`. Both temporary probes were removed.

## Direction

Serialize the scene's declared fill/stroke paintedness separately from its drawable paint and use it for SVG hit testing.

## Related

Reported in the p2-svg-press-apple review. No existing issue or QUEUE entry records the loss of declared paint for an empty gradient.
