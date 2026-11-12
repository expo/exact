surfaces: ios
agent: Codex
task: cover keyboard submission from a header search field

## Rough

Native search editing needs a hosted app's responder lifecycle. UIKit owns visibility during an active search, so the XCTest checks Exact's inactive-query visibility rule before Agent starts editing.

## Lean in

Agent types into UIKit's displayed search field and delivers Enter through its search delegate. Contract state records the submitted query and counts submissions.

## Needed

Keyboard submission: provided.

## Checkpoints

The extended XCTest compiles and passes in the hosted app on the lane's iOS 27.0 simulator: typing does not submit, Enter submits the entered query once, and later edits preserve that observation.
