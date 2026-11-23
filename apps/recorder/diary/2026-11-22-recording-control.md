surfaces: ios
agent: Claude Code (Opus 5.5)
task: give the recorder a control that starts a take from the Lock Screen, for its device qualification

### Rough

The first tool sent `getUserMedia` straight from the press, as the in-app
Record button does. Its `begun` then starts speech recognition on the track,
which the control's effect walk follows, so the control would be omitted
`control-capture`. The control's take has its own chain (`shown`,
`pressOpened`, `pressStarted`) that opens no recognizer, and so no live
transcript.

The Live Activity's placements used `data-privacy="public"`, which the bake
refuses until `app.json` declares the word: `"data": ["privacy"]`.

A press with the microphone permission still `prompt` is refused
`person-required` before anything asks. That is the design (a control may not
prompt), but the take then fails with only `NotAllowedError` on the activity's
chain; the app does not yet show the permission help the reporter describes.

Admitting the press's capture in the runner was not enough: the Apple
microphone started a capture only for a call of origin `app`, so the
recorder would have recorded nothing. exact2 now sends those two calls with
origin `press`, which the microphone starts for.

### Lean in

The bake checks a recording control's prerequisites (a declared activity the
tool starts, `device.microphone-background`) and names each one missing, so
the manifest and the tool came out right on the first bake.

`apple.categoryOptions=["mixWithOthers"]` on `getUserMedia` was already there
for the background start the reporter found iOS needs.

### Needed

- a control that starts a recording: built in exact2 for a qualification
  build only (`EXACT_QUALIFY_CONTROLS=1`, LLP 1126.004 §6d)
- a Live Activity for the take: provided (`live_activities`, `startActivity`)
- stopping a take while locked: missing until the recording hold (piece 4);
  the take is stopped in the app

### Checkpoints

- app.json and app.contract: smooth
- runner admission of the press's capture: built and tested under a fake host
- iOS 26.5 simulator build of the qualification variant: smooth; the intent
  is an `AudioRecordingIntent` and a Live Activity intent in both targets
- a locked iPhone: not on this machine
