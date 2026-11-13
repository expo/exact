surfaces: web, ios, macos, android
agent: Claude Code, Claude Opus 5.5
task: drop a photo still being taken when the camera is left

## Rough

A newest-wins mutation's reply lands and runs its `then` whatever the screen did since, so a
`takePhoto` answered after Cancel replaced the picture and its `nav = back(nav)` popped the route
opened after the camera. Nothing in `taken` said which screen the photo was for. The repair is the
one the print sheet already used for its payment: assigning the slot (`shot = none`) forgets the
request in flight (`docs/contract-grammar.md`, `analyze-send-assigned`). A link that leaves the
camera assigns nothing, so `taken` also checks `top(nav).name == "camera"`.

The runner test needed a pump that keeps a held reply's receiver across calls: one that drops it
lets the reply vanish, and a test of "the late photo is dropped" then passes against the bug.

## Lean in

`shot = none` cancels the reply with no extra state; `top(nav)` says which route a reply is for.

## Needed

- camera capture: provided

## Checkpoints

- the runner test (`contract/cli/tests/it/photo_cancel.rs`) and the camera drives on the web
