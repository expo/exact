surfaces: android
agent: Claude Code (Opus 5.5)
task: the rest of LLP 1042.001 on Android: accessibility, the rendition test, timeupdate, commands, `audio`, the poster, full screen

## Rough

- Every development build of an app with a locked Maven graph (Shelf, Weatherlight) failed
  Gradle's dependency verification once `:video` landed: such an app resolves every APK
  against its own lock, which held no Media3. The lock is one graph for development and
  production, so it cannot both hold Media3 and leave it out; such an app now links the
  player only for a plan with a video.
- TalkBack on the emulator ignores `adb shell input` taps and swipes; it explores only
  real touches. `adb emu event mouse x y 0 1` (and `… 0 0` to lift) is a real touch, and
  a right swipe of such touches moves to the next item. Its speech is in `logcat` as
  "Speaking fragment" with the verbose log level on.
- The driver's `tap` on an `audio` with `controls` reached the controller ("delivery
  platform") but pressed nothing; a real touch (`adb shell input tap`) on the same point
  played it. A `video`'s controller took the driver's taps.
- The tiled frame after the HLS stream switched to 640×360 is the emulator's decoder: the
  goldfish H.264 decoder keeps the 960×540 output format after the switch (Media3's
  `VideoSize` stays 960×540), so the smaller frames are laid out as tiles. Our surface
  sizing follows `VideoSize` and is not the cause. A phone should still be checked.
- The rendition test that was dropped selected every rung because Media3 1.11 selects by
  the parameters the playback thread has activated (`onParametersActivated`), not by
  `setParameters`; a test has to activate them as ExoPlayer does.
- The first time an app hides the system bars, Android shows its "Viewing full screen"
  notice, which holds focus: Back does nothing until "Got it" is pressed, and a fresh
  install shows it again. The driver has no system Back, so Back was `adb shell input
  keyevent BACK` sent while `clock until state media.0.state.fullscreen == false` waited.

## Lean in

- `tree --ax` lists the controller's own buttons under the video's node, which made the
  accessibility change quick to check before TalkBack.
- Temporary edits to `app.contract` (a button calling `fastSeek` or `load`, an `audio`
  row, a poster) were enough to check each command on the emulator.

## Needed

- In its 300×54 box Media3's controller drops to its minimal mode: play and pause over
  the time bar's line, with no time readout and no settings.
- An `audio` without `controls` has no box, so the painter never names it and nothing
  plays it (D12's second half).

## Checkpoints

- `:video`'s unit tests: 31 pass, the rendition cap among them.
- Emulator (API 37): TalkBack says "A color study" for the video and reaches Pause, which
  pauses; `timeupdate` moves the app's position, and without the handler `state.media`
  still runs on; `fastSeek` lands at 5 s and `load` starts again at 0; an `audio` plays
  from its controller; the poster shows while paused and goes at the first frame after
  play; Full screen (the app's button, which calls `requestFullscreen`, and the
  controller's own) fills the screen with the bars hidden, the app hears
  `fullscreenchange` both ways, and Back or the controller's button returns the player
  inline without a restart.
