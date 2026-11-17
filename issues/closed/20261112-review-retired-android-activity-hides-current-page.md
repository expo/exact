# A retired Android activity can mark its successor's page hidden

**Status:** Closed
**Resolution:** A stop tells the host the page is hidden only while its activity still reads the host, and detaches only its own window from frame measurement, so a replaced activity's late stop leaves its successor's page and window alone.
**Systems:** host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt, host/android/canvas/src/main/java/dev/exact/canvas/Owner.kt
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, activity replacement keeps the process's host and assigns
`Exact.reader` to the new activity (`ExactActivity.kt:68-78`). The old
activity can stop after that handoff. The host explicitly checks `reads()`
to stop old activity callbacks from changing its successor's state
(`:563-566`), including the gamepad release and the host side of
`view.running(false, reads())` in `onStop`.

The preceding `Exact.hidden(true)` call is unconditional (`:516-520`).
It reaches the process's current host through `Owner.kt:662` and
`Java_dev_exact_canvas_Native_hidden` (`host/linux/src/canvas/jni_extensions.rs:196-202`).
`page_hidden` writes the current runner's page visibility
(`host/linux/src/canvas/requests.rs:93-102`); it receives no reader identity.

A successor can therefore start and publish `hidden=false`, then the old
activity's delayed `onStop` publishes `hidden=true` for that visible
successor. Its `exactPage` visibility and behavior driven by that fact,
including return-lock handling, remain wrong until another start changes
them. The current activity's `onResume` does not restore that page flag.

This is source-confirmed through the Kotlin and JNI paths. The existing
`PadsTest` exercises a replaced reader's late `onStop` to ensure it cannot
release the successor's gamepad state, but its `Native.hidden` shadow is a
no-op, so that test does not observe this parallel visibility write. No
Android device or new Robolectric run is claimed.

Publish lifecycle facts only while the activity still owns the host.
Preserve cleanup local to the retired view and its observers. Extend the
replacement test to record page visibility: start B after A, stop A, and
assert B stays visible; stopping B should then publish hidden exactly once.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies for replaced/retired activity stops,
visibility, and Exact.hidden; no matching report was found.
