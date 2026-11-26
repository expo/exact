surfaces: web, ios
agent: Claude Code (Claude Opus 5.5)
task: Move a bottom tab bar's tabs with a fold on the web, as the panes beside them move.

### Rough

- A tab given `-exact-layout-transition` lowered as the bare box (LLP 1104's admission rule), which lost the tab's look and size on the web, and a tab that moved and resized started its icon away from where it showed. Both were fixed in Exact first: a native button now takes the row, takes its new size at once and moves by its centre (LLP 1069.011 D6).
- The bar's padding read `env(viewport-segment-left 1 0)` alone. A browser drops that value a frame before it reports that the device opened, so in that frame the tabs took the window's width again, and the move back had nothing to start from: the tabs jumped as the Duo opened. `foldRight`, the same `env()` with the last segments' left edge as its fallback, as the panes' helpers do, keeps them in place until the app's own commit, which plays the move. The web test showed it only once it watched the unfold as well as the fold.
- Spanning the bar and splitting the tabs around the band, three on the first segment and four on the second, worked on the web, but under the agent's own chrome on iOS a `UITabBar` stands for the tablist at its box and centres its own items, three of them on the band. A plain UIKit app on the Duo simulator showed where UIKit keeps its tab bar there: a rail at the right edge, open and partially folded alike, and, turned, a bar at the bottom of the lower segment. So the tabs keep to the trailing segment, as before, and now move there.
- `neutral.css` gives the web's tab bar its side safe-area padding at no specificity, so the bar's authored `padding-left` replaced the left one; on one segment it now says `env(safe-area-inset-left, 0px)` itself.

### Lean in

- One row on each tab, bound to the panes' spring, moves the tabs with the panes, and the binding's `none` under Reduce Motion places them at once.

### Needed

- foldable segments: provided
- an event for a hinge that moves in place: missing in the browser (the `env()` fallbacks stand in, as for the panes)

### Checkpoints

- editing app.contract / app.ts: smooth
- web build: smooth
