# A completed clock held leaves its deadline keeping the process alive

**Status:** Closed
**Resolution:** Each clock held look clears its losing deadline timer when the race settles, so a finished wait no longer keeps the process alive until its bound.
**Systems:** scripts/agent-held.mjs, programmatic agent driver
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, each `heldClock` poll races `look()` against `until(end)`
(`scripts/agent-held.mjs:30-33`). The latter creates a referenced timer,
but no code clears it when the look wins. A successful held-request wait
returns promptly while those timers keep the calling process alive until
the original deadline: 20 seconds by default, up to an hour when requested.

This affects programmatic drivers that finish normally after closing their
session. A caller that explicitly exits the process avoids the delay.
Several successful polls also retain one timer each until that deadline.

An isolated Bun 1.4.2 process imported the actual `heldClock`. Its controlled
session returned one held request on the first read and an increased reply
count on the next. With a 750 ms bound it printed:

```text
result { clock: 7, settled: true } returned after ms 27
process exit after ms 752
```

The probe installs no other timers or I/O:
`/tmp/exact-review-20261112/held-clock-timer.mjs`.
It reproduces process lifetime without launching a host.

Clear each losing deadline in a `finally` block, as `untilClock` does at
line 85 and `waitAtMost` does in `agent-wire.mjs`. Validate successful and
failed polls in a child process so the check observes both completion and
exit; checking only the returned result misses this defect.

Checked open and closed filesystem issues and all 275 cached GitHub issues
for held waits, retained timers and deadline cleanup. No matching report
was found.
