surfaces: ios (iPhone Duo simulator, iOS 27.1; iPhone 18 Pro simulator, iOS 27.0), macos, web
agent: Claude Code (Claude Opus 5.5)
task: the deck's 150-point throw on iOS, which left the top card in place

### Rough

- A driven 150-point drag released the card at 95 to 127 points across six runs, under the deck's 120 in four, and every 140-point drag at 109 to 118.3: UIKit's translation for a touch starts where its pan recognizer began, past its hysteresis (27.7 points after touch-down in one instrumented drive), and the iOS transform drag counted from there. Fixed in exact2: a one-finger transform pan is anchored where the finger touched down.
- The release's arguments are not in the journal (`transformrelease view …` alone); a state the action wrote, shown in a text, read them (`QUEUE.md`).

### Lean in

- `transformrelease` gives the card's position and velocity, so a throw is one comparison in the action.

### Needed

- The fix in exact2 (LLP 1002's note of 2026-11-23).

### Checkpoints

- iOS, both simulators, `--touch platform`, three runs each: `tap deck-handle drag 140 0` releases at 134.3 and throws the card (109 to 118.3 before); `drag 120 0` releases at 115.3 and springs back.
- `apps/interaction-gallery/app.test.contract`, whose rightward throw drags 140 and whose 120-point drag springs back, on `ios` (both simulators), `macos` and `web`.
