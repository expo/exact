surfaces: linux, android, web
agent: Claude Code (Claude Opus 5.5)
task: the viewer's zoomable photo on the painted hosts, LLP 1057.001.000 §8 steps 5 and 6

The photo now zooms on Linux and Android as on iOS, macOS and the web. The app is unchanged; it gained an Android crate (`android/`, as Shared
Elements has) so the Canvas reader can run it.

### Rough

- On Android the zoomable photo first took no pinch: at zoom 1 it has no
  travel, and the presenter left a scroller without travel out of what it
  tells the reader is under a touch. A unit test fed the reader a made-up
  answer and passed; a real pinch on the emulator found it.
- After a pinch, Fit sometimes did nothing on the web and on the painted hosts:
  a zoom the pinch had reported stayed an "echo" after the pinch rested, so the
  app's later write of that value was dropped (D2 holds only a write the
  gesture held to that test).
- The agent's `--touch platform` is iOS's; a real two-finger pinch on the
  emulator took a small program injecting `MotionEvent`s through
  `InputManagerGlobal`, as `adb shell input` does (the console's `event send`
  and `sendevent` reached nothing on a Play image).

### Lean in

- Nothing in the app had to change for three more hosts: the range row, the
  bound `scrollZoom` and the mirroring handler behave as on iOS.

### Needed

- The painted presenter's zoom (drawing, extent, `ctrl`+wheel, double tap,
  the held press) and the Android reader's `ScaleGestureDetector` and
  `GestureDetector` (LLP 1057.001.000 §8).

### Checkpoints

- Linux (headless agent): `ctrl`+wheel of -69.3 takes the photo to 200%, Fit
  back to 100%, 2× centres it; the screenshot matches the web's at 2×. `tap
  zoom-image pinch` and `dblclick touch` answer unsupported.
- Android (emulator, the Canvas reader): a real pinch from a 400 to a 940 px
  span takes it to 207%, a one-finger drag pans it on both axes, a real double
  tap zooms to 200% about the tap and another returns to 100%. The agent's
  `tap zoom-image pinch 2` and `dblclick touch` (through the presenter) give
  200%.
- Web: pinch, `ctrl`+wheel, Fit and the touch double tap as before; a page
  rendered ahead of time shows the photo at its height until it boots.
