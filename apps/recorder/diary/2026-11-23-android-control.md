surfaces: android
agent: Claude Code (Opus 5.5)
task: offer Recorder's `record` control on Android as a Quick Settings tile, and record a take from it

### Rough

The control's tool starts the `recording` Live Activity before the
microphone, and the activity had no Android mapping, so on Android the take
never began. The activity now maps to an ongoing notification titled by a new
`label` field and counting up from `startedAt`, and the app grants
`device.notifications`.

With notifications off, the activity's start fails, and a failed send runs no
`then`: the app learned of it only through a task on `failed(shown)`, which
now shows a row asking for notifications.

Every take on Android was unplayable: each MP4 sample was the file's first
bytes, since `MediaMuxer` reads a heap buffer from its array's start and not
from a slice's offset (exact2: `packages/recording/android/Parts.kt`).

The emulator has no audio input, so every take is silence.

### Lean in

The bake placed the control as `foreground` with no change to the tool: the
tile opens the app and the press records once it is in front, and it never
prompts, so the app's own Record button asks for the microphone.

### Needed

- a Quick Settings tile for a recording control: built in exact2 (LLP
  1126.004 §6e)
- asking for notifications before a tile can record: built in the app

### Checkpoints

- app.json and activities.contract: rough; see Rough
- Android build: smooth
- driving the tile on the API 36.1 emulator: rough at first (the unplayable
  takes), then cold, warm, after Back and locked all behaved
