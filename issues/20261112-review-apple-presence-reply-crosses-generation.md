# An old Apple presence callback can complete a replacement runner's call

**Status:** Open
**Systems:** host/apple, runner/src/runner/modules/presence.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `ExactSession.presenceOp` captures the call number and session,
then sends the eventual platform result to that session's current runtime
(`host/apple/Sources/ExactKit/Presence.swift:175-214`). It captures no session
generation. A plan replacement keeps the session but replaces its runner and
increments `generation` (`Session.swift:992-1000`). A delayed completion from
the old check can therefore reach the new runner.

Call numbers are not unique across those runners. `Presence` derives
`Default`, including its `next` counter, and allocates calls from it
(`runner/src/runner/modules/presence.rs:30-43,304-305`). Both fresh and carried
boots initialize extensions to `Default` (`runner/src/runner.rs:894`).
`Bridge::presence` forwards a result to the current host, and
`presence_reply`/`presence_checked` match only its call number
(`host/apple/src/abi/commands.rs:327-331`;
`runner/src/runner/modules/presence.rs:238-251,569-573`).

A concrete failure is a check started before a plan reload, followed by a new
check with the same number after reload. A late cancellation of the old check
fails the new call. The replacement fence does not prevent this: its pending
request gate applies only to a module replacement, while a plan-only candidate
can commit with pending work (`host/apple/src/abi/preparation.rs:5-10`). The
old platform callback is separate from the executor work retired at commit.
There is also an earlier window: the main-queue closure that starts a presence
operation checks only destruction, not its batch's generation
(`Session.swift:1153-1155`).

A local Swift probe compiled the production `presenceOp` method with an inert
authenticator and runtime. It retained the first callback, changed the fixture
runtime's generation from 1 to 2, started call 0 in the replacement, then
completed the old callback with `USER_CANCEL`. It printed:

```text
old completion delivered to generation=2, call=0, code=USER_CANCEL
```

Probe: `/tmp/exact-review-20261112/presence-generation.swift`. This verifies
the Swift callback routing; the runner's counter reset and number-only reply
matching were checked in source. It does not exercise a real reload, platform
authentication sheet, or keychain.

Fence both operation delivery and its completion by the session generation,
or use an identity that remains unique across runner replacements. Retire the
old check without delivering its cancellation to a new call. Cover a queued
operation overtaken by reload, an outstanding operation completing after
reload, and a current-generation operation completing normally.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies for presence/authentication callbacks
crossing reloads or generations. No matching report was found.
