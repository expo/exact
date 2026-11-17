# A truncated diagnostic makes the benchmark log summary loop forever

**Status:** Closed
**Resolution:** diagnostics.mjs stops at an id the log cuts off instead of restarting its search at the start, keeping every diagnostic before it.
**Systems:** bench/agent-framework/tools/diagnostics.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, `diagnostics.mjs:8-14` searches raw agent logs for
`"id":"lower-`, then searches for the ID's closing quote. If the log ends
inside that ID, `idEnd` is `-1`. Assigning it to `i` restarts the next
`indexOf` at the beginning of the same input. The tool consumes a CPU core
without finishing. An interrupted benchmark run can leave such a log.

Reproduce by saving these 12 bytes to a temporary file and passing its path
to `bun bench/agent-framework/tools/diagnostics.mjs`:

```text
"id":"lower-
```

The actual script under Bun 1.4.2 failed to finish within one second on
this input; the probe terminated only its own child process. There is no
I/O or other work in the repeated loop that can make progress.

Stop parsing an incomplete final ID, or parse complete structured records
before collecting diagnostics. Ensure every iteration advances the input
position. Check an interrupted final record alongside complete records so
earlier valid diagnostics remain available.

Checked open and closed filesystem issues, `QUEUE.md`, and all 275 cached
GitHub issues. No matching report was found.
