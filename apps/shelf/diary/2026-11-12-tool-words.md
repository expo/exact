surfaces: web, ios, macos
agent: Claude Code (Opus 5.5)
task: settle WebMCP's open decisions (expo/exact#329) with Shelf's two faced tools as the consumer

### Rough

- An agent reading Log Pages' and Add to Shelf's schemas saw `page` and `pages`,
  each titled and nothing more: which one is the book's length was left to the
  tool's description. A face's parameter now takes a `description`, and both
  have one.

### Lean in

- The face digest covers the whole face, so a description joined the
  compatibility id with no further code.

### Needed

- intents: provided (a parameter's `description`)

### Checkpoints

- Add to Shelf is `writes additive closed`: on the web it is no longer
  `consequentialHint`. Log Pages overwrites the page, so it still is.
- App Intents metadata for a described parameter matches Xcode's extractor
  (`host/apple/intents.test.mjs`).
