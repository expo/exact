surfaces: ios, macos
agent: Claude Code, Claude Opus 5.5
task: finish gapless `loop` on Apple (#561) and make the loop test wait on its condition

## Rough

The first loop test slept 32 s of real time and expected exactly three loops, so a slow start
failed it. `clock until state loops == 2 within 60000` waits for the count instead. The counter
also took any jump back of more than 5 s for a loop; it now counts a time near the start after
one near the end.

## Lean in

`clock until … within` was the whole wait.

## Needed

- video playback: provided

## Checkpoints

- driving the app: `loop.test.contract` on macOS
