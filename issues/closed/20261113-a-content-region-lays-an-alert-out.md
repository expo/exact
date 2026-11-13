# A content region lays an alert out without its title

**Status:** Closed
**Resolution:** A content region's trial tree gives a titled alert its title's box before its children, as rebuild does, and the title is sized through measure_reservation, which the region's measurer answers from the installed measurer as it does width samples.
**Systems:** kernel
**Severity:** P2
**Author:** Claude Opus 5.5
**Date:** 2026-11-13
**Related:** issues/closed/20261113-a-content-region-s-trial-tree.md

## What is wrong

Ordinary layout gives an alert with a title an engine leaf before its children, the title's box (`LayoutTree::new_alert_prefix`). A content region's trial tree (`Derived::build`) makes none, so inside a registered region the alert's children are laid out where the title should be.

Adding the leaf to the trial is not enough. The title is measured with `TextMeasurer::measure`, without a paragraph stamp (`layout/alert.rs`, `measure`). The region's candidate measurer answers only identified paragraphs and refuses any other measure, so a region holding a titled alert would then fail its pass with "exact-offer/source budget exhausted".

## Where

`kernel/src/region/tree.rs`, `Derived::build`. `kernel/src/layout/alert.rs`, `measure`. `kernel/src/region/state.rs`, `Candidate::measure`.

## How to reproduce

Register a content region whose content holds a `div` with an `AccessibilityLabel` and `set_alert_prefix(id, true)`, and a sized child. The region publishes the child at the alert's top; `rehydrate` and ordinary layout put it below the title.

## Direction

Give the title a measure the region's protocol can answer (an identity of its own, or the installed measurer's answer as fields have), then build the prefix in the trial as `of_subtrees` does.
