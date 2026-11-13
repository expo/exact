# Kernel-free symbol color misses plain dialog and raised popover starts

**Status:** Open
**Systems:** runner, kernel
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

`DocTree::symbol_color` stops the walk for a neutral route header, an `alertdialog`, and a non-button child of an `alertdialog`. Kernel `says` also stops where a style start sets `color`. A plain `dialog` (alert context 13) uses `DIALOG`, and a default-surface popover (contexts 15 and 17) uses `RAISED`. Both start from `SURFACE`, which sets `TextColor`. The web document writes `data-symbol-color` from either tree.

A symbol inside a `dialog` whose role is not `alertdialog`, or inside a popover the compiler marks `defaultSurface`, under an ancestor that wrote `color`, is marked in the kernel-free document and left on the sheet color in the kernel document.

## Where

`runner/src/instance/document.rs`, `symbol_color`. `kernel/src/arena/alert.rs`, `SURFACE`, `DIALOG`, `RAISED`, and `alert_start`. `kernel/src/arena/text_style.rs`, `says`.

## How to reproduce

On the neutral column, put a `symbol:` image inside a `dialog` that is not an `alertdialog`, or inside a `defaultSurface` popover, under an ancestor with an authored `color`. Compare `data-symbol-color` on the kernel-free document with the color the kernel document paints.

## Direction

Stop the document walk on the same neutral starts that set `TextColor`, including plain dialog and raised popover. Keep the field exception: a field holds no image.
