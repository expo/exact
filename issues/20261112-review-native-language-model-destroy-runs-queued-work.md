# Destroying a native model session runs queued work and can create a surviving clone

**Status:** Open
**Systems:** packages/language-model/apple/LanguageModel.swift, packages/language-model/android/LanguageModel.kt
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Failure

At `f6be99af`, Apple's `drop` aborts responses before clearing the session's
queue (`packages/language-model/apple/LanguageModel.swift:614-618`). Ending
the active response calls `next` (line 296), which executes queued work.
An append waiting behind the response therefore runs during destruction,
and a waiting clone creates a new live session and reports success.

The same order appears in Android's `drop`
(`packages/language-model/android/LanguageModel.kt:492-495`), whose `end`
also advances the queue. Android was inspected, not executed.

Module-wide `destroy` visits a snapshot of sessions. A clone created by
teardown is absent from that snapshot, so it survives even after the
module reports destruction complete. Ordinary prompt completion and
session destruction need different queue transitions.

## Reproduction

Compiled the actual Apple `LanguageModel.swift`, `LanguageModelApple.swift`
and `LanguageModelSchema.swift` with Swift 6.4.2 against a minimal temporary
`ExactPackageContext` stand-in. Used the module's built-in agent model, so
no window, model download or Foundation Models generation was started.

1. Create a session; prefer a held response and start `promptStreaming`.
2. Queue `appendLanguageModel` and `cloneLanguageModel` on that session.
3. Call `destroyLanguageModel`.

Both queued callbacks returned successful values. The original session was
removed, but `module.sessions.count` was 1: the queued clone survived.
Repeating with module-wide `destroy` likewise left one session and returned
a successful clone handle. The queue callbacks ran synchronously during
teardown; no timing assumptions were needed.

Probe: `/tmp/exact-review-20261112/model-swift/main.swift` and its compiled
`probe`. Android and real Foundation Models were not driven.

## Direction and acceptance

Enter a destroying state and settle queued operations as aborted before
ending the active response. Do not use the normal next-operation transition
during teardown. Represent cancellation callbacks for non-prompt queue
entries so none remains unanswered when the queue is discarded.

Test session destruction, dropped session handles and module destruction
with queued prompts, appends and clones. Every queued call must settle once,
no queued operation may begin, and no session may survive module teardown.

## Duplicate check

Checked open and closed filesystem issues, QUEUE.md and all GitHub issue
bodies on 2026-11-12. Existing model issues cover web abort admission,
post-abort tools, platform-cache invalidation and request retention. The
web implementation already cancels its queued entries before aborting its
active response; this report concerns the native implementations.
