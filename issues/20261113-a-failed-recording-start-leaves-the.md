# A failed recording start leaves the microphone leased and shows Record

**Status:** Open
**Systems:** apps/recorder, media capture
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`arm` stores `trackId` and sends `startMediaRecorder` in one commit. The waveform binds that id, and the first app consumer leases the capture. If the recorder answers with an error, the mutation fails and `begun` does not run. `trackId` stays set, `recorderId` stays empty, and `recording` is false, so the button says Record and Stop is not drawn.

Nothing calls `stopTrack`. Cancel is only offered while `pending(opened)`. The microphone indicator stays on, and a second Record asks for another capture while the first track is still held.

## Where

- `apps/recorder/app.contract` `arm`, `begun`, `recording`, and the Record button
- `packages/media-capture/web/index.js` `subscribe`: the first app consumer leases
- `packages/recording/web/index.js` `startMediaRecorder` returns `{ error }` for a missing stream, an unsupported type, or a bad path, which fails the mutation

## How to reproduce

Start a recording whose `startMediaRecorder` fails after `getUserMedia` has succeeded (a path the package rejects, or a host that returns an error record). The waveform is live, the button says Record, and the microphone stays in use.

## Direction

On `failed(started)`, call `stopTrack` on the stored `trackId` and clear `trackId` and `takeName`. Do not publish `trackId` until the recorder id exists.
