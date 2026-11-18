surfaces: android
agent: Claude Code (Claude Opus 5.5)
task: Compare the segmented buttons with Compose in the Material oracle, headless and on an emulator, and fix what differs.

### Rough

The reference measured a segmented row 48 dp tall. `DrawTest` drops the 48 dp touch box only for a control that is the tagged box's child, and a segmented row's controls are one level further down. Every offset inside the row was then 4 dp out, which looked like Exact's error. About 20 minutes.

The first `--phone` run on a fresh emulator compared Compose with itself and found 24 of 36 different. Android's one-time "Viewing full screen" hint was over the catalogue's pictures. Nothing in the run says so; only the pictures show it. About 15 minutes.

### Lean in

The oracle's two repeat pairs (Compose on the device twice, the reader twice) make a bad capture obvious: anything above 0 there is the device, not the change.

### Needed

none

### Checkpoints

- oracle against Compose, headless: rough (the reference's row box, above); then smooth.
- oracle on an API 36 emulator: rough (the full-screen hint); then smooth.
