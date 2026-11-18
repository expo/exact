surfaces: iOS 27.1 (Probe Duo simulator, open and folded), iOS 27.0 (iPhone 18 Pro simulator)
agent: Claude (Opus 5.5)
task: The sidebar covered the page, the header no longer collapsed, and the title was not the platform's.

### Rough

On the open Duo, UIKit's sidebar floats over the content's column and insets that column by its width. The pages padded their sides by `env(safe-area-inset-left)`, the window's inset, which holds no sidebar, so the text and the range controls started under it. Since routes clear their sides by themselves (LLP 1123 D3), the gallery's route opts out of them and pads its own.

The same opt-out stopped the header collapsing (bisected to 53d567920c). The host recognizes the range bar as a control bar only while the scroller below it spans the room the route's cover leaves, and it took the cover's sides off that room though the route opted out of them: a 951-point scroller was measured against 547 points beside the sidebar, the range bar was no bar, and UIKit was handed no scroller to collapse with. The host now leaves an opted-out edge out of that room.

The module's custom bar appearance and its material over the bar made iOS 27.0 leave the large title undrawn; 27.1 draws it.

### Lean in

`var(--exact-safe-area-inset-left)` and `-right` are the route's own inset, the sidebar included, and on the web they are `env()`. The scroller pads its sides by them, `box-sizing="border-box"` keeping it the route's width, so it still runs under the glass sidebar as a UIKit column does, and the page centres beside it; the range bar pads by them plus 16 points.

From iOS 26 the bar draws no material of its own, so the gallery leaves the bar and its scroll edge effect to UIKit there, and the title is UIKit's large title. Before iOS 26 the shared material stays.

### Needed

- the host keeping a route's control bars on the edges the route opts out of: fixed here (`ControlBarsIOS.swift`)

### Checkpoints

- `GalleryUITests` on the open Duo: the header collapses and the range bar follows it (failed before), the page's text and range controls start beside the sidebar (`testPageContentStartsBesideTheSidebar`, 336 against the sidebar's 320; 131.7 and 16 before), the page switches keep content below the range bar, the range controls stay interactive, and the sidebar and video checks pass. Folded, the header and video checks pass. `GalleryRangeBarTests` passes.
- iPhone 18 Pro, iOS 27.0: the large title is drawn on every section (it was blank) and collapses as the content scrolls.
