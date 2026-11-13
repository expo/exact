surfaces: web, ios, macos, android
agent: Claude Code, Claude Opus 5.5
task: let go of a stream that opens after the camera was left; flip `facing` only once the track turned

## Rough

`getUserMedia`'s `then` (`show`) ran whatever the screen did since, so a Cancel while the camera
opened stored the late stream and the camera stayed on behind the editor. Unlike `shot = none`,
forgetting the send would not help: the platform has opened the camera either way, so `show` has
to stop the track it receives when the camera route is gone.

`flip` wrote `facing` beside its `applyConstraints`, so a refused flip (no track yet, no camera
that way) left `facing` naming the camera that was not open. It now has its own mutation whose
`then` reads the track's `settings.facingMode`.

## Lean in

`top(nav).name` says which screen a reply is for; a `let` in an action names the arrived track once.

## Needed

- camera capture: provided

## Checkpoints

- `contract/cli/tests/it/photo_cancel.rs`: a stream arriving after Cancel is stopped and not held;
  `facing` follows only the flip the track took
- `contract verify apps/photo-editor/app.contract --runner-only`: 5 properties held
