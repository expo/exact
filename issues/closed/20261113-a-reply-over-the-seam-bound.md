# A reply over the seam bound keeps the module's mutations

**Status:** Closed
**Resolution:** Any failed call to a Rust module, a refusal such as a reply over the seam's bound as well as a trap, discards the instance and activates the module afresh before another call.
**Systems:** logic
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`answer` or `parse` runs, and the data source updates its own fields, before the reply is encoded. `finish` refuses a reply longer than `MAX_MESSAGE` (16 MiB). `dispatch` then returns an error and does not store `output`. `exact_logic_call` reports that as a refusal. The wasm and native executors do not set `trapped` for a refusal, so `run` keeps the same session.

A stateful module can advance a cache or a one-shot counter for an answer the host treats as failed. A retry observes the effect of a call that did not commit.

## Where

`logic/abi/src/lib.rs`, the answer and parse arm, `finish`, and `exact_logic_call`. `logic/src/lib.rs`, `run`, which rebuilds only when `trapped()` is true.

## How to reproduce

Have a Rust data module return an answer whose encoded reply exceeds 16 MiB, after it has updated its own fields. The call fails. The next call uses the updated session.

## Direction

Treat a too-large reply like a trap: discard the executor and activate a fresh instance before another call is accepted.
