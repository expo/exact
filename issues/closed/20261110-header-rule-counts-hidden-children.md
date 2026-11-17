# The header rule counts a header's `display: none` children as bar items and headings

**Status:** Closed
**Resolution:** The header rule leaves out a `display: none` child with all it holds, as it leaves out a popover or dialog, so it is no bar item and no heading.
**Systems:** kernel/src/bars.rs, host/apple/Sources/ExactKit/IOS/HeaderShapeIOS.swift, host/linux/src/presenter/bars.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`bars::header_shape` is the one rule for what a route's `header` gives the platform's
bar (LLP 1115.001 D11). Its walk (`kernel/src/bars.rs:251-288`) skips a top-layer child
(`fact::LAYER`) and nothing else. `fact::HIDDEN` is read only on the header itself
(`bars.rs:351`). So a button, a search field, a tablist or a heading in the header that
is `display: none`, or that sits in a box that is, is placed as if it showed. The web
shows none of them.

Two results follow.

- A hidden button is in `leading` or `trailing`. Android's top app bar makes a button of
  every id in the lists (`host/linux/src/presenter/bars.rs:234-277`), and iOS makes an
  `Item` of every one that is not a readout
  (`host/apple/Sources/ExactKit/IOS/HeaderShapeIOS.swift:234`). Neither reads `display`.
  The Mac filters the lists after the rule
  (`host/apple/Sources/ExactKit/Mac/HeaderMac.swift:139`).
- A header with two headings, one of them hidden, is not header-shaped (`bars.rs:315`,
  exactly one heading), so the route gets no bar.

## The failure

```
header display="flex"
  button id="back" press=back
    image "symbol:back"
  text "Detail" role="heading"
  button press=edit display="none"
    text "Edit"
```

Android's top app bar for this route has the trailing item "Edit". Expected: no trailing
item, as with the button removed.

```
header
  text "Other" role="heading" aria-level=1
  text "Draft" role="heading" display="none"
```

Android shows no top app bar. The Mac leaves the header painted in the page and takes no
window title from it. Expected: the bar titled "Other", as without the second heading.

## Verification

- A kernel test: `Kernel::header_shape` on a header holding a heading (12) and a
  `display: none` button (13) answered heading `12`, trailing `[13]`; with a second,
  hidden heading it answered `None`.
- Android: a temporary test in `host/linux/src/presenter/bars_tests.rs` over that file's
  Contract fixture with the two edits above (`cargo test -p exact-linux --lib`) printed
  `trailing [("Edit", None)]` with the button hidden, and `top bar None` with the hidden
  heading where the fixture as written gives `Some("Other")`.
- The Mac: the same two cases in a temporary routed `apps/hello`, driven with
  `bun scripts/agent.mjs macos --app hello --chrome platform layout "tap tab-other" layout "logs --all"`.
  The hidden button is no toolbar item. The header with the hidden heading is laid out
  in the page (376×26) with no `window header` line in the log; with the heading removed
  the log says "route #19's header #20 is the window's title and toolbar".
- iOS was not run (no simulator on this machine). Its path was read: nothing between the
  rule's lists and `UIBarButtonItem` tests `display` for a button.

## Direction

Skip a `fact::HIDDEN` child in `Walk::walk` and in `holds_heading`, as a `LAYER` child is
skipped. Both trees already carry the fact (`fact::of`, `HeaderRule.facts`).

## Related

`kernel/tests/it/bars.rs:651-656` pins the hidden header, and
`host/apple/Tests/ExactKitTests/HeaderMacTests.swift:232` the Mac's filter. Nothing pins
a hidden child in the rule. No issue or QUEUE entry names this.
