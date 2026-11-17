# Android calls drop Telecom operations before the scope exists

**Status:** Closed
**Resolution:** An end the list makes frees the call (its entry, the `phoneCall` claim) whether or not Telecom has added it, and once Telecom adds a call its block disconnects one that ended, does a notification button pressed meanwhile, or answers or sets active (and inactive) the entry as it is then.
**Systems:** packages/calls
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

`follow` stores a call's new entry and then skips the Telecom operation when `call.scope` is still null. The next `follow` sees the same state and does not try again. The `addCall` block only calls `setActive` for an outgoing call that is already `connected`, then waits on `call.ended`. A notification answer, decline, or hangup returns immediately when the scope is null.

The call notification is posted in `claim()` before Telecom returns a scope. Ending or answering in that window is dropped. `ended()` never runs, so the system call and the `phoneCall` foreground claim stay until a later listing removes the entry.

## Where

`packages/calls/android/Calls.kt`, `follow`, `add`, and `notificationAction`.

## How to reproduce

List an incoming call. Before `addCall` assigns `call.scope`, tap the notification's answer or decline. The choice is not applied. The Telecom call and the foreground claim remain after the app has marked the call ended.

## Direction

When the `addCall` block starts, apply the entry as it is then: disconnect if it has ended, answer if an incoming call is connected, otherwise set active or inactive. Queue a notification action that arrives with a null scope and run it once the scope exists.
