surfaces: ios, web, macos
agent: Claude Code, Claude Opus 5.5
task: a UIKit arrangement screen rebuilt in Exact

### Rough

- A pane scroller sized `height="100%"` came out 116 points taller than UIKit's allocation, and its last rows never scrolled into view. The kernel laid a scroller's platform insets (82 under the bar, 34 over the home indicator) out as padding, which a `content-box` height adds to. Fixed in exact2: the kernel lays the insets out as a gutter below the padding box, as a horizontal scrollbar's, so the box keeps the size its `height` gives it. Finding it took about five minutes, from the pane's frame against UIKit's allocation.
- Without a height, a pane in native layout took its content's height (11,216 points) and did not scroll, since a pane scrolls only when its content is taller than it. Documented in exact2: the guide says how a pane is sized in its allocation.
- After any option change, a slow drag in the primary pane left the bar expanded where the UIKit reference minimizes it. The cause was the route hatch's `setContentScrollView(nil, for: .top)` for Automatic, as the reference sets it: Exact took the nil as the app holding the edge with no scroll view, while UIKit's nil is its automatic selection. UIKit cannot find an authored scroller through Exact's views, so Exact makes that selection; fixed in exact2 so a nil returns the edge to it. Reproducing it against the reference took about five minutes.
- The secondary pane's rows run under the open Duo's side strip, where the reference ends them at the pane's safe area. Arranged panes get no side clearance, and Contract cannot read a pane's own safe area yet. Queued; a pane's own safe area in Contract (LLP 1123 D3) would have helped.
- Hatching a pane scroller made the development log attribute the scroll view's own gesture recognizers to the hatch. Fixed in exact2: the element hatch's check leaves out the recognizers already beneath the node before the call.
- From the second screen on, UIKit moves the bar's trailing items into an overflow menu beside Back, which the agent cannot open: under `--chrome platform` a tap on Next is refused because a `_UIButtonBarButton` covers it. Deeper screens were reached by typing their locations, and Next was pressed with Exact drawing the header. An agent operation that opens UIKit's overflow menu and presses an item in it would have helped.
- With the Duo closed, the tab is under More, whose bar More's list shares. The route hatch leaves that bar alone, since `route.navigation` is nil for a stack UIKit built, so Title Display Large does not turn large titles on. Nothing in Exact would have helped: UIKit's More list owns that bar.

### Lean in

- The route hatch sets UIKit's own `largeTitleDisplayMode`, `navigationBarMinimization` and `setContentScrollView(_:for:)` from `data-*` words; it runs after Exact projects the header, so they hold. Real drags match the reference: the primary pane minimizes the shared bar and the untouched pane moves up 58 points; Never, a secondary selection and fold poses behave as UIKit does.
- The header's buttons and `role="menu"` popovers become UIKit's bar items and nested `UIMenu`s with checkmarks, with no native menu code.
- A `state` initialized from a prop gives each pushed screen its own settings, seeded from the screen that pushed it.

### Needed

- Navigation item options for arranged panes: provided through the route hatch
- The pane scroller for an explicit bar selection: by hand, an `arrangement-pane` element hatch
- A pane's own side safe area: missing

### Checkpoints

- editing app.contract: rough (the pane heights, under Rough)
- iOS build: smooth
- web dev loop: smooth
- driving the app: rough (the nil selection, the side strip, the development log, the overflow menu and More's bar, under Rough)
