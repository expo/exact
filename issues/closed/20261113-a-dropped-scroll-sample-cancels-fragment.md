# A dropped scroll sample cancels fragment navigation

**Status:** Closed
**Resolution:** A reader scroll sample counts as the person's scroll only once it is accepted and moves the scroller, so a dropped sample no longer cancels a fragment scroll (the presenter is now host/painted).
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`reader_scrolled` increments `person_scrolls` before it decides the sample is dropped. A scroller the reader was never told of, or a revision that no longer matches, returns without moving the offset. The counter has already changed.

`scrollToFragment` and the image-load hold treat any change of that counter as the person having scrolled. A fling sample that arrives after the app moved the scroller, defined to be dropped, cancels the fragment. The route stays where it is, and the journal says the person scrolled first.

## Where

`host/linux/src/presenter/scrollers.rs`, `reader_scrolled`. `host/linux/src/presenter/reveal.rs`, `scroll_to_fragment`.

## How to reproduce

Navigate to a fragment. Before `scrollToFragment` runs, deliver a reader scroll sample whose revision is stale. The offset does not change. The fragment scroll does not run.

## Direction

Increment `person_scrolls` only after the revision check accepts a sample whose offset actually changes.
