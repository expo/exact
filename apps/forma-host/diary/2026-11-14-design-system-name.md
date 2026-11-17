surfaces: web, ios, macos, android
agent: Codex, GPT-6
task: Rename the design system and its fixture app.

### Rough
The compiler embeds the design recipe, so its file must move with the code that names it.

### Lean in
The same design fixture covers generated bindings on the web and native hosts.

### Needed
- design tokens: provided through Forma

### Checkpoints
The design, bindings, crate paths and development protocol use Forma. The rename preserves their behavior.
