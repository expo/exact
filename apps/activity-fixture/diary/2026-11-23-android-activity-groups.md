surfaces: android
agent: Claude
task: keep several Android activities from being bundled into one notification

### Rough

The agent's Android drive force-stops the app when it ends, which cancels its
notifications, so several activities at once were driven by hand: taps found
through `uiautomator dump`, posts read with `dumpsys notification --noredact`.

Gradle restored the canvas unit tests' results from its build cache after the
source under test changed back, so a run against the old code first read as a
pass; `--rerun` ran them.

Turning Live Updates off for the app without Settings' UI: `appops set --uid
<package> POST_PROMOTED_NOTIFICATIONS ignore`, then new posts.

### Lean in

`dumpsys notification --noredact` shows each post's group key, so a bundle the
system made (`g:Aggregate_AlertingSection`) is plain from the app's own groups.

### Needed

- several live activities at once on Android: provided

### Checkpoints

- Android build: smooth
- driving on the API 36.1 emulator: rough; see Rough
