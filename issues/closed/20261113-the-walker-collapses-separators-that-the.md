# The walker collapses separators that the shape path keeps

**Status:** Closed
**Resolution:** The walker collapses only U+0020, U+0009, U+000A and U+000D, the set collapse removes, so a form feed, NEL, U+2028 or U+2029 is text on both paths.
**Systems:** textflow
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

Under `normal`, `nowrap`, and `pre-line`, `Prepared` treats U+000C, U+0085, U+2028, and U+2029 as collapsible whitespace, because `hard_break` matches them and those modes do not force the break. A run of them becomes one hanging space. `collapse`, the path native hosts shape before painting, removes only U+0020, U+0009, U+000A, and U+000D. The same source keeps a form feed when it is shaped, and merges it into a space when it flows around a shape.

## Where

`textflow/src/walker.rs`, `whitespace` and `hard_break`. `textflow/src/collapse.rs`, the set of characters that collapse. `textflow/tests/it/walker.rs`, `css_normal_transforms_segment_breaks_and_collapses_runs`.

## How to reproduce

Shape `alpha` plus a form feed plus `beta` with `white-space: normal`, then flow the same string around a float. The shaped line keeps the form feed. The flowed line merges it into a space.

## Direction

Collapse only U+0020, U+0009, U+000A, and U+000D in the walker as well. Do not classify form feed, NEL, line separator, or paragraph separator as removable spaces under `normal`, `nowrap`, or `pre-line`.
