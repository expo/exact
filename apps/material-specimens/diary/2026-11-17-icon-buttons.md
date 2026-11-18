surfaces: android
agent: Claude Code (Claude Opus 5.5)
task: Draw a symbol-only button as Material's icon button, and add the oracle's specimens for it.

### Rough

The oracle needs JDK 21 (Robolectric's sandbox for Android SDK 36 refuses JDK 17 with "Android SDK 36 requires Java 21"). With only JDK 17 on the Mac, all 45 reference tests failed before the cause showed in the test report. `oracle.mjs`'s header names JDK 21, but nothing checks it before Gradle runs. About 10 minutes.

The tokens alone gave the wrong colour for the standard and outlined icon buttons. `StandardIconButtonTokens.Color` is `onSurfaceVariant`, but Compose 1.4's default `iconButtonColors()` and `outlinedIconButtonBorder()` use `LocalContentColor`, which is `onSurface` on a surface. Only the `*VibrantColors` variants read the token. The oracle's probe caught it (ΔE 14 on the standard button's ink, 70 on the outline), as it caught the disabled field's container before. The outline then still differed at its edge: `Modifier.border` rounds the stroke up to whole pixels (3 px for 1 dp at 2.4375×), which no token says.

### Lean in

The oracle ran end to end on a Mac once JDK 21 was installed. Its detection test passed 44 of 44, and the tonal specimen matched Compose exactly (geometry, corners to 0.05 dp and the container's colour).

### Needed

none

### Checkpoints

- specimen door build: smooth.
- oracle run against Compose: rough (JDK 21, above); then smooth.
