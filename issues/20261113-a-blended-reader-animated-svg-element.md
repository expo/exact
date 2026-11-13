# A blended reader-animated SVG element never closes its layer

**Status:** Open
**Systems:** host/linux
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A reader-animated SVG element opens a layer, then a blended item returns before that layer is closed. Opacity that is zero closes the layer. The blend path draws an island, or skips it, and returns. `layer_close` runs only on the path below that return.

CPU `layer_begin` declines the layer, so the CPU painter is unaffected. GPU, Ink, and the Android canvas accept it. The open capture still holds everything drawn before the element. A later row end pops that capture and panics (`row balanced`), or later frames keep nesting captures. A fading or moving SVG shape with a blend mode blanks or crashes the frame for the whole animation.

## Where

`host/linux/src/paint/svg.rs`, `svg_item`, the `item.blend != 0` return. `host/linux/src/paint/layer.rs`, `svg_layer` and `layer_close`. `host/linux/src/gpu/rows.rs`, `keep_row`.

## How to reproduce

Animate an SVG shape's opacity or transform, and set `mix-blend-mode` to multiply or screen. Paint it on the GPU, Ink, or the Android canvas. The frame blanks, or the process panics with `row balanced` when the row is kept.

## Direction

Call `layer_close` on every blend return, including when the island is absent, before compositing the island into the parent.
