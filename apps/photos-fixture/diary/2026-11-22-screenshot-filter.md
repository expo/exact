surfaces: android (emulator, API 36), macos (agent), ios-simulator (iOS 26.5)
agent: Claude Code, Claude Opus 5.5
task: `photoAssets(subtypes=["screenshot"])` (expo/exact#714, LLP 1133.019.000 piece 2)

### Rough

- Adding one button under Show more pushed Transform below the emulator's
  screen, and drive 4's taps then missed it ("covered or not hit at its
  projected center"). The toggle went into a row beside Show more.
- `adb shell content delete --uri <image uri>` refuses another app's row
  ("UID 0 has no associated package"), so a drive that takes a system
  screenshot removes its file and rescans the volume instead.
- `simctl addmedia` never marks a photo a screenshot, and the simulator's own
  screenshot needs Simulator's window, so iOS shows only that imported photos
  are left out.

### Lean in

The toggle is one state value and a ternary in the resource's arguments:
`photoAssets(limit=limit, cursor=cursor, subtypes=(screenshots ? ["screenshot"] : []))`.

### Needed

none

### Checkpoints

- `packages/photos/test/android.mjs --emulator`, drive 7: 10 files in
  `Pictures/Screenshots/`, 3 in `DCIM/screenshots/` and a system screenshot
  (`KEYCODE_SYSRQ`) listed in 2 pages of 12, each once; the decoys in
  `Pictures/MyScreenshots/`, `Pictures/Screenshots/old/` and `Pictures/` left
  out.
- Under the agent, Screenshots lists agent-2 alone, on Android and macOS.
