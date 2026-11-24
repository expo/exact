surfaces: web, ios
agent: Claude Code (Claude Opus 5.5)
task: Place the arranged panes clear of a fold on the web and move them as iOS does.

### Rough

A browser drops `env(viewport-segment-*)` as the device opens, a frame before it reports the posture. The Panes tab's panes were laid out for that frame with no segment at all, 843 pixels wide in a 951-pixel window, and a layout transition had nothing to start from. Finding it took a film of every frame's boxes. `env()` with the last segment from `exactViewport().segments` as its fallback keeps the panes where they were until the posture's commit.

No event reports a hinge that moves in a window of the same size, so `exactViewport().segments` alone leaves the panes at the old one (about ten minutes, as an existing test moves the hinge that way).

Down a fold the panes had to know where their container starts, which CSS does not say. The chrome used to move into the upper pane for that, which moved the title. Now an empty box after the container, as tall as the route below the upper segment, ends the container there, and the lower pane is placed from the route.

A bound `position` of `static` on a scroller is refused (`lower-attr-value`); `relative` is meant. The message says so.

### Lean in

`-exact-layout-transition` is UIKit's own rule for an arrangement: a pane takes its new size at once and its position springs. Filmed on an iPhone Duo simulator at 60 frames a second, UIKit's panes follow a spring of 0.3 s with no bounce, so one property and one spring match it.

### Needed

- foldable segments: provided
- an event for a hinge that moves in place: missing in the browser (worked around with `env()`)
- a container's offset from the viewport in CSS: missing (the empty box)

### Checkpoints

- editing app.contract / app.ts: smooth
- web build: smooth
- driving the app (`exact.mjs agent`, `exact.mjs test`): smooth
