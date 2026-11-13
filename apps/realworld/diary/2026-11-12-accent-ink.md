surfaces: web, ios, macos
agent: Codex (GPT-6)
task: keep text readable on authored accent fills

### Rough

- The browser chose black for the pale dark accent, while Apple's `AccentColorText`
  stayed white. The selected favourite measured 1.97:1 on iOS.
- Realworld declares only a web host. Native checks used an ignored copy with a
  generated Apple shell and the API redirected to a local fixture. The first
  generated build reported its generated inputs stale; a warm rebuild settled them.
- Sharing the fixture's Cargo target with the checkout caused incompatible Markdown
  artifacts. A separate target for the fixture and rebuilding the checkout's
  Markdown editor restored the Apple build.

### Lean in

- A named `light-dark()` ink pairs with the existing accent without changing its
  use for links or the platform's colour roles.
- The iOS check reports the displayed foreground and fill, including a selected
  favourite and Post Comment after scrolling them into view.

### Needed

none

### Checkpoints

- Editing app.contract: smooth; all four authored fills use the same ink token.
- Web tests: smooth; both schemes cover sign-in, selected and unselected favourites,
  article actions and Post Comment with local API responses.
- iOS drive: smooth after the fixture setup; white on the light green measures
  4.92:1 and black on the dark green 10.65:1.
- macOS drive: smooth; both favourite states and schemes remain readable. Captured
  fill pixels match the authored greens; the dark article and comment do too.
