# Apple tool polling can return a replacement runner's unrelated result

**Status:** Closed
**Resolution:** The door polls a call through `ExactTools.runner`, which answers only while the runner that accepted the call runs (a `GenerationStamp`), and a poll after a fresh or carrying replacement ends the call once, `interrupted` and `uncertain`, instead of reading the new runner's call by the same id.
**Systems:** host/apple/Sources/ExactKit/ExactTools.swift, runner/src/runner/modules/calls.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `ExactTools.call` submits a tool to a session and captures its
numeric call ID. Its nested `poll` then asks that same session for
`{"op":"called","id":…}` every 50 ms until an outcome appears
(`ExactTools.swift:78-92`). It captures neither the session generation nor
the runner that accepted the call.

Replacing a plan keeps the `ExactSession` object but increments its
generation (`Session.swift:997`). Fresh and carrying boots both initialize
`extensions` with its default value (`runner/src/runner.rs:894`), resetting
`origins.last_id` to zero and discarding the old call table
(`modules/admit.rs:110-145`; `modules/calls.rs:84-91`). The next invocation
can therefore reuse the old call's ID. `call_answer` searches only the
current runner's ended, running and waiting calls by that number
(`modules/calls.rs:410-422`).

If call A is still pending when an update or dev reload replaces the
runner, and call B receives the same ID before A's next poll, A's native
caller can receive B's result and success envelope. The caller may then
believe A completed with a value belonging to another invocation. With no
reused ID, the poll instead sees "no call" and returns an uncertain timeout;
that fallback does not protect the reused-ID case.

This is source-confirmed across the complete tool door and the runner's
allocation, boot and result lookup paths. No UIKit/AppKit invocation was
launched. A controlled regression should submit a pending call, replace
the runner, submit a different call with the first ID, then release the
old poll; the first reply must never contain the second result.

Fence each poll to the accepting runner's generation, or give the door an
invocation identity that survives runner replacement without aliasing.
Finish the old request once with an appropriate superseded/uncertain
outcome when its runner retires. Cover both fresh and carrying replacement,
and replacement between submission and the first delayed poll.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies for ExactTools, tool polling, and
call IDs across generations. The separate presence and iframe callback
reports concern other completion paths; neither guards this tool door.
