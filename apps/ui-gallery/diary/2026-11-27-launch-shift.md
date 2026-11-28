surfaces: iOS 27.1 (iPhone Duo simulator, open)
agent: Claude (Opus 5.5)
task: On some launches the Typography page moved up a few points and back down just after it appeared.

### Rough

The range bar's least height is `sidebarBarHeight`, the list column's inline bar height, which the gallery's module measures. The runner keeps a resource's answer across launches, so the first frame drew the bar at the 58 points kept from the last launch. The page then asked the module, which answered 0 because its first check had not run yet; the bar fell to its own height and the content rose 7 points, and when the check measured 58, about 90 ms later, the module said so and the content fell back. Frame by frame, every warm open launch recorded showed it, for 35 to 150 ms. A synchronous `native.call` can only answer what the module knows at that moment, and at launch it knew nothing.

### Lean in

`native.later` let the module say "not yet" by not answering. Until a check has settled the height, `call` now gives none, and the page asks with `native.later`, which the settling check answers; the kept answer stays on screen until the measured one replaces it. After that `call` answers at once, as before, so a fold or an unfold still reads the height in the same turn.

### Needed

- resource answers kept across launches: provided, smooth (the first frame was right; only the early 0 undid it)
- a module answer that waits for the module's own measurement: provided, smooth (`native.later` with the reply held, only until the first settled check)

### Checkpoints

- iOS build: smooth
- seeing the shift: rough (a screen recording, `simctl io recordVideo --display=3`, with the tops of the first text lines read frame by frame; a trace of the module's answers found the order)
- regression test: smooth (`SidebarBarHeightTests`, run as `GalleryRangeBarTests` is, with `SidebarBarHeight.swift` as the extra source)
