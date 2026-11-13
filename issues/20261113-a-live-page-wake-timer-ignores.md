# A live-page wake timer ignores a budget raised after delivery

**Status:** Open
**Systems:** host/web-js
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

Off the agent, `deliver` arms `setTimeout(expire, current.budget)` once, when the wake starts. A later `raise` for the same platform id sets `budget` to the max of the events and does not move the timer. `expire` marks the wake expired without reading `started + budget`. The clocked path re-reads that deadline. The runner's `wake_due` does too.

A second wake event with a longer budget, joined after the first delivery, is cut off at the first budget. Queued work ends expired. Agent-driven pages never arm this timer.

## Where

`host/web-js/wakes.js`, `raise`, `deliver`, and `expire`. `runner/src/runner/modules/wakes.rs`, `wake_due`.

## How to reproduce

On a live page, raise a wake and, after it is delivered, raise another event on the same id with a longer budget. The wall timer fires at the first budget. The second invocation ends expired while its own budget is still running.

## Direction

In `expire`, return unless the clock is past `started + budget`. When `raise` increases the budget, clear the timer and arm the remaining delay.
