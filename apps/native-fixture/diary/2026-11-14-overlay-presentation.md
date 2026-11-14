surfaces: web, ios
agent: Codex
task: Keep the root overlay a layering fixture.

### Rough
The overlay declared a dialog role but exercised only stacking and touch delivery. Removing the role keeps the fixture truthful.

### Lean in
The host owns presentation and accessibility from the declared structure.

### Needed
- native tab layering: provided

### Checkpoints
- iOS simulator: NavigationTabsIOSTests passed, including root-overlay stacking and touch delivery.
