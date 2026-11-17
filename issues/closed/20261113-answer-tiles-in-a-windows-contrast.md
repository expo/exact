# Answer tiles in a Windows contrast theme paint the label in the same colour as the fill

**Status:** Closed
**Resolution:** In a contrast theme (prefersContrast custom) a correct or wrong AnswerTile is the theme's Highlight fill under HighlightText, its mark included, as Badge's role tints are.
**Systems:** contract content library, Windows contrast palette
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A correct or wrong `AnswerTile` sets `background-color` to `-exact-system-green` or `-exact-system-red` and `color` to `-exact-label`, including the check or cross. The Windows contrast palette maps `-exact-label` and every `-exact-system-*` hue to `COLOR_WINDOWTEXT`, in both schemes. The words, the mark, and the fill are one colour.

`Badge` in the same file switches a role fill to `Highlight` and the label to `HighlightText` when `prefersContrast` is `custom`. `AnswerTile` never reads that fact. `appearance="none"` keeps the authored fill, so this is the standalone tile, not a grouped row whose look is dropped. The accessible name still says "correct" or "wrong". The tile itself does not.

## Where

- `contract/lib/content.contract` `AnswerTile` (the correct/wrong button) and `Badge` (the `prefersContrast == "custom"` branch)
- `host/linux/src/presenter/contrast.rs` `ContrastPalette::report`, the `CanvasText` line that lists `-exact-label` and the system hues together

## How to reproduce

On Windows with a high-contrast theme, show an `AnswerTile` whose `status` is `correct`. The label and the checkmark are the same colour as the tile.

## Direction

When `prefersContrast` is `custom`, paint correct and wrong tiles with `background-color="Highlight"` and `color="HighlightText"`, as `Badge` does, and keep the check or cross.
