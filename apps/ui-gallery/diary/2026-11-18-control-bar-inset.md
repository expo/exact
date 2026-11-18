surfaces: iOS (run on the iPhone 18 Pro, iOS 27.0, and the iPhone Duo, iOS 27.1, simulators)
agent: Claude (Opus 5.5)
task: Fix the gap between the range bar and each section's page on an iPhone.

### Rough

On the iPhone 18 Pro each section's page started about 56 points below the range bar, where its 12-point padding should have put it: the host made up a scroller's top inset by everything between the route's cleared top and the scroller, the range bar included, as though it were the route's own padding. On the Duo the page sat right, since its bar has no large-title area to clear.

The large title (`Typography`) is laid out under the bar on the 18 Pro but not drawn, on every section, the range bar's or not, and with the bar's minimization off too; on the Duo it is drawn. Not resolved here.

The start screen's list ends flush with the Duo's 84-point strip, with its scroll indicator in the strip: a plain UIKit app with the same split and inset-grouped table does the same, so it stays.

### Lean in

UIKit's own rule decides it: a scroll view is inset by what of the safe area overlaps it, so a box above the scroller is content and gives it nothing.

### Needed

- the host's clearance to make up only the route's own padding (`ClearanceIOS.swift`): provided

### Checkpoints

- With the fix the 18 Pro's Typography page starts 12 points below the range bar (the intro at 236, the bar ending at 224), and its scroll indicator starts at the scroller's top.
- `LocalInsetsIOSTests` passes on the 18 Pro with a new case over `scripts/fixtures/bar-control.contract`.
