# A failed queued deadline keeps a process one-shot's place

**Status:** Open
**Systems:** host
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A process one-shot is admitted into `one_shots` before its queued deadline is armed. The request is independent HTTP, so it is lane 1. When the executor has no async machine, a `timeout_ms` (an `AbortSignal.timeout` or `exactTimeout` on `execFile`) arms `QueuedDeadline` by spawning `exact-queued-deadline`. If that spawn fails, admission returns `"native queued deadline could not be armed"` and does not call `release_one_shot`.

The process thread is never started, so the release on that spawn's failure does not run either. The ticket stays in `one_shots` until the process exits. That map holds at most 16 one-shots and a shared byte ceiling. A ticket left there counts against both, and a later one-shot can be refused with `"native executor process limit reached"` while nothing is running.

## Where

`host/apple/src/executor_admission.rs`, the `one_shot` admit and the `queued_deadline` error return. `host/apple/src/executor_process.rs`, `admit_one_shot` and `release_one_shot`. `runner/src/process.rs`, `Request::process` (independent HTTP). `host/apple/src/executor_core.rs`, `reservation`.

## How to reproduce

Admit an `execFile` whose request has `timeout_ms` set, on an executor whose async machine is absent, while spawning the deadline thread fails. Inspect `one_shots`: the ticket is present. Repeat until new one-shots are refused.

## Direction

Call `release_one_shot` on the deadline-arm error before returning, as the process-thread spawn failure already does.
