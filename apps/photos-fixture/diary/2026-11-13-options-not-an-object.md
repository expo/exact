surfaces: none (the data source, by its Rust test)
agent: Claude Code, Claude Opus 5.5
task: refuse Options text that is valid JSON but not an object

### Rough

- `[]`, `123` or `"text"` in the Options field panicked the data source at the
  `destination` it writes into the record, before any storage request; malformed JSON
  was already refused. The crate turned its test harnesses off (`test = false`), which
  `caps` refuses once it holds a test, so they are on again. About 10 minutes.

### Lean in

The refusal is the same `DataError` malformed JSON already gets, so the drive's
error path shows it with no change to the app.

### Needed

none

### Checkpoints

- `cargo test -p photos-fixture-data`: empty text, `{}`, `null` and an object make the
  first storage request; an array, a number, a string, a boolean and malformed JSON
  are refused. The test panics before the change.
