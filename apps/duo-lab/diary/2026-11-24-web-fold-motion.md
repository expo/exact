surfaces: web, ios
agent: Claude Code (Claude Opus 5.5)
task: Place the arranged panes clear of a fold on the web and move them as iOS does.

### Rough

A browser drops `env(viewport-segment-*)` as the device opens, a frame before it reports the posture. The Panes tab's panes were laid out for that frame with no segment at all, 843 pixels wide in a 951-pixel window, and a layout transition had nothing to start from. Finding it took a film of every frame's boxes. `env()` with the last segment from `exactViewport().segments` as its fallback keeps the panes where they were until the posture's commit.

No event reports a hinge that moves in a window of the same size, so `exactViewport().segments` alone leaves the panes at the old one (about ten minutes, as an existing test moves the hinge that way).

Down a fold the panes had to know where their container starts, which CSS does not say. The chrome used to move into the upper pane for that, which moved the title. Now an empty box after the container, as tall as the route below the upper segment, ends the container there, and the lower pane is placed from the route.

Safari on an iPhone Duo (iOS 27.1) tells a page of no fold, so the panes there stay as they are flat. It does resize the page as the Duo opens or turns, and the panes' spring played then: the second pane crossed the first, with white gaps for over a second. Exact's JS target measured the commit that answers a resize, which LLP 1063 D6 has take its boxes at once; fixed in Exact. Emulated folds in Chromium keep the window's size, so only a film of Safari showed it.

A bound `position` of `static` on a scroller is refused (`lower-attr-value`); `relative` is meant. The message says so.

### Lean in

`-exact-layout-transition` moves a pane where UIKit's arrangement does. Filmed on an iPhone Duo simulator at 60 frames a second, UIKit's panes follow a spring of 0.3 s with no bounce, so one property and one spring match its place. UIKit springs the pane's size, and its rows with it; on the web only the pane's own background springs in size and the rows take their final width at once.

### Needed

- foldable segments: provided (not by Safari)
- an event for a hinge that moves in place: missing in the browser (worked around with `env()`)
- a container's offset from the viewport in CSS: missing (the empty box)

### Checkpoints

- editing app.contract / app.ts: smooth
- web build: smooth
- driving the app (`exact.mjs agent`, `exact.mjs test`): smooth
