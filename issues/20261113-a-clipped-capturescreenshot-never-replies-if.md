# A clipped captureScreenshot never replies if the frame goes away

**Status:** Open
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`captureScreenshot` with a clip waits on `evaluateJavaScript` for the visual viewport, then returns without calling `answer` when the capture has been invalidated or the web view is gone. The reply closure lives only in that callback. `FrameAutomation.end` answers in-flight evaluations and `performActions`. It does not answer a capture.

Unmount, or a partition change, can happen while a dialog holds that script. The caller keeps the call until the callback. There is no timeout.

## Where

`host/apple/webarm/FrameCapture.swift`, the clip's `evaluateJavaScript` completion. `host/apple/webarm/FrameAutomation.swift`, `end` and the `captureScreenshot` arm of `run`.

## How to reproduce

Drive `captureScreenshot` with a clip. Invalidate the frame before the viewport script returns (unmount the frame, or swap its partition). The call does not answer.

## Direction

Register the capture reply beside the evaluations. Complete it from `end`, and from the early return in the viewport callback, with `NotFoundError`.
