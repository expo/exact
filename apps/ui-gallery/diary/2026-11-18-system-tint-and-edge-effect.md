surfaces: iOS 27.0 (iPhone 18 Pro simulator), iOS 27.1 (iPhone Duo simulator, open and folded)
agent: Claude (Opus 5.5)
task: The tint was not the system's, and the header and the controls under it did not blur the content beneath them as a UIKit screen does.

### Rough

The scroller's `overflow-x="hidden"` clipped it at its box, though the host draws its scroll view up through the range bar and the navigation bar to the screen's top (LLP 1131 D2). Content and UIKit's edge effect ended in a hard line at the box's top, under the status bar. Screenshots showed the line but not its cause; a dump of UIKit's view hierarchy through lldb did: the scroller's node clipped, and its scroll view reached past it. The host now leaves a banded scroller's clipping to its scroll view (expo/exact#963). What would have helped: `agent.mjs check` naming a band that the scroller's own clip cuts short.

The Accent section's last button, `accent-color="auto"` written inside the purple box, was labelled "auto: the app's" but draws the box's purple on iOS, where the host maps a written `auto` to UIKit's unset tint, which inherits (`applyAccentTint`). A browser's `auto` is its own accent (CSS), and a native Mac button takes the system accent there. What a written `auto` should be beneath an accented box is not settled, so the label and the legend no longer claim one.

### Lean in

Saying less fixed the rest. The app's red `AccentColor`, its request that iOS 27 minimize the bar on scroll, and its soft scroll edge effect each overrode UIKit. Without them the tint is the system's; the bar stays on an iPhone (iOS 27.0), while the Duo (iOS 27.1) hides it as the content scrolls up, as UIKit does there unasked; and with the range bar registered as the scroll view's edge element, UIKit's automatic style blurs the content from the screen's top to the range bar's foot, as a hand-built screen with a `UIScrollEdgeElementContainerInteraction` does.

### Needed

- system tint: provided
- scroll edge effect under a control bar: provided, rough (the host clipped a banded scroller with `overflow-x: hidden`; fixed in expo/exact#963)

### Checkpoints

- iOS build: smooth
- driving the app: smooth (`GalleryUITests`, and a probe that photographs the header at measured drags)
