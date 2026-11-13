surfaces: ios, macos, watchos
agent: Codex, GPT-6 Astra
task: add a placement-specific widget display choice

### Rough
The widget manifest rejected configuration. The framework now generates fixed
enum choices for Edit Widget and passes them to a snapshot inject. The build
must use AlarmKit's module name for both Swift compilation and metadata extraction.
The simulator's first Edit Widget sheet ignored choice taps; closing and
reopening the sheet made the chooser respond. watchOS uses recommendations
instead of a parameter form; each declared combination needs its own intent.

### Lean in
The existing snapshot driver, gallery and agent share the same configuration
boot input. Changing the row count needs no new data source.

### Needed
Each widget placement needs its own display choice. Refresh still supplies one
shared timeline; configuration does not choose what to fetch.

### Checkpoints
The snapshot compiler's 34 tests and Apple Rust boot's 10 tests passed.
The resumed run passed six bake tests, eleven JS snapshot tests, both generator
tests, touched-crate clippy, caps and the quick semantics comparison. Generated
intent code compiled and produced metadata for iOS with AlarmKit, macOS and watchOS.
The iOS XCTest run passed 94 tests, with one baseline-comparison test skipped.

On an iPhone 17 Pro simulator running iOS 27, the placed widget changed from two
departures to one through Edit Widget. Installing this build over origin/main
preserved the placed widget's kind and identity; it kept rendering two departures,
and Edit Widget showed the default choice of two. Screenshots and SpringBoard
state captured both checks.

The watchOS app and configured complication compiled and signed. The macOS app
compiled and signed ad hoc, but its widget extension was omitted because this
machine had no valid signing identity. A signed Mac widget build, physical-device
selection and upgrades between configured versions remain unverified.

The watch recommendation probe on a watchOS 27 Series 11 simulator took both
intents from the generated provider and drew the real snapshot plan. The first
rendered two departures and the second one, checked in the laid-out nodes and
a screenshot. This was a temporary app, not selection through the watch-face
picker: UI automation lacked macOS Accessibility and Screen Recording access.
The review-fix run passed 25 snapshot compiler tests and 95 iOS XCTests, with one
external-baseline comparison skipped. The watch app and extension built and
passed deep signature verification. The generator's unavailable-SDK path reports
three skipped compilation tests; the three source tests still run.

The second review round reproduced two generator edge cases: Swift collapsed
two Unicode spellings of a choice, and watch labels could collide after adding
value identifiers. UTF-8 hex raw values preserve the original bytes through an
explicit mapping; colliding complete labels receive unique choice suffixes.
The six generator tests passed, including compilation and metadata on all three
Apple SDKs and an executed macOS Unicode round trip. Six scoped widget packaging
tests also passed. This round did not repeat the installed-widget drives.
