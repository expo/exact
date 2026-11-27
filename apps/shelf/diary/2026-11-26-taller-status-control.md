surfaces: ios, macos, web
agent: Claude Code, Claude Opus 5.5
task: James: "Can you make the segmented control taller?"

### Rough
The Status control is UIKit's `UISegmentedControl`, and Exact draws it at its own height in any box (LLP 1116.019 D1), so a taller box left it at 31 points. UIKit has no size for it.

SwiftUI's segmented `Picker` does: on the iPhone Duo simulator (iOS 27.1), `.controlSize(.large)` draws the same UIKit control, with the same labels, in a 48-point capsule, at every text size.

`-exact-control-size`, Exact's name for the platform's sizes, was admitted only on a native button. Asking SwiftUI for the height at run time would link SwiftUI, which the build refuses: it would load at every launch.

### Lean in
Exact now takes `-exact-control-size` on a segmented control (LLP 1069.011.001 D8, amended). iOS draws UIKit's control as tall as SwiftUI's picker of that size, with SwiftUI's heights held as measured. The Mac sets `NSSegmentedControl.controlSize`, and on the web the tabs take the larger title a button of that size has.

The Status control writes `-exact-control-size="large"`, in Add Book and in a book's detail.
