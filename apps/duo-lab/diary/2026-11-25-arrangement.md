surfaces: ios
agent: Claude Code (Claude Opus 5.5)
task: Write the Panes and Arrangement screens' panes with the `arrangement` attribute.

### Rough

UIKit takes the view of a pane it hides out of the hierarchy, so the views above a hidden pane do not lead to its box. A test of one arrangement inside another found it; a comment on `UIArrangementViewController`'s hidden panes would have saved an hour.

### Lean in

One attribute replaced a Swift module, a hatch, a `data-` word and their entries in `app.json`, and the panes, their state and the CSS elsewhere are as they were.

### Needed

- two parts that stand together, as UIKit arranges them: provided (`arrangement`)
- an arrangement that reaches the route's sides by default: missing (`-exact-safe-area="left right"` and padded content)

### Checkpoints

- editing app.contract: smooth
- driving the app on the Duo simulator: smooth
