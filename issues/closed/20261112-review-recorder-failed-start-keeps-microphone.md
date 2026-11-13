# Recorder keeps the microphone active after recording fails to start

**Status:** Closed
**Resolution:** A `change` task gated on `failed(started)` stops the take's track, clears its identifiers and shows the start's failure code; a retry records on a new track.
**Systems:** Recorder app, media capture lifecycle
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, a successful microphone acquisition followed by a failed
`startMediaRecorder` leaves Recorder holding and metering the microphone.
The screen returns to Record/Ready, offers no Stop or Cancel control,
and displays no error. One concrete trigger is a browser with working
`getUserMedia` but no supported `MediaRecorder` output type.

`apps/recorder/app.contract:81-87` stores the track and starts the
recorder. Its level resource at line 37 and waveform view keep consuming
that track. The `begun` action at lines 90-98 is a success-only `then`;
it does not run when startup fails. The only `stopTrack` call is in the
successful stop path at lines 110-117. Once the failed mutation is no
longer pending, `recording` and `busy` are both false, but `trackId` is
still set and the user has no control that stops it.

Verified through the actual compiled Recorder Contract and Runner, with
a controlled native reply supplying a stream and a failed recorder start:

```text
calls=["getUserMedia", "audioLevel", "startMediaRecorder"]
trackId="track-1" recorderId="" takeName="take-1"
busy=false recording=false
```

No `stopTrack` request follows the failure or subsequent clock steps.
A separate probe of the actual web capture and recording implementations
used a controlled microphone source and omitted `MediaRecorder`:

```json
{"startError":"NotSupportedError","leaseAfterFailure":true,"sourceStopped":false}
```

The first level consumer takes the capture lease
(`packages/media-capture/web/index.js:121-140`), so the fresh-track grace
timer does not close it. Explicit `stopTrack` in the probe stops the source.
No real microphone, browser, or device app was opened. Probes are
`/tmp/exact-review-20261112/recorder-failed-start-probe.rs` and
`/tmp/exact-review-20261112/recorder-failed-start-modules.mjs`.

Handle startup failure as a terminal transition: stop the captured track,
clear the pending take's identifiers, and show the error. Ensure cleanup
belongs to that attempt so a late failure cannot stop a newer recording.
Validate acquisition success followed by start refusal, retry after the
refusal, and successful start/stop.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot. The separate web recording write-failure report concerns
a recorder that started and later reports an unsaved file as successful;
this report concerns app cleanup when no recorder starts.
