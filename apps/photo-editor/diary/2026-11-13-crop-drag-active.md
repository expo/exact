surfaces: ios, macos
agent: Claude Code, Claude Opus 5.5
task: count a crop drag as an active gesture in the photo canvas

## Rough

The canvas settles (reports the resting edit and eases to it) once no gesture is active. A crop
drag never counted as one: on the Mac its pan returned before `track`, and on iOS `dragHandle`
had its own switch. A pinch, a rotation or a wheel zoom that ended under a held crop handle
therefore settled, and the ease wrote `edit` while the drag did.

## Lean in

The canvas's own `track` and `settle` already said what a gesture is; the crop drag now goes
through them, and its end settles as the others' do.

## Needed

- nothing new

## Checkpoints

- `bun host/apple/build.mjs photo-editor --no-install` and `--ios --no-install` build the module;
  the gestures were not driven (a held crop handle under a pinch is not an agent operation)
