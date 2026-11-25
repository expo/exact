surfaces: ios, web
agent: Claude Code (Claude Opus 5.5)
task: Clear each safe-area inset once on the screens that pad themselves, and keep a note-sheet field in reach on a short laptop fold.

### Rough

Each route clears its own edges of the bars, the side strip and the home indicator (LLP 1123 D3), and the screens that pad themselves also padded by `env(-exact-safe-area-corners-inset-*)`, so on an open iPhone Duo they cleared the 84-point strip twice: the Images and Reflow text ended at 767 where 16 points before the strip is 851, the Combinations column was centred in 867 points, and the note sheet ended 34 points above the display's bottom. `-exact-safe-area` turns a route's clearance off per edge, and the boxes pad by the larger of the corner-adapted `env()` inset and the route's own `var(--exact-safe-area-inset-*)`, which holds UIKit's bars where `env()` does not.

A short window folded like a laptop left the note sheet 80 points of the lower segment, less than its padding and handle, so no field could be reached to focus it.

UIKit's side tab bar took no tap from the agent ("shows no item for its tab"), so the drives typed each location on the root.

### Lean in

Inside a route, `var(--exact-safe-area-inset-*)` is the route's own inset, bars included, and 0 above the web's tab bar, so one expression serves iOS and the web.

### Needed

- turning a route's clearance off by edge: provided (`-exact-safe-area`)
- the route's own inset as a length: provided (`var(--exact-safe-area-inset-*)`)

### Checkpoints

- editing app.contract: smooth
- web build and `duo-panes.test.mjs`: smooth
- driving the app on the Duo simulator: rough (tabs by typed location)
