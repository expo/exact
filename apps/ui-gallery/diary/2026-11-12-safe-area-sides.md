surfaces: iOS (not run here)
agent: Claude (Opus 5.5)
task: Keep the gallery's side insets as routes start clearing their safe area (LLP 1123 D3).

### Rough

The gallery's pages pad `env(safe-area-inset-left)` and `-right` on boxes inside each route, under `viewport-fit="cover"`. Routes now clear their whole safe area, and padding on a box inside the route is not credited, so in landscape the sensor housing's inset would have been cleared twice.

### Lean in

`-exact-safe-area="left right"` on the route leaves the sides to the page's own padding, which also serves the web, where no route is cleared. The top and bottom now clear the bars and the home indicator, which the page did not pad itself.

### Needed

- automatic clearance per route with a per-edge opt-out (LLP 1123 D3, D4): provided

### Checkpoints

- The route gained `-exact-safe-area="left right"`; the app compiles. Not driven on a device or simulator in this change.
