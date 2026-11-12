# An iOS build with Live Activities started outside the checkout cannot find the activity arm's Swift sources

**Status:** Closed
**Resolution:** Fixed by 2917ed57, which resolves the arm's Swift sources from the checkout's root and tests a build run from an app's own directory.
**Systems:** host/apple/activities.mjs, host/apple/bundle.mjs, scripts/app-cli.mjs
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`packageActivityArm` names its two Swift sources with a one-argument `resolve` (`host/apple/activities.mjs:12`):

```js
resolve('host/apple/Sources/ExactSnapshot/ActivityEnvelope.swift'), resolve('host/apple/activityarm/ActivityArm.swift'), '-o', binary]);
```

`path.resolve` with one relative argument resolves against the process's working directory. Every other source path in the Apple build is resolved against the checkout (`resolve(root, 'host/apple/…')`); this file declares no `root`. `runApple` gives the child `cwd: root`, but the paths are already absolute by then.

The working directory is the checkout only when the build is started there. An app `exact new` made runs `bun exact.mjs ios` in its own directory, and `scripts/app-cli.mjs:206-223` spawns `host/apple/build.mjs` with no `cwd`. `host/apple/bundle.mjs:232` calls `packageActivityArm` for every iOS app whose manifest has `live_activities`.

## The failure

In an app outside the checkout whose `app.json` declares a `live_activities` entry:

```
cd ~/my-app && bun exact.mjs ios
```

Observed: after the app's Rust and Swift have compiled, swiftc is given `~/my-app/host/apple/Sources/ExactSnapshot/ActivityEnvelope.swift` and `~/my-app/host/apple/activityarm/ActivityArm.swift`, which do not exist, and the build fails. Expected: the arm compiles from the checkout's sources.

## Verification

Run. `apps/activity-fixture`, built for the simulator from a directory `<dir>` outside the checkout:

```
cd <dir> && bun <checkout>/host/apple/build.mjs --ios activity-fixture-apple --no-install
…
<unknown>:0: error: error opening input file '<dir>/host/apple/Sources/ExactSnapshot/ActivityEnvelope.swift' (No such file or directory)
<unknown>:0: error: error opening input file '<dir>/host/apple/activityarm/ActivityArm.swift' (No such file or directory)
xcrun failed (1)
```

The same command from the checkout exits 0.

In an app made by `bun scripts/exact.mjs new`, `bun exact.mjs ios --no-install` started `<checkout>/host/apple/build.mjs --ios <name>-apple --no-install` with the app's directory as its working directory (`lsof -d cwd` on that process).

Not run: that app's build as far as the arm. The iOS build of an `app.ts` app needs the Hermes bundle for `aarch64-apple-ios-sim`, which the Mac used here does not have.

## Direction

Resolve both paths against the checkout, as `widgets.mjs`, `extensions.mjs` and `bundle.mjs` do (`const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))` at `56e325b8`).

## Related

No issue or QUEUE entry names `ActivityArm` or the build's working directory. `QUEUE.md`'s "Two activities of one app show one at a time on the simulator" is about presentation.
