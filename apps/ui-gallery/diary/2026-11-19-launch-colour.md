surfaces: iOS 27.1 (iPhone Duo simulator, open)
agent: Claude (Opus 5.5)
task: In dark mode the app launched on a white screen, then showed its black first frame.

### Rough

The manifest named `"background_color": "#ffffff"`, and a named colour is the launch screen in both schemes unless `color_scheme_dark.background_color` names a dark one (LLP 1163 D2), so a dark launch was white. The gallery's root already paints `light-dark(#ffffff, #000000)`, which says everything the launch needs; the manifest's one colour contradicted it. Nothing pointed at the manifest: the white screen is gone before a screenshot can be taken, and only a screen recording showed it.

### Lean in

Saying less fixed it. Without `background_color`, the launch screen is the platform's page, white or black with the system's scheme, which is what the gallery's root paints in each, as an empty `UILaunchScreen` is in a hand-built app.

### Needed

- launch screen in the system's scheme: provided, rough (a lone named colour silently held in dark mode; expo/exact#1022 has `verify` say so)

### Checkpoints

- iOS build: smooth
- seeing the launch: rough (a screen recording, `simctl io recordVideo`, read frame by frame)
