# A second Expose send while a reply is streaming drops the draft and shows no error

**Status:** Open
**Systems:** apps/expose
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

The composer stays enabled while a reply is streaming. `ask` returns immediately with `Expose is still answering.` and does not append the line. `sendDraft` has already cleared `draft`. That string is the `Turn`'s `error`. The transcript renders `chat.error`, which `conversation` fills from the `errors` map, and this return never writes that map. The words are gone and the thread shows nothing about why.

`askAbout` clears `draft` on the same pattern when it shares an article during a reply.

## Where

- `apps/expose/app.ts` `ask`: the `streaming.get(thread)` return
- `apps/expose/app.contract` `sendDraft` and `askAbout`: `draft = ""` after the `send`
- `apps/expose/app.contract` the transcript's `chat.error` line

## How to reproduce

Send a message to Expose. While the reply bubble is still streaming, type another and send it. The field is empty, the new line is not in the thread, and no error is shown. Clear stays disabled for the whole reply.

## Direction

Ignore Send while `chat.streaming`, and clear the draft only after the line is appended. Surface the refusal on the transcript.
