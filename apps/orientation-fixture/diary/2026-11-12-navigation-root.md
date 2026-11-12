surfaces: linux, ios, macos, web, android
agent: Codex
task: keep a fixture's navigation root at the app's first element

### Rough

The navigation placement check exposed a stack behind the fixture's outer
column. Merging the column and navigation root makes its screens direct children
and keeps the root's orientation lock around them.

The first Linux development build lost its incremental output directory and
Rust panicked. Building with `CARGO_INCREMENTAL=0` succeeded.

### Lean in

The compiler's app sweep found the misplaced stack.

### Needed

- navigation: provided
- orientation: provided; Linux reports that applying a lock is unavailable

### Checkpoints

- editing app.contract: rough; see Rough
- Linux build: rough; see Rough
- driving the app on Linux: smooth; all 20 checks passed
