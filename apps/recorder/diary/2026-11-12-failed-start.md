surfaces: web, ios, macos, android
agent: Claude Code, Claude Opus 5.5
task: release the microphone when a recording fails to start

## Rough

A mutation's `then` runs only on an answer, so `begun`'s `case none` never saw a refused
`startMediaRecorder`, and nothing stopped the track `getUserMedia` had opened for the take. The
grammar has no failure counterpart to `then`; the answer is a `change` task gated on
`failed(started)` (`key=true`, so each new failure reopens the gate). It took reading the task
section of `contract-grammar.md` to find; no app here used a `change` task before, and a line in
the mutation section ("to react to a failure, gate a `change` task on `failed(m)`") would have
saved the search.

No browser or device run: the test drives the app's own `app.contract` in the runner with the
modules played by the native slot (`contract/cli/tests/it/recorder_start.rs`).

## Lean in

`failure(started)` gives the refusal's code for the message with no extra state.

## Needed

- microphone capture and recording: provided

## Checkpoints

- driving the app: the runner test only
