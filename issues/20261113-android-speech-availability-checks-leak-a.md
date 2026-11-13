# Android speech availability checks leak a thread per call

**Status:** Open
**Systems:** packages/speech-recognition
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`support` passes `Executors.newSingleThreadExecutor()` to `checkRecognitionSupport` and never shuts it down. The callback runs on that executor, posts the result to the main handler, and returns. The pool's thread is non-daemon and blocks on the queue, so the executor cannot be collected.

Each availability check on API 33 or newer leaks a thread. Below API 33, `support` returns before creating one. The transcript writer is one executor and `finish` shuts it down.

## Where

`packages/speech-recognition/android/SpeechRecognition.kt`, `support` and `availability`.

## How to reproduce

Call `speechRecognitionAvailability` repeatedly on API 33 or newer. Each call leaves a live thread named from the default factory. The process's thread count grows.

## Direction

Share one executor for support callbacks, or shut the executor down in the callback after posting the result to the main handler.
