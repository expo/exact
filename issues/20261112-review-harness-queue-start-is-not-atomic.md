# Concurrent harness queue pumps can start two workers for one turn

**Status:** Open
**Systems:** harness agent, prompt queue, turn ownership
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `apps/harness/data/src/agent.rs:126–139` checks that the
session is idle and removes the next prompt while holding the state lock,
then drops that lock before `start()` installs a turn. `start()` does not
check again. Its later read of `s.turn` at line 68 can also capture a turn
installed by another caller, instead of the turn it just created.

This can occur when a new prompt arrives as the previous turn finishes.
The completion thread calls `pump()` at lines 99 or 120, while the submit
path independently calls it from `commands.rs:50–61`. Cloned `Harness`
values also explicitly share a session (`lib.rs:58–65`).

With prompts A and B queued, a valid interleaving is:

1. Pump A observes idle, removes A, and releases the lock.
2. Pump B observes idle, removes B, and installs turn 1 for B.
3. Pump A installs turn 2 for A and captures turn 2 for its worker.
4. Pump B reads the current turn at line 68 and captures turn 2 too.

Both workers then pass `State::current()` because it checks the shared
turn ID and cancellation flag (`state.rs:285–292`). They can concurrently
stream replies, replace the same `round` state, and run tool calls under
one turn identity. Prompt order is already reversed before either worker
starts. If each captures a different ID instead, one accepted prompt is
superseded without receiving its own answer.

A deterministic standard-library probe copies the current `start()` and
`pump()` function bodies verbatim from `agent.rs`. Small state, provider,
and worker stand-ins expose the lock boundaries; barriers select the
interleaving above. It reports:

```text
queued A then B; history order: ["B", "A"]
worker turn IDs: [2, 2]; current turn: 2
```

Run `python3 /tmp/exact-review-20261112/harness-start-race/run.py` to
extract the functions, compile the probe, and assert both observations.
This verifies the queue and ownership race; it does not call a model or
execute any tool, and does not measure how often ordinary scheduling
hits the window.

Reserve a turn, remove its prompt, and capture that turn's identity in
one locked transition. Pass the captured turn to its worker. Test a
completion racing with submission and concurrent submissions through
clones: every accepted prompt must run once, in queue order, with at most
one worker owning the current turn.

Checked all open and closed filesystem issues and the full bodies of the
275-issue GitHub snapshot. The closed harness busy-seconds report concerns
the UI timer; no queue admission or duplicate-worker report was found.
