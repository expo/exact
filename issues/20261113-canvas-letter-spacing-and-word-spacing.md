# Canvas letter spacing and word spacing stay at the font size from when they were set

**Status:** Open
**Systems:** canvas recorder
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13
**Related:** issues/closed/20261110-canvas-word-spacing-lost-with-letter-spacing.md

## What is wrong

`letterSpacing` and `wordSpacing` store a pixel length computed when the attribute is set. `em` is multiplied by the font size at that moment. `ex` and `ch` go through `length_px`, which resolves them against `BASE_SIZE` (10px), so they ignore the current font entirely. `set_font` replaces the font and does not recompute either spacing. `measure_raw` and `send_font` then record the stored pixels. The getter still returns the specified string, so the attribute reads `1em` while drawing uses the old gap.

The canvas text tests require the used value to follow a later font. `2d.text.drawing.style.letterSpacing.change.font` sets `letterSpacing` to `1em` at the default 10px font, then sets `font` to `20px serif` without touching the spacing, and expects the extra width to double. Both recorders (Rust and `canvas/recorder.js`) disagree with that, and they agree with each other, so every host paints the frozen gap.

## Where

- `canvas/src/context/text.rs` `spacing`, `set_letter_spacing`, `set_word_spacing`, `set_font`
- `canvas/src/font.rs` `length_px` (`ex` and `ch` against `BASE_SIZE`)
- `canvas/recorder.js`, the same pair of setters

## How to reproduce

On a 2D context: `letterSpacing = "1em"`, measure `"Hello World"`, then `font = "20px serif"` and measure again. The second measure should add twice the extra width. It adds the same width as the first.

## Direction

Keep the specified string and resolve `em`, `ex`, and `ch` from the font in force inside `measure_raw` and `send_font`, and in the JS recorder's twins.
