# Observe

Sends Expo Observe's performance metrics and custom events from an Exact app, under Observe's metric names. It works on iOS, macOS, Android, Linux and the web.

## Turning it on

Add these fields to `app.json`:

```json
"launch": ["observe"],
"moduleConfig": { "observe": { "projectId": "<EAS project id>" } }
```

These settings are optional:

- `endpoint` (default `https://o.expo.dev`);
- `sampleRate` (0–1, decided per install). An install outside the sample sends only feedback;
- `dispatchingEnabled`;
- `dispatchInDebug`. A development build sends nothing unless this is true.

On Apple, the launch part installs fatal-signal handlers, starts a network-path monitor and enables battery monitoring on iOS. Its deferred service loads after startup; a direct native query can load it on demand. Pending crash records live in the app's Application Support directory.

Android reports its OS version and device model. Its launch-module interface does not expose battery or connectivity facts, so those attributes are omitted.

## Metrics

| Metric | From | To |
|---|---|---|
| `expo.app_startup.cold_launch_time` / `warm_launch_time` | process start | launch end: didFinishLaunching on Apple, the display opening on Linux, the host starting on Android |
| `expo.app_startup.ttr` | launch end (page activation on the web) | the first frame on screen |
| `expo.app_startup.tti` | launch end | the first frame where nothing on screen is loading (below) |
| `expo.navigation.cold_ttr` / `warm_ttr` / `tti` | the input (or commit) that changed the route | the same marks for the new screen; `tti` only with `cold_ttr` (a screen shown before, as on a back or a tab seen before, has none) |
| `expo.updates.download_time` (Apple) | the first file requested | the update staged |

## Time to interactive

TTI is the first frame where nothing on screen is loading. Loading means any of these:

- a resource showing its placeholder (its `else`), until its answer lands;
- a stream, until its first message;
- an `after(ms)` task with `ms` ≤ 1000, until it runs;
- the host still drawing: images, canvas, SVG;
- an element marked `aria-busy="true"`.

These don't hold TTI:

- **A background refresh.** This is a request for a resource already showing an answer (kept from the last launch, baked, or settled).
- **A mutation the app sends.**

`aria-busy` is how an app marks anything else as still loading, for example `column aria-busy=(not ready)`. TTI waits until no element is busy. It can delay TTI, never advance it.

The TTI metric carries `exact.tti.trace`, which lists what was outstanding over time, in ms from process start.

## Custom events

```
observe("checkout.completed", Checkout(items=n, total=t), "info")
observeAttributes(Tier(tier="pro"))   // merged into every later event
observeError("cart unavailable", "CartError")
```

The attributes are a shape constructed in place. These follow Observe's rules:

- an event name is 1–256 characters and doesn't start with `expo.`;
- keys starting with `expo.` and the keys `session.id` and `event.name` are dropped;
- at most 128 attributes are kept.

On Apple, the local `recent` query reports `sent: true` only for rows acknowledged by the server. Rows skipped by configuration or sampling, or rejected without a retry, advance processing without reporting delivery.

The web exporter drops queued rows when configuration, sampling or development mode prevents dispatch. It retains metadata only for the current session and sessions with queued rows. The tabs of an app share one queue in `localStorage`; one tab at a time sends it, holding a Web Lock, so each row is sent once.

On Linux and Android, the processes of an app share one queue file; each changes it under a file lock, and one process at a time sends it. Android keeps the queue, install ID and pending panic records under its app cache directory; Linux uses `XDG_STATE_HOME`, defaulting to `~/.local/state`. Startup has a 30-second deadline; a pending activation or resource reports a timeout without a TTI value. Android's render and interactive marks wait for the reader's frame-presentation acknowledgements. Its APK's SHA-256 identifies the build for cold and warm launches. The exporter subscribes after startup and replays the journal's retained events. Headless runs keep the bounded in-memory journal but do not start the exporter or emit startup metrics. Crash capture records Rust panics, not fatal signals. Refused dispatches drop these hosts' pending rows; Apple retains rows with `sent: false`.

## Feedback

```
observeFeedback("checkout", message)                      // a message only
observeFeedback("checkout", message, 8)                   // a rating on 0 to 10
observeFeedback("checkout", message, stars, 1, 5, Context(screen=s))
```

`observeFeedback(id, message[, rating[, min, max]][, Shape(key=value)])` sends what expo-observe's `Observe.reportFeedback` sends: an `expo.feedback` log, info severity, whose body is the message. The id says where in the app the feedback was collected; Observe groups reports by it. The app draws the form.

- the id must not be blank, and a report needs a message or a rating; a blank message is none;
- a rating is a whole number within its scale, and the scale's ends are whole numbers with 0 ≤ min < max ≤ 10 (0 to 10 when left out); a rating that is not a finite number is none;
- the message is cut to 4096 characters, and the attributes follow the rules above;
- `expo.feedback.id`, `expo.feedback.rating`, `expo.feedback.scale.min` and `expo.feedback.scale.max` are added after the app's attributes, so none can replace them.

A report that breaks a rule is dropped with a line on stderr (the console on the web). Feedback is sent outside the `sampleRate` sample too, as expo-observe sends it; `dispatchingEnabled: false` and a development build without `dispatchInDebug` drop it. The text can hold personal data: declare it in the iOS privacy manifest (`NSPrivacyCollectedDataTypeOtherUserContent`), the App Store privacy label and the Google Play Data safety form.

## Tests

`modules/observe/tests/wire.json` is one fixture that all three encoders must produce:
`bun test ./modules/observe/tests/wire.test.mjs` checks Swift and JS. `bun test ./modules/observe/tests/tabs.test.mjs` runs the web service in two tabs of one Chromium profile. `cargo test -p exact-painted --test observe` checks Rust, including dispatch, retry, sampling and panic recovery.
