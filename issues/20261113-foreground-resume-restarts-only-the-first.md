# Foreground resume restarts only the first arrival extension

**Status:** Open
**Systems:** host/web-js, runner
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

Leaving the foreground suspends each arrival extension's scans into one shared set. Coming back should resume each extension and re-read a tag still in its field. The web substitute clears the whole set on the first extension that sees it, then arrives only for that extension. Every later extension finds the set empty and skips resume.

The runner does the same: `mem::take` of the shared set sits inside the per-extension loop, so the first extension takes every token.

## Where

`host/web-js/arrival.js`, `visibility`. `runner/src/runner/modules/arrival.rs`, the foreground arm of the visibility walk.

## How to reproduce

Run two arrival extensions. Background the page so both scans suspend, then foreground it. The journal says "scans resumed" for the first extension only. The second stays held and does not read its tag until the field changes again.

## Direction

On resume, take only this extension's scan claims from the suspended set, and re-arrive only when that subset was non-empty.
