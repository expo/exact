# An Android material picker keeps the theme and clock it opened with

**Status:** Closed
**Resolution:** The Material picker reads night mode from LocalConfiguration and watches the 12/24-hour setting while open, telling an open time picker's state when it changes.
**Systems:** host/android
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

The activity stays up across night mode. Its manifest lists `uiMode` in `configChanges`, and `onConfigurationChanged` tells the page. The Material picker does not listen. `Theme` reads `activity.resources.configuration.uiMode` as an ordinary value, so a night-mode change does not recompose it. The dialog keeps the scheme it opened with.

The clock is the same shape. `rememberTimePickerState` takes `is24HourFormat` only as its initial value. The activity watches `TIME_12_24` and sends that to the host. The open picker does not read it again, so the dial stays 12-hour or 24-hour after the system setting changes.

## Where

`host/android/app/src/compose/java/dev/exact/app/Material.kt`, `Theme` and `TimeDialog`. `host/android/app/src/main/AndroidManifest.xml`, `configChanges`. `host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt`, `onConfigurationChanged` and the `TIME_12_24` observer.

## How to reproduce

Open a date or time picker on Android. Switch the system between night and day. The page follows. The dialog does not. With the time picker open, switch the system clock between 12-hour and 24-hour. The dial keeps the mode it opened with.

## Direction

Read the night mode from a composition local that configuration changes invalidate, and pass the current 12/24-hour setting into the time state when that setting changes.
