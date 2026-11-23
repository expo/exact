surfaces: macos, ios
agent: Claude Code (Claude Opus 5.5)
task: the Player and Flight screens' `portal` video on AppKit and UIKit (LLP 1013.002 piece 4, #698)

### Rough

- Neither Apple host drew the player at all, even in the card. The player is hidden at boot (the
  Grid tab has no card), and the Apple host's first tree was made by walking from the roots, so
  no view was made for it; the later `children` op named a view Swift never had. Fixed in
  exact2: the walk (`Host::preorder`) includes the runner's hidden movers.
- Events from the moved Count ran the shown card's ancestors' `pointerdown`, not the
  declaration's. Fixed in exact2 with a `declaredParent` prop the presenter's event walks read.
  On iOS a real touch only reaches recognizers on the views it falls in, so a mover also
  carries a touch relay to its declaration (`--touch platform` showed it).
- UIKit drops the focus of a field whose view changes superview and sends a blur. AppKit keeps
  it, but only when the view moves in one step, so the host now sends the receiving list
  first. Fixed in exact2 (`CarriedFocusIOS.swift`).
- `screenshot` without `window` paints the layer tree, which shows AVKit's picture black. The
  video frames are only in `screenshot … window`.

### Lean in

- AppKit and UIKit already move the same view between parents in either op order; the player
  (an `AVPlayer` the view holds) played on without anything Apple-specific.

### Needed

- tvOS focus and watchOS are undriven, and `press` above a button still bubbles through the
  shown tree (QUEUE).

### Checkpoints

- macOS 27, Player: Count, Open, Back 60 ms in, Open, Count: Count 2, `loads` 1, one video
  (#62, generation 1), `currentTime` 0.60, 0.66, 0.71, 1.15 s. A tab without the card hides it:
  absent from the AX tree and the agent's targets, paused by the offscreen-autoplay rule, and
  it resumes from its position on return. Remove, then New: a new player, Count 0, `loads` 2.
- macOS 27, Flight, clock held 80 ms into the flight: two window captures differ only inside the
  video picture in the moving card (`currentTime` 1.07, 1.34 s); Back 60 ms in, Open: Count 2,
  one video.
- iOS 26.5 simulator, the same: Count 2, `loads` 1, one video (0.55, 0.61, 0.67, 1.06 s); with
  the system Back mid-transition the same. Flight 80 ms in: a 248×210 card between 168×148 and the
  detail, the two captures differ only inside its video (2.40, 4.13 s); the system Back
  mid-flight, then Open: Count 2, one video.
- iOS with `--touch platform` (real touches): Count on the card, in the detail sheet and on the
  card again each run the declaration's `pointerdown` once; Open runs the shown column's.
