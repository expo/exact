# An Apple file picker can deliver its answer to a replacement tree

**Status:** Closed
**Resolution:** A picker request carries the `GenerationStamp` it was shown in and its answer, picked or cancelled, is dropped once that generation has ended; the open mark is kept per generation, so a replacement's input opens its own picker at once.
**Systems:** host/apple/Sources/ExactKit/Picker.swift, host/apple/Sources/ExactKit/Session.swift
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `ExactSession` keeps one `Picker` across its runner
generations (`Session.swift:370-371`). A picker request records a numeric
view ID, an author ID and its filters, but no generation or view identity
(`Picker.swift:109`, `:127-133`). Its completion uses only the numeric ID:
the photo picker at `:350-352`, the document delegate at `:363-372`, and
the Mac panel at `:386-393`.

Reloading the session increments its generation and resets the presenter
(`Session.swift:895-903`, `:997-1012`). Neither presenter reset retires the
session's picker requests. A new runner starts allocating view IDs from
one again (`runner/src/instance.rs:237-254`). An old panel or an unfinished
photo import can therefore finish after the replacement tree is visible.
`deliver` copies the files and sends `picked` to the current runner at the
old numeric ID (`Picker.swift:155-185`); `cancel` similarly dispatches to
that ID (`:149-151`).

For example, open the file picker for A, reload a plan that puts file input
B at A's former view ID, then complete A's pick. B receives the chosen
files and its change handler runs. Cancelling instead can run B's cancel
handler. The old request also keeps its ID in `open`, so a replacement
input with that ID cannot open its own picker until the old one completes.

This is source-confirmed through the retained session, request records,
native completion paths and runtime dispatch. No native picker or UI test
was launched. `ResetGate` only holds runtime calls during synchronous
teardown; it does not reject a completion arriving afterward.

Capture the session generation and the input's identity when presenting.
Retire outstanding requests when that generation ends, and check ownership
before copying files or dispatching completion. Preserve cleanup of staged
photo files and security-scoped access even for an obsolete answer. Cover
both success and cancellation with a deferred completion that fires after
replacement, and verify that the new input can immediately open a picker.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies. The closed
`20261110-picker-answer-after-reload-sets-another-node.md` fixes Linux's date
picker queue; it does not change Apple's file picker. The restart entry in
`QUEUE.md` describes other presenter caches, not these session-owned native
picker requests.
