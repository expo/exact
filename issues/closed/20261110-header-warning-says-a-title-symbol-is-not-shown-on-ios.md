# `lower-header-unplaced` says a symbol image beside a route header's heading is not shown on iOS; the iOS presenter draws it in the title

**Status:** Closed
**Resolution:** `lower-header-unplaced` counts a symbol image in the heading's own flex row as placed, by the rule the iOS title applies (`HeaderTitle`: the row's other shown children are bar items or unfilled symbol images), and its sentence says the title takes them.
**Systems:** contract/lower/src/warnings.rs, host/apple/Sources/ExactKit/IOS/NavigationTitleIOS.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at `fe4632ed`. The compiler warns about each node in a route's first `header` that the iOS navigation bar does not take (`contract/lower/src/warnings.rs:447-451`). Its walk of the header (`warnings.rs:458-515`) has no case for an image: one in the heading's row falls to `Part::Other` and is reported as "this `image`".

The iOS presenter does take one. When the heading's parent is a flex row whose other shown children are passive symbol images, `NavigationTitleIOS.swift:71-87` collects each as a `TitleGlyph`, and the title is drawn with them before or after its text (`:343`).

## The failure

```
header display="flex" align-items="center" padding="8px 16px"
  row align-items="center" gap=6
    image "symbol:add"
    text "Detail" role="heading" aria-level=2
```

Observed, from `contract build`:

```
warning [lower-header-unplaced] this `image` in this route's `header` is not shown on iOS: the navigation bar there takes one heading as its title, the first text after it as the subtitle, one `input type="search"`, one `role="tablist"`, the buttons that press or are disabled and the readouts (`role="status"`), and hides the rest of the header …
```

Expected: no warning for an image the bar draws, and a sentence whose list of what the bar takes includes it.

## Verification

Compiled `docs/patterns/list-detail/app.contract` with its detail header changed to the form above, at `c047f991`: the warning shown, at the image's line. The presenter's side was read, not run: this machine cannot run the iOS simulator.

## Direction

In the walk, count a symbol image in the heading's row as placed, by the rule the presenter applies (passive, an image, a symbol), and name it in the warning's sentence.

## Related

`issues/20261110-navigation-title-symbol-loses-colour.md` is about how the presenter draws such a symbol.
