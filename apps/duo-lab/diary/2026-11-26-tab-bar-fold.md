surfaces: web, ios
agent: Claude Code (Claude Opus 5.5)
task: Lay a bottom tab bar's tabs out on both sides of a fold's band on the web, and move them there as the panes beside them move.

### Rough

- A tab given `-exact-layout-transition` lowered as the bare box (LLP 1104's admission rule), which lost the tab's look and size on the web, and a tab that moved and resized started its icon away from where it showed. Both were fixed in Exact first: a native button now takes the row, takes its new size at once and moves by its centre (LLP 1069.011 D6).
- Tabs sized by `env(viewport-segment-*)` alone jump as the Duo opens: a browser drops those values a frame before it reports one segment, so in that frame the tabs take their content's width, and the move back starts there. `foldFirst` and `foldBand`, the same `env()` with the last segments' edges as fallbacks, as the panes' helpers do, keep the tabs in place until the app's own commit, which plays the move. Every tab still animates in that case, so the web test checks where each move starts, from its first keyframe, and where each tab is drawn in the frames it samples: between where it was and where it goes.
- Under the agent's own chrome on iOS a `UITabBar` stands for the tablist at its box and centres its own items, three of them on the band, and reports those items as the tabs' boxes. So the smoke's web leg leaves the tabs out of its band check on a book fold, and the Chromium test (`host/web-js/duo-panes.test.mjs`) checks the web's own. The iOS app shows UIKit's own tab bar, which on the Duo is a rail at the right edge, open and partially folded alike.
- The web pads its tab bar by the side safe areas only while the bar ends at the window's bottom, so above Safari's keyboard the padding goes. The first segment's tabs share what is left after the left inset, so the bar writes its left padding itself, the same `env(safe-area-inset-left, 0px)`.

### Lean in

- One row on each tab, bound to the panes' spring, moves the tabs with the panes, and the binding's `none` under Reduce Motion places them at once. A tab is a native button, so it takes its new width at once and its centre moves straight to its new place.

### Needed

- foldable segments: provided
- an event for a hinge that moves in place: missing in the browser (the `env()` fallbacks stand in, as for the panes)

### Checkpoints

- editing app.contract / app.ts: smooth
- web build: smooth
