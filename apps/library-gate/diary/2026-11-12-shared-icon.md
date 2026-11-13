surfaces: ios, macos
agent: Codex
task: alias a shared package icon

### Rough

Two packages declared the same icon independently. Their identical source forms
lost the distinction between their names. The compiler now reports the conflict;
an app design alias resolves it without changing the image.

### Lean in

The app can alias package icons without adding a dependency between the packages.

### Needed

- icons: provided

### Checkpoints

- Contract compilation: smooth with the alias.
