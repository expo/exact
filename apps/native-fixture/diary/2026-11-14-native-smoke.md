surfaces: macos, web
agent: Codex
task: requalify the native module smoke

### Rough

- The web drive tapped Close during the sheet's slide. Settling presentation before the tap keeps the viewport check useful. The sheet's content also needs to scroll within its short detent so it cannot cover Close.
- The home header's controls overflowed a 420-point Mac window. Wrapping the authored row keeps Compose visible with native control sizes.
- Mac clicks preserve focus, so the blur probe and dialog restoration check must explicitly focus their source. Dialog traversal needs Keyboard Navigation enabled to include its buttons.
- The finite hatch loop could finish before settlement was checked and overwrite the journal used by another assertion. Settlement now checks for either pending acts or all 4,001 presses, including the final change while armed; journal assertions run before it.
- Pixel checks sampled module boxes outside the viewport. Each capture now reveals its target and reads its pixels against that capture's geometry.
- The layout inspector reported a paragraph's own inline key view as stray. It now accounts for that view by identity and still reports an unregistered sibling.

### Lean in

- The web refusal named the remaining slide duration and the clock operation that completes it.

### Needed

- none

### Checkpoints

- macOS build: smooth
- macOS drive: rough; see Rough. All 150 native checks pass at 420 points, with no layout disagreements.
- web drive: rough; see Rough. All 100 native checks pass.
