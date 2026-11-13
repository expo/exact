# Android hosted fields skip beforeinput and caret selectionchange

**Status:** Open
**Systems:** host/android, host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

An Android hosted field applies an edit through `field_edit`, which passes `InputDetails::default()` and never calls `beforeinput`. A caret move that does not change the text goes through `note_field`, which stores the selection and does not queue `field_change_tasks`.

Desktop edits go through `edit_field`, which dispatches `beforeinput` and drops the edit when it is prevented, and through `mark_field`, which queues a selection change. Grammar says `beforeinput` can cancel the edit, and a caret move is a `selectionchange`. On Android neither happens for a hosted field.

## Where

`host/linux/src/presenter/hosted.rs`, the reader field edit. `host/linux/src/presenter/field.rs`, `field_edit`, `edit_field`, `note_field`, and `mark_field`.

## How to reproduce

On Android, focus a hosted field whose `beforeinput` calls `preventDefault`, and type. The character lands. Move the caret without changing the text. `selectionchange` does not run. The same field on desktop prevents the character and reports the caret move.

## Direction

Apply a hosted edit through `edit_field`, so `beforeinput` runs first. Record a caret move through `mark_field`, so a changed selection queues `selectionchange`.
