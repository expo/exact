surfaces: ios
agent: Claude Code (Claude Opus 5.5)
task: Put the Panes and Arrangement screens' scroll indicators where a UIKit app has them on an iPhone Duo.

### Rough

A route clears its sides, so on an open iPhone Duo the arranged panes ended at the side bar, 867 of 951 points: their halves met at 433.7 instead of the fold, 475.7, and the right pane's indicator lay at 861, inside the bar's items. In a UIKit app with the same arrangement the pane spans to the display's edge, its content ends at 867 and its indicator is at 945. Turning the clearance off was not enough: `var(--exact-safe-area-inset-right)` was the route's in both panes, so the left pane would have been inset by a bar it does not touch. Fixed in Exact: an arranged pane reads its own safe area.

Nothing says where a scroller's indicator is, or that it lies inside the bar. Finding it took a probe in the app, about an hour with the UIKit app to compare.

### Lean in

With the pane's own inset, one expression serves every pose: 84 in the right pane of an open Duo, 0 in the left, 84 in both when they stack on the cover screen, and 0 upright.

### Needed

- a pane's own safe area as a length: provided
- an arrangement and a scroller that reach the route's side by default: missing (`-exact-safe-area="left right"` and padded content)

### Checkpoints

- editing app.contract: smooth
- driving the app on the Duo simulator: smooth
