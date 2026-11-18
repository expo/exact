surfaces: android
agent: Claude Code (Claude Opus 5.5)
task: Draw Android's tonal, outlined and text buttons as Material's, and compare them with Compose in the oracle.

### Rough

A text button has no container, so the oracle's visual box is its title's ink, and a probe in that box lands on a glyph's edge. The `text-button` specimen has no probe; the text and pixel layers compare it. About 10 minutes.

### Lean in

`tokens.mjs` read the new rows against the pinned Compose source and found the one it could not resolve (a private constant), which `InputsTest` then wrote as a `ButtonDefaults` value.

### Needed

none

### Checkpoints

- oracle against Compose, headless: rough (the text button's probe, above); then smooth.
