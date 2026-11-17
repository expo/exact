surfaces: web, ios, macos
agent: Claude Code (Claude Opus 5.5)
task: a photo in a zoomable scroller, the consumer LLP 1057.001.000 §8 names

The viewer now shows the selected photo a second time, alone in a `scroll`
with `-exact-max-zoom=4`, below the transform-binding clip. Fit and 2× write
`scrollZoom`, and the `scroll` handler mirrors `e.scrollZoom` back into the
state they write.

### Rough

- The scroller zooms on iOS, macOS and the web, not yet on Linux:
  `style-undrawn` names that, as it should, and the gallery builds for Linux
  with the warning until the painted hosts zoom (§8 step 5).

### Lean in

- The whole viewer is twelve lines: a range row, one bound prop and one
  handler. No geometry, clamping or pan state of the app's own, where the
  transform binding above it needs `photoFit`, `photoClamp` and a geometry
  action.

### Needed

- Nothing in exact2 for this fixture; the zoomable scroller itself landed in
  the same series (LLP 1057.001.000).

### Checkpoints

- Web: Fit and 2× move the zoom about the port's centre; Chrome's two touch
  points pinch it, a touch double tap zooms to 2× and back, and a touch's press
  on the photo waits for the second tap.
- iOS: 2× centres the photo (offset 181, 160 of a 362 by 320 port), a pinch
  of 1.5 takes it to 3×, and Fit returns it to 1.
- macOS: driven on the series' scratch app (NSScrollView magnification), not
  in this app.
