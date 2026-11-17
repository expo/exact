# Observe

Sends Expo Observe's performance metrics and custom events from an Exact app, under Observe's metric names. It works on iOS, macOS, Linux and the web.

## Turning it on

Add these fields to `app.json`:

```json
"launch": ["observe"],
"moduleConfig": { "observe": { "projectId": "<EAS project id>" } }
```

These settings are optional:

- `endpoint` (default `https://o.expo.dev`);
- `sampleRate` (0–1, decided per install);
- `dispatchingEnabled`;
- `dispatchInDebug`. A development build sends nothing unless this is true.

On Apple, the launch part installs fatal-signal handlers, starts a network-path monitor and enables battery monitoring on iOS. Its deferred service loads after startup; a direct native query can load it on demand. Pending crash records live in the app's Application Support directory.

## Metrics

| Metric | From | To |
|---|---|---|
| `expo.app_startup.cold_launch_time` / `warm_launch_time` | process start | launch end: didFinishLaunching on Apple, the display opening on Linux |
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

On Linux, the processes of an app share one queue file; each changes it under a file lock, and one process at a time sends it. Startup has a 30-second deadline; a pending activation or resource reports a timeout without a TTI value. The exporter subscribes after startup and replays the journal's retained events. Headless runs keep the bounded in-memory journal but do not start the exporter or emit startup metrics. Crash capture records Rust panics, not fatal signals. Refused dispatches drop Linux's pending rows; Apple retains rows with `sent: false`.

## Tests

`modules/observe/tests/wire.json` is one fixture that all three encoders must produce:
`bun test ./modules/observe/tests/wire.test.mjs` checks Swift and JS. `bun test ./modules/observe/tests/tabs.test.mjs` runs the web service in two tabs of one Chromium profile. `cargo test -p exact-painted --test observe` checks Rust, including dispatch, retry, sampling and panic recovery.
