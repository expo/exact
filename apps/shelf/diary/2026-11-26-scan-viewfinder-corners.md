surfaces: ios, macos, web
agent: Claude Code, Claude Opus 5.5
task: James: "Give the scan barcode camera preview nice continuous corners"

### Rough
The viewfinder spanned the Scan screen's width, so rounding it rounded corners against the sheet's edges. It needed a side inset from the platform, and none of the margin readings gave the sheet's own:

- On the closed iPhone Duo, UIKit gives the sheet's view layout margins of 16 points leading and the side strip's 76 trailing (a hand-built UIKit sheet on the same simulator).
- `var(--exact-layout-margins-inset-*)`, the box's own margins, read 0 inside the sheet's route, with or without `-exact-preserves-superview-layout-margins`. The kernel resolves a box's margins to its safe area when its owner reports none, so the sheet's route seems to get no margins from the host.
- `env(-exact-layout-margins-inset-*)` reads the window's root view, 20 points on this display, not the sheet's 16.
- A browser has no layout margins, so each `env()` margin is the safe area, 0 in a page (LLP 1123 D10).

### Lean in
`border-radius="radius.lg"` with `corner-shape="-exact-continuous"` and `overflow="hidden"` gives the viewfinder each platform's large card corner on Apple's continuous curve: 26 points on iOS, the size of the Add Book form's cards; 10 on the Mac; 12 on the web. The camera's native preview layer clips to it on iOS and the Mac.

Each side's margin is `max(space.md, env(-exact-layout-margins-inset-*))`: the platform's layout margin, or the base scale's `space.md` where the platform has none. On the closed Duo that is 20 points leading and 16 before the side strip, 20 a side unfolded and on the Mac, and 16 on the web.
