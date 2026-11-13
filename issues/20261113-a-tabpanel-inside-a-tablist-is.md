# A tabpanel inside a tablist is never deferred

**Status:** Open
**Systems:** semantics, runner
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`panels` does not walk into a node whose role is `tablist`, so a tabpanel there is not a candidate and no tab's `aria-controls` can name it. `toOpen` then opens it immediately. `tab_panels` walks through a tablist and stops only at `navigationKey`, so the same panel stays in the named set and the runner opens at most one waiting panel until a route holds the key.

## Where

`semantics/Contract/Tabs.lean`, `panels` and `toOpen`. `runner/src/head.rs`, `tab_panels`.

## How to reproduce

Put a named `tabpanel` inside its `tablist`, under a navigation root, and verify. The semantics builds that panel at once. The runner keeps it waiting with the other named panels.

## Direction

In `panels`, descend into a tablist the way `tab_panels` does. Stop the walk only when `navigationKey` is set. Keep using the tablist's direct children as its tabs.
