surfaces: windows
agent: Codex / GPT-6 Astra
task: keep a sample selection consistent after cancellation

### Rough

The native list changed its selection before the unsaved-changes prompt.
Cancel kept the document but left the new row highlighted. The adapter now
retains the accepted row and restores it when replacement is cancelled.

### Lean in

The existing native smoke can exercise the actual list and confirmation
instead of substituting a dialog answer.

### Needed

- confirmation: provided by the app's Win32 adapter.

### Checkpoints

- Windows build: smooth, including the checkout path's space.
- Driving the app: rough; an external WM_SETTEXT did not deliver the edit
  notification, so the drive used EM_REPLACESEL.
- Native smoke: smooth; cancellation from Welcome and Scratchpad preserves
  the edited text, title, dirty state and previously accepted row.
