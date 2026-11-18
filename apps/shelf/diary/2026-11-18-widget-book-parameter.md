surfaces: ios, macos, watchos, android
agent: Claude Code, Claude Fable 5.2
task: let a person choose one book for a placed widget

### Rough
The agent's drive stops at the app and its widget gallery. Placing a widget and
choosing in Edit Widget were driven on the Home Screen through the simulator's
accessibility tree, outside `agent`.

An unchosen widget shows the newest book, so choosing the newest book shows no
difference between two placements. The check chose an older one.

Xcode's App Intents metadata pass refused the first generated entity type with
"Unable to get name of variable declaration syntax", for two stored properties
in one `let`. The generator now writes one binding to a declaration.

### Lean in
The app already published its books with `setContentIndex`, so the manifest row,
the inject and the refresh's second argument were the whole change in the app.
Edit Widget listed the books with their authors, searched both, and listed a
book added a moment before, with no app code for any of it. The refresh's
argument was typed `{ book: string | null }`.

The bake asked the refresh with no book chosen and checked its answer.

### Needed
- widgets: provided
- system search: provided

### Checkpoints
- editing app.contract / app.ts: smooth
- iOS build: smooth
- driving the app (`exact.mjs agent`, `exact.mjs test`): rough (see Rough, the Home Screen)
