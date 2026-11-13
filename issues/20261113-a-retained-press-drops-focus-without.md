# A retained press drops focus without blur or change

**Status:** Open
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A successful press on a retained row takes `focus` and clears `focus_out`. It does not call `set_focus`. That call is what commits the field, dispatches `blur` and `focusout`, and clears `edited`.

The field therefore stays edited with no `change` and no `blur`, while focus is already gone. A refused retained press leaves focus. An accepted one drops it without those events.

## Where

`host/linux/src/presenter/retained_action.rs`, `retained_press_target`. `host/linux/src/presenter/events.rs`, `set_focus`.

## How to reproduce

Focus a text field and type. Press a retained row that accepts the press. The field is no longer focused. Its `change` and `blur` handlers do not run, and the typed value is still marked edited.

## Direction

Clear focus through `set_focus(None)`, so the field commits and blurs before the press's commit. Do not take `focus` aside from that path.
