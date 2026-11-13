# Destroying the last purchases session drops billing replies

**Status:** Open
**Systems:** packages/purchases
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

The Billing client, the connect waiters, and `flowReply` are process-wide. `destroy` of the last session ends the client, clears `waiting` without calling those bodies, and leaves `flowReply` set.

A `purchase`, `getDetails`, or query that was waiting on the first connection never replies. A purchase sheet that was already showing stays locked: the next session's `purchase` returns `NotAllowedError` because `flowReply` is not null, and the original call is not answered when the client is gone.

## Where

`packages/purchases/android/Purchases.kt`, `destroy`, `afterConnect`, the purchases listener, and `purchase`.

## How to reproduce

Start a purchase so the sheet is showing, or call `getDetails` before Billing setup finishes. Destroy the last purchases session. The in-flight call stays open. A new session cannot start a purchase until the process is restarted.

## Direction

On the last session, fail every waiter and `flowReply` with an abort, then clear them, and end the client.
