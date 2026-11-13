# A leftover preventDefault cancels the next input event

**Status:** Closed
**Resolution:** Each dispatch site now counts and takes only the preventDefault commands its own handlers queued (prevented_since), so one an earlier event left, such as a pointerup's, no longer cancels the next key, wheel, press, beforeinput, paste or edit menu (the presenter is now host/painted).
**Systems:** host/linux, host/windows, host/android
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`preventDefault` is a command the event that queued it is supposed to see. `run_commands` drops it, and it runs once at the start of a turn. Several input paths instead delete every queued `preventDefault` and treat any deletion as this event's own cancel.

`pointer_lifted_by` counts only the commands this `pointerup` added, and leaves them queued. The primary `pointer_lifted` throws that count away. A back or forward button uses the count and still leaves the command. The next `key`, wheel, `pointerdown`, `beforeinput`, paste, or edit menu in the same drain deletes it and skips its own default.

Linux reads every pending evdev event after one `run_commands`. Android drains every queued touch and scroll the same way. Windows runs `run_commands` on redraw, and a right-button release opens the edit menu in that same mouse-up. A `pointerup` that prevents the default, the way a back or forward button is kept from stepping history, therefore also drops the next character or Tab, skips a wheel's scroll, blocks focus on the next press, or suppresses the edit menu.

## Where

`host/linux/src/presenter/pointer.rs`, `pointer_lifted` and `pointer_lifted_by` (the count) and `wheel_event` (deletes every `preventDefault`). `host/linux/src/presenter/events.rs`, `key_event`. `host/linux/src/presenter/contact.rs`, the press path that sets `focus_blocked`. `host/linux/src/presenter/field.rs`, `before_input`. `host/linux/src/presenter/clipboard.rs`. `host/linux/src/presenter/edit_menu.rs`. `host/linux/src/presenter/commands.rs`, the `preventDefault` arm of `run_queued_commands`. `host/linux/src/display.rs` and `host/linux/src/android.rs`, one command drain then the whole input batch. `host/windows/src/window.rs`, right-button release then `edit_menu_at`.

## How to reproduce

On Linux, give a box a `pointerup` that calls `preventDefault`, and a button whose `key` handler types a character. Click the box with the mouse's back button, which stays on this page, and in the same turn press a key on the button. The character does not land. On Windows, right-click a field whose `pointerup` calls `preventDefault`. The edit menu does not open, and the field's `contextmenu` did not call `preventDefault`.

## Direction

Count only `preventDefault` commands appended by the current dispatch, as `pointer_lifted_by` already does, and leave commands an earlier event queued. The loop's `run_commands` remains the drain between turns.
