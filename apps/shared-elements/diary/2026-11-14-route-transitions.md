surfaces: web, linux, android, ios, macos
agent: Claude Code (Claude Opus 5.5)
task: a route's own entrance and exit, the reporter's fade, slide and cut (LLP 1013.001)

### Rough

- On iOS the agent's default timing freezes UIKit's transitions, so a film there shows
  none; `--timing platform` and a simulator recording showed them.

- Moving the existing route into the last arm of an `if` chain left its body one level too
  shallow: the route compiled empty and its children became the root's, with no warning. The
  zoom viewer then showed a blank page, and only the app's own test noticed.
- The first push of a route with parts cut: shared.js loads after first paint only when a
  mounted node asks for it, and the route that declares a part is not mounted at boot. Fixed
  in exact2: a plan with a part asks for it at boot.

### Lean in

- The parts are CSS's: `::view-transition-new` and `::view-transition-old` with `animation`,
  in a `style` the route takes, so the three cases are three small styles.

### Needed

- Nothing for this.

### Checkpoints

- Web (Chromium): Detail slides in and out over Home, which stays still; Camera fades in and
  out full screen; Scanner cuts; Plain keeps the platform's cut. Films under `screenshot …
  over`.
- Linux (headless, `agent.mjs linux`): the same three, on the web's timeline: Detail in
  from 10 % over 250 ms with Home still beneath, out over 200 ms; Camera fading over the
  page.
- Android (emulator, the reader): the same on the agent's clock, stepped. A held Back
  gesture on Detail seeks its exit, full size and square, with Home beneath; cancelled it
  returns, released it pops. Plain keeps Material's shrinking preview.
- iOS (simulator, `--timing platform`, recorded): Detail slides in and out over Home in place
  of UIKit's push; Camera fades over the page in place of the presentation; Plain keeps
  UIKit's push, and an edge swipe on Detail runs UIKit's interactive pop.
- macOS (`--timing platform`, filmed): Detail slides in over Home and out again with Home still
  beneath; Camera fades in and out over the page; Scanner and Plain cut, as AppKit's route
  changes do.
