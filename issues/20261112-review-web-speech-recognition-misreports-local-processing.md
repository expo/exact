# Web speech recognition reports local processing when remote processing is allowed

**Status:** Open
**Systems:** packages/speech-recognition/web/index.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, `browserStatus` sets `local` from the presence of `processLocally` on the browser recognizer's prototype (`packages/speech-recognition/web/index.js:75`). That is a capability check. `start` treats it as the mode of this recognition: `leavesDevice: args.processLocally ? false : !s.local` (`:184`).

On a browser that supports local recognition, both omitted and explicit `processLocally: false` therefore report `leavesDevice: false` and log "on the device" (`:200`). The browser instance still has `processLocally: false` (`:153`), so remote recognition is allowed. Apps relying on the returned privacy disclosure can incorrectly tell the person their speech stays on the device.

The [Web Speech API specification](https://webaudio.github.io/web-speech-api/#dom-speechrecognition-processlocally) defines false as the default and permits the browser to choose local or remote processing in that mode. Only true requires local processing.

Reproduced with the actual module and a `SpeechRecognition` stand-in whose prototype exposes `processLocally`, initially false. The stand-in records the configured mode and does no recognition or networking:

| Requested option | Browser instance mode | Returned `leavesDevice` |
| --- | --- | --- |
| Omitted | false | false |
| false | false | false |
| true | true | false |

Probe: `/tmp/exact-review-20261112/speech-privacy-probe.mjs`, run with Bun 1.4.2. This establishes the incorrect disclosure from the actual code; no speech transmission was performed or measured.

Derive the disclosure from the processing mode selected for this recognition, independently of whether local processing is supported. Without an explicit local requirement or another reliable local-only guarantee, report that audio may leave the device. Add coverage for browsers with and without local support, with the option omitted, false, and true.

Checked all open and closed filesystem issues and the full bodies of all 275 cached GitHub issues for speech recognition, `processLocally`, `leavesDevice`, and local-processing disclosure. No existing issue covers this mode/capability confusion.
