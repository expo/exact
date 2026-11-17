surfaces: android (emulator, API 37)
agent: Claude Code, Claude Opus 5.5
task: drive 3 of `packages/photos/test/android.mjs` failed intermittently

### Rough

- After `prefer extension photos.selection=["agent-5"]` and `clock settle`,
  the grid still showed three photos in most runs. The settle answered before
  the photos extension had acted on the new selection: `prefer` tells the
  reader's extension on its UI thread, and the agent's wait counted only
  requests in flight, not that tell. The `changed` the extension then
  announced was posted to the exact thread, which the settle held, so it
  could not have counted either. Fixed in exact2 (the painted host's wait and
  the reader's `OwnerReplies`); nothing changed in the app or the drive.

### Lean in

`clock settle` then `tree` is still the whole drive: no added wait.

### Needed

none

### Checkpoints

- Drive 3's operations alone: 3 of 4 runs showed three photos before the
  fix, and 1 of 6 at earlier commits of the day; none of 18 after.
- `packages/photos/test/android.mjs --emulator`: all passed, twice (drive 4's
  file checks skip on a Google Play image).
