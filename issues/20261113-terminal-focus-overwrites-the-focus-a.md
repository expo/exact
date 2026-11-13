# Terminal focus overwrites the focus a blur handler set

**Status:** Open
**Systems:** host/terminal
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`focus` clears the slot, dispatches `blur`, runs that commit, and then writes the caller's target into the slot. A blur action's `focus()` or `showModal` runs inside that commit and sets the slot. The assignment after the dispatch replaces it, and it does not send `blur` for the focus the commit chose.

Tab off a field whose blur opens a dialog therefore drops the dialog's autofocus. Later keys edit the covered control. Escape still dismisses the layer.

## Where

`host/terminal/src/host.rs`, `focus`, `autofocus`, `open_layer`, and `focus_open_layer`. `host/terminal/src/keys.rs`, the field path after a key's default, which does not ask `input_eligible` again.

## How to reproduce

In the terminal host, focus a field whose `blur` calls `showModal` on a dialog with `autofocus`. Tab away. The dialog opens. The caret stays on the field under it, and the next character edits that field.

## Direction

After the blur dispatch, keep a focus the commit already set. Assign the caller's target only when the slot is still empty.
