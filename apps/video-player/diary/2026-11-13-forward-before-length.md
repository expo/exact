surfaces: web, ios, macos
agent: Claude Code, Claude Opus 5.5
task: keep Forward from seeking to the start before the length is known

## Rough

A media length that is not known yet reads 0 here, as an empty clip's does, so a clamp to it
seeked to the start. No drive orders `timeupdate` before `durationchange` on purpose, so the test
is in the runner (`contract/cli/tests/it/app_action_snapshots.rs`), calling the app's actions
directly.

## Lean in

An `if` in the action was the whole fix.

## Needed

- video playback: provided

## Checkpoints

- driving the app: the runner test only
