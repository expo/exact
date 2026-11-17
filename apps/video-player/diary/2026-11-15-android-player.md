surfaces: android
agent: Claude Code (Opus 5.5)
task: play the video on Android with Media3's player (LLP 1042.001)

## Rough

- Android had no player at all: the reader's `VideoPlayer` seam had no implementation in the
  repository, so the video was an empty box and a log line. The app had no Android crate
  either; `android/` is modelled on Caltrain's.
- A record restored at a start was missed at first: the stop removed each player before
  taking its snapshot, so the reader made again found no record and the video played on
  return. The stop's `pause` was also dropped, because the loan closed before the posted
  report ran.
- `uiautomator dump` waits for an idle window; a playing video never is. Pause it first,
  and use `--windows`, or the dump is the launcher's.
- In a Robolectric test, `DefaultTrackSelector` with an explicit viewport selected every
  rung of the ladder, although the cap it computes (read by reflection) was right. On the
  emulator the cap holds, so the viewport is checked there and the unit test was dropped.

## Lean in

- `agent.mjs android --reader` drives the APK as the other hosts are driven; `state.media`
  came from the player as soon as the painted host kept its reports.
- The keyboard's resize ("the picture makes room while you type") is a ready test of the
  rendition cap: with the box shrunk the stream dropped from 960×540 to 640×360.

## Needed

- The `video`'s `aria-label` does not reach Android while `controls` is on: Media3's view
  reports itself as "Show player controls".
- On the emulator (SwiftShader), after the stream switched to 640×360 the frame drew tiled;
  a phone should be checked.

## Checkpoints

- `:video`'s unit tests: 25 pass (the media state machine, the picture's rect and viewport,
  the record's rules).
- Emulator (API 37): the bundled MP4 plays, pauses from the app and from the controller,
  loops; the HLS stream holds 960×540 in the full box and 640×360 with the keyboard up;
  the settings menu opens at the gear; home and back returns paused where it was.
