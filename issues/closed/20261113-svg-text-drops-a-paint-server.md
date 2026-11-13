# SVG text drops a paint server and its fallback colour

**Status:** Closed
**Resolution:** A text run resolves url() as a shape does: a missing server paints its fallback colour, a one-stop gradient its colour, and a gradient is attached for the host (painting gradients on text is still owed on the painted and Apple hosts, which draw no fill for one).
**Systems:** kernel
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A text run's paint returns nothing for every `Paint::Url`. A `text` or `tspan` fill or stroke of `url(#id)`, or `url(#id)` with a colour fallback, never becomes a `ShapePaint`. The same paint on a shape resolves the server, or paints the fallback colour.

Text whose paint is a server is invisible on every host, including when the author gave a colour the text painter already draws.

## Where

`kernel/src/svg/scene/text.rs`, the `paint` closure in `runs`. `kernel/src/svg/scene.rs`, the shape paint that resolves `Paint::Url`.

## How to reproduce

Load an SVG whose text is `fill="url(#g) red"` and whose `#g` is missing or is a gradient. The text is not drawn. A rect with the same fill is red, or shows the gradient.

## Direction

Resolve `url()` the way shape paint does. Attach the server when it resolves, otherwise paint the fallback colour.
