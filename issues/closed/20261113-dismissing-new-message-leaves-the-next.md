# Dismissing New Message leaves the next send opening a message id as a thread

**Status:** Closed
**Resolution:** `command("new")` answers its own `created` mutation, and the frame task opens a thread only for an id that mutation returned, so neither a dismissed New Message nor any other change can open a message id as a thread.
**Systems:** apps/messages
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`choose(true)` sets `newConversation`. Dismiss clears `choosing` and leaves the flag set. `animate` opens `changed.id` as a thread whenever the flag is set and a new change arrives.

`command("new")` returns a `thread-N` id. `command("send")` returns the new message id `m-N` through the same slot. After a cancelled New Message, the next send navigates to `/t/m-N`. `snapshot` does not find that id and shows `threads[0]`.

## Where

- `apps/messages/app.contract` `choose`, `dismiss`, and `animate` (the `newConversation` branch)
- `apps/messages/app.ts` `command`: `new` returns the thread id, `send` returns `m.id`

## How to reproduce

Open New Message, dismiss the sheet without choosing a model, then send in the conversation that is open. The app navigates to a thread path whose id is the message just sent, and the screen falls back to the first thread.

## Direction

Clear `newConversation` in `dismiss` and on any other way out of the sheet that is not a completed `new`. Open a thread only for an id `command("new")` returned.
