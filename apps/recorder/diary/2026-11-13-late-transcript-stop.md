surfaces: web, ios, macos, android
agent: Claude Code, Claude Opus 5.5
task: keep a late transcript stop from clearing the next take's recognition

## Rough

`noted`, the stop's `then`, cleared `recognitionId` whatever take it was, and `busy` left out the
stop's mutation, so a second take could start, and store its own recognition, before the first
take's stop answered. Nothing on the web or in a drive reorders two answers, so the race was found
by reading and reproduced only in the runner: the test answers the media and speech modules from
the native slot and holds `stopSpeechRecognition`'s answer
(`contract/cli/tests/it/recorder_transcript.rs`). A resource's request names its `source`, a
mutation's its `send`; the earlier recorder test answered only sends, which took a run to see.

## Lean in

Clearing the id in the same action that sends the stop needs no extra state: the stop's argument
is read before the assignment.

## Needed

- speech recognition from a track: provided

## Checkpoints

- driving the app: the runner test only
