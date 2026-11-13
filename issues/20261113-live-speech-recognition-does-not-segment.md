# Live speech recognition does not segment at a minute

**Status:** Open
**Systems:** packages/speech-recognition
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

The file reader calls `endAudio` at 50 seconds and starts the next request when `isFinal` arrives. `live` only appends buffers. `answered` continues only for error 1110, or 301 as an abort. Any other task error calls `end`.

The class comment says Apple ends a task at about a minute. Only the file path was given that seam. A continuous live recognition longer than a minute stops instead of starting the next request. `device` recognition that must keep going past a minute does not.

## Where

`packages/speech-recognition/apple/SpeechRecognition.swift`, `RecognizerEngine.live`, `read`, `answered`, and `segmentEnded`.

## How to reproduce

Start a continuous on-device recognition and speak for longer than a minute. The file path seams and continues. The live path ends when the task hits Apple's limit.

## Direction

On the live path, end the current request around 50 seconds the way the file reader does. On the minute-limit error, call `segmentEnded` when `continuous` instead of `end`.
