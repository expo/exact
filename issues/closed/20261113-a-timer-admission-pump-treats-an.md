# A timer admission pump treats an advance stop as continue

**Status:** Closed
**Resolution:** The admission check continues only when `advance` answers `true`; a journal line (a refused timer, the fire limit) ends the turn and `drive` schedules the rest.
**Systems:** host/web-js
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

When a wait-class timer is due, `check` loops while `rt.advance(now, true)` is truthy. `advance` returns true to resume after one change-timer boundary. It returns the journal line when a timer action is not true, or after 4096 commits. That string is truthy. `fired` starts at 0 on the next call, and only a poisoned, agent, or stopped clock aborts the loop.

`drive` is meant to yield at the fire limit and schedule the rest. A due frame, idle, or timeout wait makes `check` run the remainder synchronously, so the cap does not bound the turn. A refused timer also does not stop the rest of the queue.

## Where

`host/web-js/timer-waits.js`, `check`. `host/web-js/rt.js`, `advance` and `drive`.

## How to reproduce

Arm more than 4096 wait-class timers for one instant, or a timer whose action returns a refusal, with later timers due in the same check. The pump keeps calling `advance` in that turn. The later timers still run after the refusal.

## Direction

Continue only when `advance` returns true. On a string, return and let `drive` schedule the next turn.
