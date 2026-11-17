surfaces: android, linux, web
agent: Claude Code (Opus 5.5)
task: keyboard and D-pad control of a `controls` video on Android (LLP 1042.001 D4); the poster on Linux

## Rough

- When exact2's focus reached the video, Media3's controller showed but its play button did not take
  the focus: `PlayerControlView.show()` asks for the focus before the fade that makes its buttons
  visible has started (the fade's start, a frame later, sets them visible). The player now holds the
  focus itself until then and moves it to the play button before that frame is drawn, and does the
  same whenever a key shows the controller again.
- When the controller hides, the focused button gives the focus to the window's first view. The player
  takes it back, so the next key shows the controller instead of reaching exact2.
- `dumpsys activity top` is the way to see which Android view holds the focus: the second flag group
  of a view's line has `F` when it is focused.
- One canvas unit test, `SharedElementsTest.retainedFlightsReadCurrentStaticAncestorOpacity`, failed in
  the whole suite and passes alone.

## Lean in

- `adb shell input keyevent TAB` (and `keycombination KEYCODE_SHIFT_LEFT KEYCODE_TAB`) drove the whole
  walk: exact2's buttons, the video's controller, then the note field, and back.
- `EXACT_PLAN` with `EXACT_ASSETS` ran a fixture with a poster on the Linux host (headless, through
  `agent.mjs linux --app svg-gallery --plan`), and the same plan on the web, without a Linux crate for
  this app.

## Checkpoints

- Emulator (API 37): Tab reaches the video after the source menu and the controller shows with its play
  button focused; Space, Enter and the D-pad's centre each toggle play and the app hears `play` and
  `pause`; left and right move between the seek buttons, down reaches the time bar, where right seeks;
  the controller's edges keep the focus; Tab goes through the controller's buttons and on to the note
  field, Shift+Tab back. After the controller hides, the first D-pad key shows it at the play button and
  the second pauses.
- Linux headless: a video's poster shows under `contain` and `cover`, placed as Chrome places it.
