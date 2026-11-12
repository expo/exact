# `host/apple/touch.mjs` is dead: nothing runs it, its simulator path throws and its presenter mode cannot build

**Status:** Closed
**Resolution:** `host/apple/touch.mjs` and `host/apple/ios/touch.swift` are deleted; LLP 1012 keeps what the reduction found and `scripts/smoke-touch.mjs` drives real touches.
**Systems:** host/apple/touch.mjs, host/apple/ios/touch.swift, host/apple/build.mjs (`install`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`host/apple/touch.mjs` (86 lines, with `host/apple/ios/touch.swift`, 325 lines) is the reduction of a real-finger crash, approved as a diagnostic on 2026-09-07 (LLP 1012). The crash was fixed the same day: sessions are created in `application(_:didFinishLaunchingWithOptions:)` (LLP 1012, "Targeted fix"). The script has not been kept up with the build since.

Without `--device` it installs its bundle with `build.mjs`'s `install` (`touch.mjs:79`):

```js
install(dev, bundle);
```

`install` now takes the app and checks the executable's baked identity (`host/apple/build.mjs:282-284`):

```js
export function install(dev, bundle, app, host = false, said = false, executable = host ? 'ExactHostIOS' : executableName(app)) {
  …
  assertAppleIdentity(app, resolve(bundle, executable));
```

With no `app`, the default for `executable` throws. Passing the app and the executable's name does not help: the default mode links no Exact archive, so the executable has no baked identity and `assertAppleIdentity` refuses it.

`--presenter` defaults its archive to `target/<target>/release/libcaltrain_ts_apple.a` (`touch.mjs:52`), a name no crate in the repository produces. It also compiles the files under `Sources/ExactKit` alone in one `swiftc` call, though 32 of them import `ExactCore`, a separate module of the package.

No script or test runs `touch.mjs` (`scripts/smoke-touch.mjs`, which `async.mjs` and `affected.mjs` name, is another file). LLP 1080.000 §6 plans a rerun of it with `--touches` and `--late`, flags it never got.

## The failure

```
bun host/apple/touch.mjs
```

Observed: after compiling and booting a simulator, `TypeError: undefined is not an object (evaluating 'app.displayName')`. Expected: the bundle installed, as its header says.

## Verification

Run: `install` from `host/apple/build.mjs`, called as `touch.mjs` calls it and then as it would have to be called, on a bundle whose executable links no Exact archive:

```
install(dev, bundle): TypeError: undefined is not an object (evaluating 'app.displayName')
install(dev, bundle, app, false, true, 'TouchRepro'): Error: <bundle>/TouchRepro embedded app identity is missing, expected com.exact.touch-repro
```

Not run: `touch.mjs` itself (it boots a simulator), or its `--device` path, which does not call `install`. The presenter mode is read.

## Direction

Delete `host/apple/touch.mjs` and `host/apple/ios/touch.swift`. LLP 1012 keeps what the reduction found, and `scripts/smoke-touch.mjs` drives real touches now. Drop or reword LLP 1080.000's stage 4, which names the script.

## Related

Nothing in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md` names `touch.mjs` or `TouchRepro`.
