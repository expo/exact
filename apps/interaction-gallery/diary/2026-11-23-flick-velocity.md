surfaces: ios (iPhone 17 simulator, iOS 26.5), macos, linux, web
agent: Claude Code (Claude Opus 5.5)
task: a short flick of the deck and the reading sheet, judged by its release velocity (expo/exact#1443)

### Rough

- On iOS a driven flick released at the driver's clock, not the finger's: the clock moves in steps while a finger is down, so a short drag's samples landed on one or two instants and the velocity was 0 or a fraction of the finger's. Fixed in exact2: a hold's samples carry the touch's own time (LLP 1057.001 §3).
- A 60-point flick of the sheet's header still released at rest on iOS: UIKit recognized the pan on its last move, and its translation starts there, so the hold saw no motion. Fixed in exact2 as #1419 did for the deck: the height drag counts the finger's travel from where and when it touched down.

### Lean in

- `heightrelease` and `transformrelease` give the velocity, so a flick is one comparison in the action.

### Needed

- The fixes in exact2 above.

### Checkpoints

- `apps/interaction-gallery/app.test.contract`'s two flick tests (the card thrown by 80 points in 40 ms; the sheet carried to peek by 60 points in 40 ms) on `ios`, `macos` (and `--timing platform`), `linux` (and `--timing platform`) and `web`. On iOS the sheet flick failed once in 41 runs, the second after a rebuild, and not again.
