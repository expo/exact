surfaces: iOS (not run here)
agent: Claude (Opus 5.5)
task: Keep the app's own inset handling as routes start clearing their safe area (LLP 1123 D3).

### Rough

Under `viewport-fit="cover"` iOS used to clear a route only of what its safe area held beyond `env()`, assuming the page padded `env()` itself; this app's routes, which have no bar and pad the home indicator with a box of their own, were cleared of nothing. Routes now clear their whole safe area unless the route's own padding reads the inset, which would have added the status bar above and the home indicator below a second time.

### Lean in

`-exact-safe-area="none"` on the route says the app handles every edge itself, which is what it already did, so nothing moves.

### Needed

- automatic clearance per route (LLP 1123 D3, D4): provided

### Checkpoints

- The route gained `-exact-safe-area="none"`; the app compiles. Not driven on a device or simulator in this change.
