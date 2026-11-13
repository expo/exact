surfaces: android, linux
agent: Codex, GPT-6 Astra
task: add system-browser sign-in

## Rough

- Sign-in returned 501 even with an Android browser installed. The shared
  presenter had no Android browser arm. The reader now opens the browser and
  receives callbacks; a documented host-support table would have made the gap
  visible before building the fixture.
- Lifecycle and browser-selection checks took several review rounds, about
  half an hour for the regressions and fixes in this pass. A pause
  without a stop cancelled sign-in, an auth request delivered after activity
  destruction disappeared, and separate HTTP/HTTPS filters hid a valid browser.
  Failing JVM tests reproduced all three. The reader now records stops, cancels
  launches without an activity and intersects HTTP/HTTPS browser packages.
  The presenter preserves only waits the reader acknowledged. Lifecycle tests
  through the owner’s actual request queue would have caught the lost launch.
- Callback and reload checks also needed corrections: endpoint matching must
  leave ordinary links alone, responses matching a live state but failing other
  checks settle 502, and runner replacement must retire old waits before tickets
  are reused.
  The ten-minute bound must count deep sleep, which Handler delays exclude.
- The web JS auth hold survives mutation cancellation. The fixture omits its
  Cancel sign-in button; the separate defect remains in QUEUE.md. A working
  cancellation path on that host is still needed.
- Android’s emulator does not boot on this Mac. JVM tests and a headless Linux
  drive cover the reader logic and shared presenter; the amended APK still
  needs a device run. Earlier build friction included a missing icon-cache
  parent, now fixed on main. Exact timings for the earlier work were not kept.

- A stale browser tab could fail a newer sign-in: routing by endpoint alone
  selected the wrong request. Failing reader and runner regressions now cover
  selection by state, keeping a retry alive and ignoring that callback's return.
- Ephemeral sign-in opened the shared browser profile. This reader has no private
  tab API; the package's AndroidX Browser pin is 1.8.0. The presenter now refuses
  ephemeral requests with 501 before queuing a launch. Its regression checks both
  that refusal and an ordinary launch.

## Lean in

- Grants supply both the callback scheme filter and complete endpoints for
  interception, leaving other links available for ordinary navigation.
- The reader’s lifecycle runs in JVM tests with only the native boundary
  substituted; the owner’s real request queue can be exercised there too.
- The system default browser wins, with Custom Tabs when it supports them.
  Back from a full-screen browser cancels promptly; a transient pause keeps
  sign-in alive. The same-scheme Inbox route makes navigation easy to inspect.

## Needed

- sign-in: by hand (added the Android browser arm and callback delivery).
- deep links: provided (grants generate callback filters; ordinary links route).
- cancellation: provided on Android after a stopped activity returns; missing
  for the fixture’s web mutation-cancellation path (see Rough).

## Checkpoints

- first Android build: rough (icon-cache parent; see Rough).
- editing app.contract: smooth (added an Inbox route for ordinary deep links).
- driving the app: rough (Android emulator unavailable locally; see Rough).
  The earlier outside-agent run used 39ffe2c4 on Android 16 / API 36 arm64,
  Chrome 134.0.6998.135 and Firefox 145.0. Both completed custom-scheme sign-in,
  Back and retry; Chrome also exercised activity destruction, process death,
  ordinary links and no-browser 501. That APK predates the current corrections.
  The no-default chooser, a browser without Custom Tabs, actual multi-window,
  account-chooser and permission-dialog behavior, and physical phones remain
  unqualified on a device.

- driving the shared presenter on Linux: smooth (built Caltrain, opened station
  search, entered Palo and saw Palo Alto on the rebased tree).
