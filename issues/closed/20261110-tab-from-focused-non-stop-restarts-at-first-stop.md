# Tab from a focused node that is no Tab stop goes to the page's first stop, not the next one after it

**Status:** Closed
**Resolution:** Already fixed by 5f5b3064, which made a focused node that is no Tab stop the search's starting point outside a popover too; a test now presses the issue's clicked `tabindex=-1` box and gets `b` for Tab and `a` for Shift+Tab, as Chrome does.
**Systems:** host/linux/src/presenter/typing.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`move_focus_in` (`host/linux/src/presenter/typing.rs:523`) finds the next Tab stop from
the focus's position in the Tab order. When the focused node is not in that order (a
`tabindex=-1` node that a click or `focus()` focused), it falls to lines 576 and 577,
`start.and_then(after).unwrap_or(0)`, where `start` is set only for `focus_start`, the
point a fragment navigation left. So Tab goes to the first stop of the page and Shift+Tab
to the last.

HTML's sequential focus navigation starts from the focused area. When that is not in the
sequential focus navigation order, the selection mechanism is "DOM", and the candidate is
the suitable focusable area "that appears nearest after starting point" (before it, for
Shift+Tab). The function does this in two cases already: for `focus_start` (lines 559 to
571, whose comment names that algorithm) and for a focused non-stop inside an open popover
(line 538). The general case is left out, and the function's own comment (lines 498 to
504) describes it as going "to the first". LLP 1088 D7.3 asks only for "HTML's order". The
web host is the browser, so the hosts split on this input: a page that focuses a heading
after a navigation, or a `tabindex=-1` region a person clicked, sends the next Tab back to
the top of the page on the painted hosts.

## The failure

```
component App
  state n = 0
  action bump
    n = n + 1
  view
    column width=400 height=300
      button press=bump testId="a" height=30
        text "A"
      box tabindex=-1 testId="mid" width=100 height=30
        text "Heading"
      button press=bump testId="b" height=30
        text "B"
      button press=bump testId="c" height=30
        text "C"
```

Tap `mid`, press Tab. Expected: `b`. Observed: `a`. Tap `mid`, press Shift+Tab. Expected:
`a`. Observed: `c`.

## Verification

A temporary test in `host/linux/src/presenter/fragment_tests.rs` booted the plan, tapped
`mid` (and, in a second plan, focused it with the app's `focus("heading")`), pressed Tab
and Shift+Tab, and printed `a` for Tab and `c` for Shift+Tab each time. The same four
elements as bare HTML in Chrome 156 gave `b` and `a`.

## Direction

When the focus is live and not in the order, use its preorder position as `start`, as the
`focus_start` branch does, and correct the comment at lines 498 to 504.

## Related

The closed issue `20261010-macos-tab-from-a-focused-box` fixed the same rule on macOS,
where Tab from such a box went nowhere. `QUEUE.md`, "The painted presenter's focus ring
keeps the last input's modality per node", is about the ring.
