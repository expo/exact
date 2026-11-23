surfaces: android (emulator, API 36), macos (agent), web (Chromium), linux (headless)
agent: Claude Code, Claude Opus 5.5
task: `photoChanges(since=, limit=, subtypes=)` and its tokens (expo/exact#714, LLP 1133.019.000 piece 3)

### Rough

- A resource has no way to wait: mounting it reads it, and a `when-prompt`
  read at launch would hold a second ticket beside the list's. The Changes
  panel is a component of its own, mounted under `if watching`.
- The drive's first expectation was wrong, not the walk: from the first
  checkpoint a fixture deleted and restored is an update, since it was there
  before the window.

### Lean in

The panel is Contract alone: `changes.added.map(a => a.id).join(",")` shows a
list, `failure(changes)` its error, and `advance(changes.token)` takes the next
page. Typing into `since` drives every malformed and foreign token.

### Needed

none

### Checkpoints

- `packages/photos/test/changes.mjs android`: 34 agent cases and 4 against
  MediaStore with the implementation bound (rescan-only).
- `packages/photos/test/changes.mjs macos`: the same 34 agent cases.
