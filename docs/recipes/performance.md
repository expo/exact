# Recipe: a slow screen, a slow transition or a hitch

Find whose the time is before changing anything. A trace or probe output of the release build already says: read
it. Measure (again) only without one, or when it came from a development build or a run that read the
accessibility tree. Measure the build that ships, with the feel probe in the Exact checkout your `exact.mjs` names:

```sh
bun exact.mjs ios --release
bun <exact checkout>/host/apple/feel/probe.mjs --udid <udid> --app <bundle id> --steps 'tap "Trails";measure tap "Bright Angel"'
```

`measure <step>` prints the step's main-thread time, its late frames, and whose their time was; for scrolling,
`bun exact.mjs verify ios` runs the same probe. Read the labels:

- `app JS`, and `Exact runtime (main waits on exact.owner)`: your Contract and data work. Yours.
- `app code`: your native module. Yours.
- `Exact host` alone: Exact's own. Report it with the probe's `feel.json`; do not work around it in the app.
- `Exact host (in UIKit …)`, `(in maps …)`, or a framework alone: the platform's work, which the same screen
  built with SwiftUI pays too: UIKit's push, split and sheet transitions, the first navigation bar and search field
  at launch, the keyboard's first appearance, MapKit making a map. Stop there.

A Time Profiler trace reads the same way. Every touch enters through `ExactWindow.sendEvent`: read what is under
it (a push, the keyboard coming up) before blaming it. An Exact function whose time is UIKit's transition frames
(`_UINavigationParallaxTransition`, `_didMoveFromWindow`) is UIKit's push. UIKit's own bars and controls run
SwiftUI inside: SwiftUI frames in an app that writes none are the platform's.

Costs your Contract chooses:

- A `scroll=` action lays the screen out and applies it on every frame it changes state. A stretchy header that
  writes only the pull is right, and an action that writes the value state already holds costs nothing; to show a
  button past 300 points, store a boolean, not the offset.
- `-exact-safe-area-extend` uses the platform to extend a picture into the safe area. Reserve it for a hero picture.
- A native view in list rows: `ExactNativeFactory(…, reuse: true)` and `prepareForReuse()`. Without `reuse`, which is
  for list rows, a native view is made each time its node is: when it first shows, as SwiftUI makes its map, so do
  not make one at launch or keep one hidden; and again when the key of a `for` around it changes while it stays
  shown, a remake your Contract causes: keep that key stable and pass what changes as props.

Not evidence:

- A development build: its agent and development menu are not in the app.
- Absolute numbers from a run that read the accessibility tree (`axe describe-ui`, a tap by label): the
  accessibility runtime it turns on adds work people without assistive technology never pay. Compare runs.
- A first launch against a later one, or a Mac benchmark for an iOS change.

Write only syntax Exact accepts: `contract vocab <name>` says; `animation-timeline: scroll()` is refused.
