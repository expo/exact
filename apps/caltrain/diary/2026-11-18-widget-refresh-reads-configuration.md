surfaces: ios, macos, watchos, android
agent: Claude Code, Claude Fable 5.2
task: make a configured widget's refresh read its placement's choice

### Rough
The refresh's second argument reaches a Rust data source as a positional
record, in the order the injected shape declares its fields. No generated Rust
type names it: `contract rust` reads the app's plan, and the record belongs to
the entry's plan. The refresh reads `rows` as the record's first field.

An XCTest fixture names this app's refresh for a widget without a configuration,
to get a refusal on the props shape. With two arguments required it would fail
on the argument count instead, so the fixture now declares the same `rows`
configuration.

### Lean in
The bake asks the refresh with the configuration's defaults and checks its
answer, so the new argument is checked before a device runs it. The render kept
its own slice of the departures, which Android needs: its refresh receives the
defaults.

### Needed
- widgets: provided

### Checkpoints
- editing the data crate: smooth
- macOS build: smooth
