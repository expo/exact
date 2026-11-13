surfaces: linux (the app's tests, `agent.mjs linux --test`)
agent: Claude Code, Claude Opus 5.5
task: keep the stepper from editing a tempo the readout is not showing

### Rough

- While a followed MIDI clock ran the transport, the readout showed the clock's tempo
  but Slower and Faster still wrote `bpm`, which nothing showed until the clock was
  dropped, and Share named `bpm`. Of the issue's two directions, disabling the stepper
  while the number is the clock's needs no new text to explain which tempo it edits.
  About 15 minutes.

### Lean in

`disabled=` on the two buttons and one `derive` were the whole change; the test
vocabulary's `expect disabled` checks it on the Linux host as on the others.

### Needed

none

### Checkpoints

- `bun scripts/agent.mjs linux --app metronome --test apps/metronome/app.test.contract`:
  9 passed; the follow test's two new `expect disabled` lines fail before the change.
