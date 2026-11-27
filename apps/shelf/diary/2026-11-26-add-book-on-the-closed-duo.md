surfaces: ios
agent: Claude Code, Claude Opus 5.5
task: James's report on Add Book on the closed iPhone Duo after the jump fix (#1967): the scroll indicator, scrolling to the Status control, the Status control's alignment, Done over the number pad

### Rough
For each point, Add Book on the closed Duo simulator was compared against two hand-built copies on the same simulator: a UIKit one and a SwiftUI `Form`. All three were driven with the same real touches.

- **Done over the number pad.** The number pad's Done sat right on the keys. The hand-built UIKit copy draws it in the same place: its `UIToolbar` input accessory with a system Done item is what Exact's pad bar is. SwiftUI's keyboard toolbar draws its own button about 1 point above the keys too.
- **The keyboard's empty band.** After the number pad first shows, iOS grows the keyboard twice (269, then 317, then 356 points tall), leaving an empty band between the bar and the keys. The UIKit copy does the same. Exact refits the sheet on each change and on the last one, and the list ends at the bar.
- **Scrolling to the Status control.** It scrolled into view above the bar every time it was tried. The state where it could not was not reproduced.
- **The scroll indicator.** It runs inside the Duo's side strip. In the UIKit copy the list spans the sheet and its indicator runs at the sheet's edge, past the strip's buttons. That is the list's side reach in a sheet, which the safe-area work takes.

### Lean in
The Status control lined up with the text inside the fields, as a SwiftUI `Form`'s segmented picker does; James wanted it lined up with the fields' cards. Writing the tab list's horizontal padding as 0 does that in both the Add Book form and the book's detail. Exact gives a row's ends to the platform's cell margins only while the author leaves them unsaid.
