# Shelf shows a library edit as saved when the write failed

**Status:** Open
**Systems:** apps/shelf
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`change` writes the edited library into `state` and then awaits `persist`. If `atomicWriteFile` throws, the catch returns `{ ok: false }` and leaves the edit in memory. `library` serves that memory, so the screen shows the book, the status, or the notes that were not stored.

`save` pops the add sheet as soon as it sends `addBook`, without reading `changed.ok`. Remove pops the book the same way. Quit and relaunch, and the library is the file from before the edit. A failed read is already refused and covered by a test. A failed write is not.

## Where

- `apps/shelf/app.ts` `change`: `state = edit(state!)` before `persist`, and the catch does not restore the previous value
- `apps/shelf/app.contract` `save`, and the remove actions that `nav = back` after `send changed`

## How to reproduce

Make `atomicWriteFile` throw after a successful load (a full disk, or a test double). Add a book and save. The sheet closes and the library lists the book. Relaunch. The book is absent.

## Direction

Keep the previous `state` until the write resolves, and leave the sheet up with the error when `changed.ok` is false.
