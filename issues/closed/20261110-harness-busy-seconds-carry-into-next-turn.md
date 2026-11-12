# The harness's "Thinking… (Ns" line starts each turn at the seconds every earlier turn took

**Status:** Closed
**Resolution:** A prompt sent while idle sets the busy line's seconds back to 0 and records the session's output tokens, so the line counts the running turn's time and tokens.
**Systems:** apps/harness (terminal.contract)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

While a turn runs the screen shows `✻ Thinking… (3s · ↓ 0 tokens · esc to interrupt)` (`apps/harness/terminal.contract:254-257` at `dd6f5dcd`). The seconds come from `busyMs`, which `step` adds to on every 100 ms tick of the `spin` task (`terminal.contract:109-110,119-123`) and which nothing sets back to 0. `sendText` marks a turn's start by setting `stepped` (`terminal.contract:150-153`, "A turn starts when its prompt is sent"), so the first step of a turn measures from the send, but it adds that to the total of all earlier turns.

Nothing else says what the seconds count: LLP 1101 and its sub-documents do not describe the line, and no test reads it. The line names the running turn's phase and how to interrupt it, and the comment at line 150 speaks of a turn's start, so the reading taken here is that it times the turn.

## The failure

Send a prompt, let the turn run 2.6 s, press Escape. Send a second prompt and read the line 0.3 s later.

- Observed: `✽ Thinking… (3s · ↓ 0 tokens · esc to interrupt)`.
- Expected: `(0s`. After an hour of use the line starts each turn at several hundred seconds.

## Verification

Drove the real screen over the offline mock model (`Host::boot` with `terminal.contract`, `Harness::with_options(Options::offline(None))`) and read the line from the terminal emulator:

```
turn 1 after 2.6 s: "✳ Writing… (2s · ↓ 0 tokens · esc to interrupt)"
turn 2 after 0.3 s: "✽ Thinking… (3s · ↓ 0 tokens · esc to interrupt)"
```

## Direction

Set `busyMs = 0` where `sendText` sets `stepped` for a turn that starts idle. The token count beside it is `session.tokensOut`, the session's total (`apps/harness/data/src/agent.rs:257`); if the line describes the turn, it wants the turn's count as well.

## Related

`docs/agent-pitfalls.md:317-323` cites this arithmetic as the way to measure time; it does not say when the total starts over. Nothing in `issues/` or `QUEUE.md` names the line.
