# An SVG stylesheet drops rules after a nested at-rule

**Status:** Open
**Systems:** host/svg-file
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`rules` ends a block at the first `}`. `@media`, `@supports`, and `@keyframes` contain inner braces. The at-rule is dropped, and the leftover `}` is glued onto the next selector. `Selector::parse` refuses that text, so every later rule is discarded. `@font-face` and `@import`, which have no inner `}`, leave later rules intact. Rules before the at-rule still apply.

An SVG that writes a class rule after `@keyframes` or `@media` therefore never gets those fills. The picture disagrees with a browser image of the same file. The lowerer records a warning.

## Where

`host/svg-file/src/css.rs`, `rules`. `host/svg-file/src/lower.rs`, the call that applies the sheet.

## How to reproduce

Load an SVG whose `style` element is `@keyframes spin { from { opacity: 1 } to { opacity: 0 } } .cls { fill: #c00 }` and a shape with `class="cls"`. The fill stays the geometry's own. The same file in a browser is red.

## Direction

Skip an at-rule by brace depth and resume after its matching `}`.
