surfaces: android
agent: Claude Code (Claude Opus 5.5)
task: the Player screen's video waits alive on another tab on Android (`portal`, LLP 1013.002 D5, #698)

### Rough

- On another tab the reader released the hidden player, so coming back loaded the video again
  (`loads` 2). Fixed in exact2: the painter names each video in a hidden mover to the reader as
  held, and the reader keeps its element, player and size, out of view.
- `state position` read on the agent's clock looks paused whatever the player does: the video
  plays on the wall clock, so a real wait (`perf frames live <ms>`) between reads is what shows
  it advancing or holding.

### Lean in

- The off-screen rule the reader already had for a muted autoplay video pauses the held player
  and resumes it, so nothing in the app changed.

### Needed

- Nothing for this.

### Checkpoints

- Android (emulator, API 36.1, the reader APK): Count, Open, Back 60 ms in, Open, Count gives
  Count 2, `loads` 1, `position` 1.00 → 1.76 s. On the Grid tab `position` holds at 1.76 s for
  8 s; back on Player it reads 3.40 s 1.5 s later, `loads` still 1. Remove, then New: Video#134,
  Count 0.
- On Grid, `uiautomator dump` has no Count and no focusable node of the app; the held player's
  TextureView is the same unlabelled, unfocusable view as when shown.
- Recorded with `--live` and `adb shell screenrecord`: on Open the detail grows from the card
  with the video drawn inside it; on Back the route's kept drawing shrinks into the card, then
  the card's video plays on.
