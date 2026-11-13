# Cancel during a sheet's press slop leaves a drag a later move can settle

**Status:** Open
**Systems:** host/linux, host/android
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A sheet press has three states: a scrim press, a drag still inside the 8-point slop, and a drag that has started. Cancel settles a drag that has started and clears a scrim press. It leaves a drag that is still inside the slop.

`pointer_move` asks the sheet before it asks whether a contact is still down, and a move does not require the button. Past the slop that leftover drag becomes a live drag. The next release settles the sheet and can dismiss the route.

## Where

`host/linux/src/presenter/stage/touch.rs`, `stage_cancel`, `stage_move`, and `stage_up`. `host/linux/src/presenter/contact.rs`, `pointer_cancel` and `pointer_move_at`. `host/linux/src/presenter/stage.rs`, `sheet_top_now`, which follows the drag only once it is moving.

## How to reproduce

Open a sheet that dismisses when pulled down. Press it and, before the finger moves 8 points, press Escape or lose the pointer. Move. The sheet follows the pointer. Release. The sheet settles and can dismiss, with no contact still held from the cancelled press.

## Direction

In `stage_cancel`, drop a drag that has not started, the same way a scrim press is dropped, and do not call `settle_sheet`.
