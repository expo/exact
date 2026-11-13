# The Windows edit menu enables Paste from the system clipboard

**Status:** Open
**Systems:** host/windows
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

The edit menu enables Paste only when the system clipboard has `CF_UNICODETEXT`. Under the default agent the clipboard adapter does not use that board. `write` and `read_text` keep text on the thread-local memory clipboard, and Ctrl+V reads it.

After an in-app copy, Paste is gray while Ctrl+V would insert the memory text. If the system clipboard has text and the memory board does not, Paste is enabled and the action inserts nothing.

## Where

`host/windows/src/edit_menu.rs`, the `IsClipboardFormatAvailable(13)` check. `host/windows/src/clipboard.rs`, `write` and `substituting`. `host/windows/src/clipboard/system.rs`, `read_text`. `host/linux/src/presenter/edit_menu.rs`, `edit_menu_action`, which pastes through that adapter.

## How to reproduce

Drive a Windows session with the default agent clipboard. Copy text inside a field. Open the edit menu: Paste is gray, and Ctrl+V inserts the copy. Put text on the system clipboard while the memory board is empty. Paste is enabled and inserts nothing.

## Direction

When substituting, enable Paste from the memory board's text, the same `read_text` the menu action uses. Keep `IsClipboardFormatAvailable` for the system clipboard.
