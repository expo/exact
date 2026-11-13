# An open menu keeps a focus index after its rows change

**Status:** Open
**Systems:** host/linux
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

An open menu keeps its focus as an index into the current rows. When the surface is measured again, that index is kept if it still fits the new list. Enter and Space choose that index.

Inserting or removing a choosable row above the focus, while the menu stays open, points the index at a different target. The arrows last selected the old row.

## Where

`host/linux/src/presenter/top.rs`, `sync_surface`, where `focused` is filtered by the new target count.

## How to reproduce

Open a menu, move to a row with the arrows, and insert a row above it before the menu closes. Press Enter. The row that activates is the one now at the old index.

## Direction

Remember the focused target's node before measuring, and select that node again. If it is gone, clear the focus.
