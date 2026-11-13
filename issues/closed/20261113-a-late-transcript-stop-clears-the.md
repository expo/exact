# A late transcript stop clears the next recording's recognition id

**Status:** Closed
**Resolution:** Recorder clears recognitionId as it sends the stop rather than when the stop answers, and counts a pending transcript stop as busy, so a late answer cannot clear the next take's recognition.
**Systems:** apps/recorder, speech recognition
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`noted` sets `recognitionId` to empty whenever `stopSpeechRecognition` answers, before it looks at which recognition that answer belongs to. `busy` is `pending(opened)`, `pending(started)`, or `pending(saved)`. It ignores `pending(transcribed)`, so Record is enabled as soon as the file is saved.

The next take can store a new recognition id before that stop's answer runs. `noted` then wipes the newer id. The following Stop sees an empty id and sends `abortSpeechRecognition("")`, which does not stop the live recognizer, and its transcript is never kept.

## Where

`apps/recorder/app.contract`: `noted` assigns `recognitionId = ""` unconditionally; `busy` omits `pending(transcribed)`; `toggle` sends `stopSpeechRecognition` and then `filed` clears the recorder without waiting for that answer.

## How to reproduce

Stop a take that has a live transcript, then start another and reach `listened` before the first stop answers. The second take's `recognitionId` becomes empty. Stop does not abort the recognizer that is running.

## Direction

Clear `recognitionId` only when it is still the id this stop was sent for. Treat `pending(transcribed)` as busy until that answer lands.
