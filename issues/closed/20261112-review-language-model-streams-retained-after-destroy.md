# Language-model streaming request maps retain completed responses indefinitely

**Status:** Closed
**Resolution:** A response's request association is removed when it ends, on the web, Apple and Android, so completed streams and a destroyed session's responses are no longer held by it.
**Systems:** packages/language-model/web/index.js, packages/language-model/apple/LanguageModel.swift, packages/language-model/android/LanguageModel.kt
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Failure

At `f6be99af`, `respond()` adds every prompt's response to `byRequest`
(`packages/language-model/web/index.js:249`). The synchronous `prompt()`
wrapper deletes its entry after awaiting completion, but `promptStreaming()`
returns the handle immediately and has no corresponding cleanup.

`end()` limits the public response map to eight completed responses per
session, but each streaming response remains strongly reachable from
`byRequest`. It retains its generated text, tool-call payloads and session.
`drop()` and `destroy()` clear the other maps without clearing these entries,
so even explicitly destroyed model sessions and their transcripts remain
alive for the page's lifetime. The bound advertised in the response-retirement
comment is therefore ineffective for streaming use.

The same ownership error is present in both native implementations, checked
by reading their complete lifecycle code. Apple inserts at
`packages/language-model/apple/LanguageModel.swift:327` and removes only
for nonstreaming settlement or cancellation (lines 471 and 587); its
`end` and `drop` leave the request map intact. Android does the same at
`packages/language-model/android/LanguageModel.kt:236`, with removal only
at lines 329 and 463. Android responses retain their sessions strongly.
Apple uses an unowned session reference, so the retained objects there are
responses, input turns, text and tool-call payloads, not the session itself.
Android was inspected, not executed.

## Reproduction

Ran the unmodified module under Bun 1.4.2 with its built-in agent model,
`rate: 0`, and a distinct `how.request` for each of 100 streaming prompts.
A temporary `Map` subclass recorded the module's three map objects at
construction; the original global `Map` was restored before prompting.
Each response was read to completion, then the session was destroyed.

```text
before destroy: map sizes [ 1, 8, 100 ]
after destroy:  map sizes [ 0, 0, 100 ]
```

The maps are `sessions`, `responses`, and `byRequest`, in declaration order.
The retained values still referenced the destroyed session and its turns.
No browser model, download or network request was used. Both browser and
agent generations share `respond()`, `end()` and `drop()`.

The Apple implementation was also run: all three original language-model
Swift sources were compiled with Swift 6.4.2 against a minimal temporary
`ExactPackageContext` stand-in. Using its agent model and 100 distinct
request numbers produced 8 public responses and 100 request entries;
`destroyLanguageModel` reduced the public maps to zero but left all 100
request entries. The request map was counted with `Mirror`.
No UI or Foundation Models generation was used. Probe:
`/tmp/exact-review-20261112/model-swift/main.swift`.

## Direction and acceptance

Remove a response's request association when it ends or is disposed, and
clear any remaining associations when its session is dropped. Preserve
cancellation of active nonstreaming calls. Test many completed streaming
responses and session destruction against the actual retained maps or weak
references, not only `state().responses`, which already hides the leak.

## Duplicate check

Checked open and closed filesystem issues and the GitHub export on
2026-11-12 for language-model retention, streaming, disposal and `byRequest`.
The three closed web model reports cover abort timing, stale browser-session
reuse and tools invoked after abort. None covers this retained request map.
