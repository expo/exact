# A replaced Android activity marks the visible page hidden

**Status:** Open
**Systems:** host/android
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

Android starts the successor before the predecessor stops. The successor's `onCreate` takes `Exact.reader` and its `onStart` calls `Exact.hidden(false)`. The predecessor's `onStop` then calls `Exact.hidden(true)` with no `reads()` check. `Native.hidden` sets the page's `visibilityState`. The activity on screen reports hidden, so a return lock and anything that reads `exactPage().visibilityState` treat the app as not visible. `onStop` also detaches `windowFrames` on the shared owner after the successor attached them.

Returning to an earlier `ExactActivity` is worse. `read()`, which `onStart` calls after a stop, never assigns `Exact.reader`. The activity that is finishing still reads, so its `onStop` stops the host (`view.running(false, true)`) after the visible one started it, and its `onDestroy` `release()` clears the callbacks `read()` just installed and sets `Exact.reader` to null. The visible activity then has no reader.

The comment on `reads()` says a replaced activity must tell the host nothing. `pads.background` and the host half of `view.running` follow that. `hidden` and `detach` do not.

## Where

`host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt`, `onStart`, `onStop`, `reads`. `host/linux/src/canvas/requests.rs`, `page_hidden`.

## How to reproduce

Rotate the device, or any configuration change this activity does not declare, so Android creates the next activity before the previous one stops. After the new activity is resumed, `visibilityState` is `hidden`.

Start a second `ExactActivity` above another one (`singleTop` still creates that instance when the first is not the top), then go back. The visible activity's reader is null, the host is stopped, and the publish callbacks are empty.

## Direction

Call `Exact.hidden` and `windowFrames.detach` only when `reads()` is true. When a stopped `ExactActivity` starts, set `Exact.reader` to it before `read()`, so the one that is finishing no longer reads and its `release()` leaves the new callbacks in place.
