surfaces: iOS 27.1 (iPhone Duo simulator, open and folded), iOS 27.0 (iPhone 18 Pro simulator), macOS, the web (Chromium, and Safari on iOS 27.1 on the Duo, folded and open)
agent: Claude (Opus 5.5)
task: Launch on the open Duo without first drawing tabs, and give the web a list beside its detail, with the list on a screen of its own on a narrow window.

### Rough

On the open Duo the gallery's first frame was Exact's tab container, its bar in the side strip, and about 250 ms later the split the app's module built; the header and the page moved as one replaced the other. The split came from the `tabContainer` hatch, and a hatch runs only after first pixel (LLP 1075.003.000.001 D5), so no change in the app could make the first frame the split while the routes said tabs. The routes were the wrong shape: the gallery is a list of sections and the section chosen, which its module drew as a split with a state the tab router cannot hold (the list with nothing chosen).

On the web the tabs had the same shape problem: a narrow window showed a tab bar, not the list. A tab that reads its symbol from data is no native tab there, so the sidebar's rows lost their styling; and `symbol:sf/…` names draw blank on the web.

The TypeScript module decided whether it ran in a browser by its user agent, but the web page starts from the answers of the build's render, whose `navigator` has none, so the page never knew: the CSS page's rec2100 rows showed empty swatches without their note, and Safari's caveat on the HDR page never showed. `exactPage().platform` names the host from the first frame, so the contract passes it to each source that differs on the web; it names the web, not the browser.

On iOS the split's empty detail is UIKit's "No Selection", not the app's `navigationSplit="placeholder"` box, until LLP 1127 builds the author's placeholder; "Choose a section" shows on the web and the Mac.

In a UI test the list's collection view is never hittable, only its cells, so a test that asked the list whether a section covered it always went looking for Back. Back's presence answers it.

A row chosen while a section shows replaces the section, and `replace` keeps the route, so the new section kept the old one's scroller, scrolled, and UIKit's title stayed collapsed; a new scroller alone did not bring the large title back. Writing the section's route anew under its key (`for s in [e.params.section] key=s`) makes it the new screen a sidebar's detail is.

The Mac splits at every width and paints the grouped list as written, an inset-grouped form, where the tabs gave it AppKit's source-list sidebar. The manifest's 420-point window, sized for the tabs, grew only to the split's minimum: AppKit squeezed the list to its narrowest, 200 points, which wraps the rows' titles, beside a 320-point section. The window now opens at 960, where the list has its 320.

### Lean in

The gallery says what it is: `sections "/"` and `demo "/demo/:section"`, a list whose rows open a section. Each host draws it as its own: the web beside the detail from 900 points and the list alone below, and on iOS UIKit's split view (LLP 1127), the sidebar list beside the section on the open Duo and one stack when compact. Nothing the app writes is a hatch, so the first frame is the app's: a recording of each launch, every distinct frame kept, goes from the launch ground straight to the split or the list. A row chosen while a section shows replaces it, as a sidebar's row does; the browser's Back still goes back. Roles (`symbol:article`, `symbol:photo`) draw on every host; the Apple hosts keep the SF Symbols. A browser across a fold, the Duo's partly folded, puts the list in one viewport segment and the section in the other, as UIKit does there (LLP 1078 D6); before, the web split only when each segment was 900 px wide, so the page pushed and ran across the fold.

### Needed

- split view: provided, built in exact2 for this on iOS (LLP 1127) and at a browser's fold (LLP 1078 D6, amended 2026-11-20)
- split view placeholder: missing on iOS, where UIKit's "No Selection" shows (LLP 1127 builds the author's in its stage 3)
- browser engine: missing; HEIC and TIFF stay listed under their Safari-only labels
- the Mac's sidebar list: missing; beside its detail the Mac paints the grouped list as written, not as AppKit's source list (LLP 1127 D13)

### Checkpoints

- web build and tests: smooth (`bun scripts/agent.mjs web --app ui-gallery --test apps/ui-gallery/text.test.contract`)
- UI tests on the Duo and the iPhone: smooth once the helper read Back (`EXACT_UI_TEST_APP=com.exact.uigallery EXACT_UI_TEST_SOURCE=apps/ui-gallery/tests/GalleryUITests.swift bun packages/quick-actions/test/apple-ui.mjs <simulator> <output-directory>`)
- seeing the launch: rough (`simctl io --display=3 recordVideo` for the open Duo's inner display, then ffmpeg's `mpdecimate` to keep each distinct frame)
