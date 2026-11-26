surfaces: ios
agent: Claude Code, Claude Opus 5.5
task: James's reports from Shelf on an iPhone Duo, open and closed: Library's filter, Discover's filter, a tab tapped again, a fold

### Rough
Library's filter was gone on the open Duo. Exact made it the search bar's scope buttons
(LLP 1134). At regular width UIKit sets the search field in the bar's own row and draws no
scope bar there. On the closed Duo the scope bar did show, but:
- after a fold, the list lay 36 points under it;
- as a scroll collapsed the field, it hung over the rows with nothing behind it.

A plain UIKit copy of Shelf's three tabs, built for the comparison and driven with the same
real touches and hinge, behaved the same way. Under the Duo's vertical bar, UIKit counts a
scope bar shown after the bar's layout in no bar height. The filter now stays in the page
wherever the Duo places its vertical bar, and at regular width.

Discover's filter sat under the title, against it and the bar's edge, as UIKit draws a
`largeSubtitleView`. Where the title shares its row with the bar's items (the Duo, open or
closed), it is now a bar item beside the title.

Right after launch on the open Duo, tapping Library's selected tab scrolled the list 52 points
past its top. UIKit laid the bar out tall before it placed the vertical bar, and Exact kept
that inset as the large title's open one. A first fix sampled any title under a vertical bar
as inline. That broke the reselect on the closed Duo, where a search field or a subtitle view
still collapses, and James found it the next round. Now a route that appears with its list at
its top takes the inset it has there.

The agent's frozen clock takes a reselect's unanimated path, and its fold is not the
hinge's, so the overshoot and the 36 points showed only with real taps, the hinge and a
running clock. They were driven with exact2's touch runner and the Duo's hinge helper,
outside `agent`.

Stats held its list still while iOS 27's minimized bar came back on a reversed drag. The
safe-area work fixed that (#1820).

### Lean in
The header said nothing about a bar item or the page. The same tablists became the bar's item
or stayed the page's with no change to their markup.

### Needed
- a filter beside a large title where the bar's title row is shared: provided
- tabular digits for the yearly goal, which changes in place: `font-variant-numeric`

### Checkpoints
- editing app.contract / app.ts: smooth (one attribute)
- iOS build: smooth
- driving the app (`exact.mjs agent`): rough (see Rough, the frozen clock and the fold)
