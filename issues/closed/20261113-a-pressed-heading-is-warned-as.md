# A pressed heading is warned as no iOS bar

**Status:** Closed
**Resolution:** The header-bar walk classifies text role=heading as the heading even when it presses, as the kernel's bar does, so one pressed heading is the title and two are warned as two headings.
**Systems:** contract
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A header child with `press` is classified as pressable before it can be a heading. `text role="heading"` that also presses is therefore not counted as a heading. With that count at zero, and nothing else in doubt, the lowerer warns `lower-header-no-bar`: the header is drawn in the page and the bar has no title.

The kernel sets the heading fact and the press fact on the same text node, and the bar walk takes the heading fact first. iOS asks the kernel for that shape. One pressed heading is still the navigation title. The warning tells the author to add a heading the screen already has.

Two pressed headings are also counted as zero, so the warning that names two headings never fires. The kernel then projects no bar.

## Where

`contract/lower/src/warnings/header_bar.rs`, `part` and `route_header`. `kernel/src/bars.rs`, `fact::of` and `Walk::walk`.

## How to reproduce

Give a route one `text role="heading"` with `press`. The build warns that the header holds no heading. On iOS the navigation bar still uses that text as its title.

## Direction

Classify `text role="heading"` as a heading even when it presses. Keep the pressable path for an ancestor of the heading, which the kernel already walks as the title's tap.
