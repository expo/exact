# A refused live scrub is recorded as already shown

**Status:** Closed
**Resolution:** apply_live records a scrub frame as shown, and retires the run before it, only once the frame lands (set_tokens returns Ok, or there is nothing to set); a refused frame leaves both as they were.
**Systems:** runner
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A scrub frame whose revision is older than the recorded scrub is dropped. Before `set_tokens`, the runner records the frame as scrubbed and can retire the previous run. `set_tokens` then maps an invalid value to `LiveRefusal` and does not roll that record back.

The same frame sent again is treated as older and returns an empty success. A late frame of the retired run is dropped even though the new frame never landed. Hosts report `Ok` as applied. A newer revision still applies. Resending this revision or the previous run does not.

## Where

`runner/src/runner/live.rs`, `apply_live`. `runner/src/design.rs` treats the same run and an older revision as already shown.

## How to reproduce

Send a live scrub whose token value `set_tokens` refuses. Send that same revision again, or a later frame of the run it retired. The first call is a refusal. The retry is an empty success, and the design is unchanged.

## Direction

Update `scrubbed` and `retired` only after `set_tokens` returns `Ok`, and on the empty-token or same-design path that returns `Ok`. On `LiveRefusal`, leave both as they were.
