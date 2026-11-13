# A portal file picker answer survives the view it was opened for

**Status:** Open
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A portal file picker answers by the view id it was asked for. That answer sits in the portal inbox until the next pump. A renewed view keeps the same id and is a different element. `picks_renewed` drops a shell picker waiting on that id. It does not drop a portal outcome already queued.

The chosen files are then delivered to whatever element now has that id. A list row given another item while the portal is open receives the previous item's files.

## Where

`host/linux/src/presenter/picker.rs`, `ask_portal` and `finish_portal_picks`. `host/linux/src/presenter/requests.rs`, `picks_renewed`. `host/linux/src/presenter.rs`, `renew`.

## How to reproduce

Open a portal file picker from a list row. Renew that row (the list reuses the view for another item) before the portal answers. Choose a file. The new item receives it.

## Direction

When a view is renewed, drop portal outcomes addressed to it, the same way `picks_renewed` drops a shell picker's answer.
