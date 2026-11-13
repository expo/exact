# A speech claim after a capture skips ducking when the type matches

**Status:** Open
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`composeSpeechThrowing` returns while any capture is held and leaves `spoken` as it was. `claimCapture` sets `.playAndRecord` and does not clear `spoken`. If the last speech claim ends during that capture, `restoreAfterCaptures` clears `spoken` only when `speeches` is still nonempty, then restores the resting category. `spoken` still names the old type. The next `claimSpeech` of that same type sees `want == spoken` and only calls `setActive`. It does not apply the speech category, so `.duckOthers` and `.interruptSpokenAudioAndMixWithOthers` never apply.

## Where

`host/apple/Sources/ExactKit/AudioSession.swift`: `composeSpeechThrowing`, `claimCapture`, `restoreAfterCaptures`, `claimSpeech`.

## How to reproduce

Claim transient speech, start a capture, release the speech while the capture is held, release the capture, then claim transient speech again. Other audio is not ducked.

## Direction

Clear `spoken` when a capture takes the session, and when the last speech ends during a capture. Apply `speechConfiguration` whenever the live category is not that type.
