# Test server isolation

## Rough

The board tests cleared the stand-in before counting strokes because earlier tests left writes.

## Lean in

The test runner now restarts the server before each test. The tests use that empty board directly.

## Needed

No reset route or test-only cleanup is needed for isolation.

## Checkpoints

`test web`: four passed, zero failed. `verify web`: four passed, zero failed, zero problems; relaunch preserved the chosen hue.
