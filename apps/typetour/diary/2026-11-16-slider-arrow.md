surfaces: web, ios, macos, linux
agent: Claude Code (Claude Fable 5.2)
task: test the text-size slider's arrow keys

### Rough

`type "text-size" key "ArrowRight"` did not mean the same thing on every host.
The web and Linux stepped the slider by its `step`, as a keyboard's arrow does.
On iOS the key reached the slider's `key` handlers and nothing else, so a slider
with no handler did not move and the test could not check what VoiceOver's swipe
does. The iOS agent now sends ArrowRight and ArrowLeft at a range through the
slider's accessibility increment and decrement (issue #719).

macOS still differs. The agent's arrow there is a keyboard's, and on this Mac,
with keyboard navigation off, the slider did not move: `scale` stayed 1. The
test is therefore `on web, ios, linux`.

### Lean in

`on web, ios, linux` after the test's name kept one test for the hosts that
agree, with no second file.

### Needed

- a slider's keyboard and screen-reader step: provided on the web, iOS and
  Linux; on macOS only where the Mac's keyboard navigation is on.

### Checkpoints

- The new test fails on an iOS 27 simulator with the agent's range route taken
  out (`scale is 1, expected 1.01`) and passes with it.
- Authored tests: 3 pass on the web, an iOS 27 simulator and the headless Linux
  host; on macOS the first two pass and the arrow test is not run.
