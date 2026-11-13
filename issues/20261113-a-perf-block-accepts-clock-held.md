# A perf block accepts clock held and clock until

**Status:** Open
**Systems:** contract
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`perf_block` refuses a clock step only when the argument ends with ` real`. `clock held` and `clock until` also let real time pass with the agent's clock frozen, and their arguments are `held …` and the `until` phrase, so they are kept. The driver runs every inner step between the two perf reads. A held reply or an until poll can land work during the wait, so `query`, `built`, and `evaluated` depend on the machine. The comment on `perf` says a step that waits on the wall clock is refused.

## Where

`contract/syntax/src/parser/steps.rs`, `perf_block`, and the `clock held` and `clock until` arms.

## How to reproduce

Put `clock held` or `clock until state …` inside a `perf` block. The test is accepted, and the counts include whatever the wait let the host do.

## Direction

Refuse a `clock` step inside `perf` when the argument is or starts with `held` or `until`, or ends with ` real`, with the wall-clock message the `real` arm already uses.
