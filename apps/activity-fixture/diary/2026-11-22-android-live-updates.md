surfaces: android, ios
agent: Claude
task: show the departure activity on Android as a Live Update and drive its sends

### Rough

The first Android drive listed no activity after a start that had posted. The
reader read `getActiveNotifications()` straight after `notify()`, and the
system takes a post a few main-thread turns later, so the list matched the
previous one and nothing committed. The reader now lists once the shade shows
the post (exact2: `Activities.kt`).

The agent's Android drives force-stop the app before and after, and Android
cancels a force-stopped app's notifications, so a cold start that relists them
was checked by hand: launch, tap, Home, `am kill`, launch.

The fixture had no Android crate, so `apps/activity-fixture/android` was added.
The APK's minSdk is 31, so the attached Android 10 phone could not take it.

### Lean in

`dumpsys notification --noredact` shows each post's flags and extras, including
`PROMOTED_ONGOING`, `android.requestPromotedOngoing` and the chip's text. The
bake's refusal of a misspelled field names the state's fields.

### Needed

- live_activities with an android mapping: provided
- notifications permission: granted with `pm grant` for the drive; the app
  would ask with `requestNotificationPermission()`
- the status bar chip: not shown by this emulator image's SystemUI

### Checkpoints

- editing app.contract: rough; `for … key a.id` needs `key=`, and a count
  read after its assignment is the old value
- Android build: smooth
- driving on the API 36.1 emulator: rough; see Rough, then every case passed
