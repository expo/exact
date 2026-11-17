# An Apple language-model abort leaves its platform session marked current

**Status:** Closed
**Resolution:** When the response generating aborts or fails, end drops the session's Foundation Models session before the next queued call begins, so the next prompt rebuilds it from the transcript.
**Systems:** packages/language-model/apple/LanguageModel.swift, packages/language-model/apple/LanguageModelApple.swift
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Failure

At `f6be99af`, Apple's `end` commits an aborted response's input and partial
reply, cancels its task, then admits the next queued operation without
marking `s.platform` stale
(`packages/language-model/apple/LanguageModel.swift:276-296`).

The generation's error handler would mark the session stale, but its first
check returns when the response has ended
(`packages/language-model/apple/LanguageModelApple.swift:232-244`). The
cancellation therefore bypasses that invalidation. `platform` reuses the
cached object while `s.stale` is false (line 143).

The code's own transcript rule says cancelled replies are absent from
Foundation Models' session and the next prompt must rebuild it. Reusing the
cache can make that prompt omit the aborted turn's committed input or
partial reply. A queued prompt can also reach the same platform session
before its cancelled generation has unwound.

## Verification

Compiled the three actual Apple language-model Swift sources with Swift
6.4.2 against a minimal temporary `ExactPackageContext` stand-in. Created an
agent-model session, started a held streaming response, installed an
`NSObject` as the cached platform object with `stale = false`, and called
`abortLanguageModelResponse`. The observed bookkeeping was:

```text
abort: cachedPlatform=true, stale=false
```

This executes the shared abort/end transition. The cache-reuse and error
handler paths were inspected in the real source. Foundation Models was not
invoked, so missing context and concurrent-generation failures remain
consequences to qualify on that framework. Probe:
`/tmp/exact-review-20261112/model-swift/main.swift`.

## Direction and acceptance

Invalidate the platform cache when an active response aborts, before the
next queued operation can begin. Keep the committed transcript as the
rebuild source. Test empty and partially generated aborted replies, with
the next prompt already queued, and verify its reconstructed context on
Foundation Models.

## Duplicate check

QUEUE.md already records “Apple's language-model session stays fresh after
an app abort”. No filesystem or GitHub issue covers the Apple path as of
2026-11-12. The closed
`20261110-language-model-web-keeps-browser-session-after-app-abort.md`
records and fixes the analogous web path; its fix does not reach Swift.
