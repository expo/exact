# An iOS download reports completed when no export picker can show

**Status:** Open
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A person's iOS download is accepted into a temporary `exact-downloads` directory. `downloadDidFinish` shows the export picker only when a presenter exists and `arm.agent` is false. A missing arm is treated as an agent (`ui?.arm?.agent ?? true`), so the picker is skipped. The download is then reported `completed` with the temporary path.

macOS cancels a download whose web view is in no window. On iOS, the view leaving the window before the file finishes tells the app the download completed. The file stays where the person never chose a destination. `close` does not cancel downloads.

## Where

`host/apple/webarm/FrameWindows.swift`, the iOS branch of the download accept and `downloadDidFinish`.

## How to reproduce

Start a download in an iOS frame, and let the file finish after the frame's web view has no presenter (the view left the window). The app hears `completed`. No export picker was shown. The file is under the temporary directory.

## Direction

If the export picker cannot be presented, report the download cancelled and delete the temporary directory, as the macOS path does when the frame is in no window.
