surfaces: web, ios, macos
agent: Codex (GPT-6)
task: keep text and marks readable on authored accent fills

### Rough

- Apple's white `AccentColorText` measured 2.86:1 on the dark orange fill. The
  browser's automatic ink hid this from the existing browser tests.
- The iOS check also reports existing small touch targets, including the status
  clock and Save button. Their geometry is outside this colour correction.

### Lean in

- One scheme-aware ink token covers the home badge, Add Key, Send arrow and enabled
  Save label. The home screen's forced dark scheme resolves both its fill and ink
  together, even when the app's screens use light appearance.
- The browser tests can exercise disabled and enabled Save without storing a key
  or contacting the assistant service.

### Needed

none

### Checkpoints

- Editing app.contract and parts.contract: smooth; the fill colours stay intact.
- Web tests: smooth; system, light and dark appearance pass the contrast checks,
  including a draft's Send arrow and both Save states.
- iOS drive: smooth; the badge and enabled labels use black on dark orange and
  white on light orange. Disabled Save keeps its separate foreground and fill.
- macOS drive: smooth; both appearances show the corrected labels and Send arrow,
  and Save changes between disabled and enabled as its field empties and fills.
  Captured orange fills match the authored colours: white ink in light measures
  5.67:1 and black ink in dark 7.35:1.
