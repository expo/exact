surfaces: iOS (built here; run only on iOS 27, which this machine has)
agent: Claude (Opus 5.5)
task: Draw the range bar's material on every iOS version that clears the navigation bar over it.

### Rough

The module's `element` hook began with one `#available(iOS 26, *)` guard for everything it builds, so the range bar's material went with the HDR reference it guarded. `route` clears the bar on every version, so iOS 17 through 25 showed a clear header with nothing under it.

### Lean in

Keeping each availability check around only the API that needs it (`CGColor(headroom:)`, `preferredDynamicRange`) keeps the plain UIKit material on every version.

### Needed

- nothing new from exact2

### Checkpoints

- `bun host/apple/build.mjs --ios ui-gallery-apple --no-install` builds. No iOS 17 to 25 simulator runtime is installed here, so the clear header was not seen before or after.
