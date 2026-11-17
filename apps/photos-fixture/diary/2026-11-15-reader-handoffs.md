surfaces: android (emulator, API 37)
agent: Claude Code, Claude Opus 5.5
task: follow-ups to the selection settle (`2026-11-15-selection-settle.md`)

### Rough

- A picture the photos extension revoked (`revokePicture`) and a line it
  journaled still went to the exact thread as posted jobs, so a `clock
  settle` that waited for the tell's answer returned before either reached
  exact2. They now go in the reader's ordered queue with answers and topics
  (`OwnerReplies`). Fixed in exact2; nothing changed in the app.
- With no activity (after the activity was destroyed, the process still up)
  the reader dropped every module call and presence check, so a tell or a
  source's read was never answered and each later `clock settle` waited out
  its bound. The extensions outlive the activity, so the reader now hands
  them those calls; one that needs an activity says so in its answer.

### Lean in

Drive 3 is unchanged: `prefer extension`, then one `clock settle`.

### Needed

none

### Checkpoints

- `packages/photos/test/android.mjs --emulator`: all passed on an API 37
  Play image (drive 4's file checks skip there).
- Drive 3 with a temporary journal line where each revocation is applied:
  all three revocations were applied in the agent's turn inside the `clock
  settle`, before the re-asked `photoAssets` landed.
- The detached reader was not driven on a device; `OwnerRepliesTest` covers
  it.
