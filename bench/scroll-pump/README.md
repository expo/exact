# Scroll Pump Probe

Run this iOS simulator benchmark to check that clipped text leaves Exact’s display link idle during a native scroll. The probe changes a title beyond the outer scrollport’s prefetch reach, flings a visible strip twice in each direction, and checks that the new title appears when revealed. Each fling must lift inside the scroll range, coast past an edge, and settle. The build before #2047 must wake an app-owned display link after every lift. The candidate must keep those links idle throughout the measured gestures.

```sh
bun bench/scroll-pump/run.mjs
```

Use macOS with Bun, Xcode, and an installed iOS Simulator runtime that supports iPhone 18 Pro. The command builds both hosts from source, caches the calibration bundle for the fixture and SDK, and starts the candidate build during setup. It uses its own simulator and deletes it when done. To reuse that simulator and its XCTest runner for a series of runs, add `--reuse-simulator`. Run `bun bench/scroll-pump/run.mjs --stop-simulator` to release both.

Pass two paths to compare existing simulator bundles of this fixture. The first is the calibration build and the second is the candidate. `EXACT=/path/to/checkout` selects another Exact checkout for a source build.

Each run prints its artifact directory under `bench/scroll-pump/.exact/scroll-check/`. It records motion samples, executable and build-receipt hashes, and time spent building, starting, and driving the apps. An unqualified gesture gets one fresh launch of its arm. If it fails again, the command exits with status 2. Build, setup, and display-link failures exit with status 1. The first failure is recorded in `failure.json` before cleanup begins.
